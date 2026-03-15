use crate::AppState;
use crate::features::mail::outlook::OutlookManager;
use crate::storage::{TokenData, TokenStorage};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use tauri::State;
use tokio::sync::{Semaphore, oneshot};
use uuid::Uuid;
use warp::http::{HeaderMap, Method};
use warp::{Filter, Rejection, Reply};

// ==================== 数据结构定义 ====================

/// 单个 session 导入请求
#[derive(Debug, Deserialize)]
pub struct ImportSessionRequest {
    pub session: String,
    #[serde(default = "default_detailed_response")]
    pub detailed_response: bool,
}

/// 批量 session 导入请求
#[derive(Debug, Deserialize)]
pub struct ImportSessionsRequest {
    pub sessions: Vec<String>,
    #[serde(default = "default_detailed_response")]
    pub detailed_response: bool,
}

/// 默认返回详细响应
fn default_detailed_response() -> bool {
    true
}

/// 单个导入结果
#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_data: Option<TokenData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_preview: Option<String>,
}

/// 批量导入结果
#[derive(Debug, Serialize)]
pub struct BatchImportResult {
    pub total: usize,
    pub successful: usize,
    pub failed: usize,
    pub results: Vec<ImportResult>,
}

/// 健康检查响应
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub port: u16,
}

/// API 错误响应
#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    pub error: String,
    pub code: String,
}

/// API 服务器状态响应
#[derive(Debug, Serialize)]
pub struct ApiServerStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub address: Option<String>,
}

// API 服务器管理命令
#[tauri::command]
pub async fn get_api_server_status(state: State<'_, AppState>) -> Result<ApiServerStatus, String> {
    let server_guard = state.api_server.lock().unwrap();

    if let Some(server) = server_guard.as_ref() {
        let port = server.get_port();
        Ok(ApiServerStatus {
            running: true,
            port: Some(port),
            address: Some(format!("http://127.0.0.1:{}", port)),
        })
    } else {
        Ok(ApiServerStatus {
            running: false,
            port: None,
            address: None,
        })
    }
}

#[tauri::command]
pub async fn start_api_server_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    {
        let server_guard = state.api_server.lock().unwrap();
        if server_guard.is_some() {
            return Err("API server is already running".to_string());
        }
    }

    let server = start_api_server(
        Arc::new(AppState {
            augment_oauth_state: Mutex::new(None),
            openai_oauth_sessions: state.openai_oauth_sessions.clone(),
            api_server: Mutex::new(None),
            outlook_manager: Mutex::new(OutlookManager::new()),
            hme_cookie: state.hme_cookie.clone(),
            hme_storage: state.hme_storage.clone(),
            storage_manager: state.storage_manager.clone(),
            antigravity_storage_manager: state.antigravity_storage_manager.clone(),
            windsurf_storage_manager: state.windsurf_storage_manager.clone(),
            cursor_storage_manager: state.cursor_storage_manager.clone(),
            openai_storage_manager: state.openai_storage_manager.clone(),
            claude_storage_manager: state.claude_storage_manager.clone(),
            subscription_storage_manager: state.subscription_storage_manager.clone(),
            database_manager: state.database_manager.clone(),
            app_session_cache: state.app_session_cache.clone(),
            app_handle: state.app_handle.clone(),
            codex_pool: state.codex_pool.clone(),
            codex_executor: state.codex_executor.clone(),
            codex_logger: state.codex_logger.clone(),
            codex_server: state.codex_server.clone(),
            codex_unsupported_params: state.codex_unsupported_params.clone(),
            codex_server_config: state.codex_server_config.clone(),
            gateway_access_profiles: state.gateway_access_profiles.clone(),
            codex_log_storage: state.codex_log_storage.clone(),
            codex_archive_storage: state.codex_archive_storage.clone(),
            proxy_config: state.proxy_config.clone(),
            augment_sidecar: state.augment_sidecar.clone(),
            antigravity_sidecar: state.antigravity_sidecar.clone(),
        }),
        8766,
    )
    .await?;

    *state.api_server.lock().unwrap() = Some(server);

    // 初始化 Codex 状态
    if let Err(err) =
        crate::platforms::openai::codex::commands::init_codex_enabled_state_on_startup(
            &app,
            state.inner(),
        )
        .await
    {
        eprintln!("Failed to initialize Codex enabled state: {}", err);
    }

    // 通知前端 API 服务器状态变化
    let _ = app.emit("api-server-status-changed", true);

    Ok(())
}

#[tauri::command]
pub async fn stop_api_server(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let server = {
        let mut server_guard = state.api_server.lock().unwrap();
        server_guard.take()
    };

    if let Some(mut server) = server {
        server.shutdown();
        stop_managed_augment_sidecar(&state.augment_sidecar).await;
        stop_managed_antigravity_sidecar(&state.antigravity_sidecar).await;
        // 同步清除 Codex 服务器状态
        *state.codex_server.lock().unwrap() = None;
        println!("🛑 API Server stopped");

        // 通知前端 API 服务器状态变化
        let _ = app.emit("api-server-status-changed", false);

        Ok(())
    } else {
        Err("API server is not running".to_string())
    }
}

pub(crate) async fn stop_managed_augment_sidecar(
    managed_sidecar: &tokio::sync::Mutex<
        Option<crate::platforms::augment::sidecar::AugmentSidecar>,
    >,
) {
    let sidecar = {
        let mut guard = managed_sidecar.lock().await;
        guard.take()
    };

    if let Some(mut sidecar) = sidecar {
        sidecar.stop().await;
        let mut guard = managed_sidecar.lock().await;
        *guard = Some(sidecar);
    }
}

pub(crate) async fn stop_managed_antigravity_sidecar(
    managed_sidecar: &tokio::sync::Mutex<
        Option<crate::platforms::antigravity::sidecar::AntigravitySidecar>,
    >,
) {
    let sidecar = {
        let mut guard = managed_sidecar.lock().await;
        guard.take()
    };

    if let Some(mut sidecar) = sidecar {
        sidecar.stop().await;
        let mut guard = managed_sidecar.lock().await;
        *guard = Some(sidecar);
    }
}

pub(crate) fn stop_managed_augment_sidecar_blocking(
    managed_sidecar: &tokio::sync::Mutex<
        Option<crate::platforms::augment::sidecar::AugmentSidecar>,
    >,
) {
    let mut guard = managed_sidecar.blocking_lock();
    if let Some(sidecar) = guard.as_mut() {
        sidecar.force_stop();
    }
}

pub(crate) fn stop_managed_antigravity_sidecar_blocking(
    managed_sidecar: &tokio::sync::Mutex<
        Option<crate::platforms::antigravity::sidecar::AntigravitySidecar>,
    >,
) {
    let mut guard = managed_sidecar.blocking_lock();
    if let Some(sidecar) = guard.as_mut() {
        sidecar.force_stop();
    }
}

/// 简化导入响应
#[derive(Debug, Serialize)]
pub struct SimpleImportResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

// ==================== API 服务器结构 ====================

pub struct ApiServer {
    shutdown_tx: Option<oneshot::Sender<()>>,
    port: u16,
}

impl ApiServer {
    #[allow(dead_code)]
    pub fn new(port: u16) -> Self {
        ApiServer {
            port,
            shutdown_tx: None,
        }
    }

    pub fn get_port(&self) -> u16 {
        self.port
    }

    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
            println!("🛑 API Server shutdown signal sent");
        }
    }
}

impl Drop for ApiServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ==================== 辅助函数 ====================

/// 脱敏 session 字符串（只显示前4位和后1位）
fn mask_session(session: &str) -> String {
    if session.len() <= 5 {
        return "***".to_string();
    }
    format!("{}***{}", &session[..4], &session[session.len() - 1..])
}

/// 验证 session 格式
fn validate_session(session: &str) -> Result<(), String> {
    if session.trim().is_empty() {
        return Err("Session cannot be empty".to_string());
    }
    if session.len() < 10 {
        return Err("Session is too short".to_string());
    }
    Ok(())
}

fn optional_raw_query()
-> impl Filter<Extract = (Option<String>,), Error = std::convert::Infallible> + Clone {
    warp::query::raw()
        .map(Some)
        .or(warp::any().map(|| None))
        .unify()
}

fn extract_gateway_bearer_token(header: &str) -> Option<&str> {
    let trimmed = header.trim();
    if trimmed.len() < 7 {
        return None;
    }

    let (scheme, rest) = trimmed.split_at(7);
    if !scheme.eq_ignore_ascii_case("bearer ") {
        return None;
    }

    let token = rest.trim();
    if token.is_empty() { None } else { Some(token) }
}

fn resolve_gateway_profile_from_profiles(
    profiles: &crate::core::gateway_access::GatewayAccessProfiles,
    headers: &HeaderMap,
) -> Result<crate::core::gateway_access::GatewayAccessProfile, Rejection> {
    let mut candidates: Vec<&str> = Vec::new();

    if let Some(auth) = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
    {
        if let Some(token) = extract_gateway_bearer_token(auth) {
            candidates.push(token);
        }
    }

    if let Some(api_key) = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        candidates.push(api_key);
    }

    candidates
        .into_iter()
        .find_map(|candidate| profiles.find_by_key(candidate).cloned())
        .ok_or_else(|| {
            warp::reject::custom(
                crate::platforms::openai::codex::server::CodexRejection::ExecutionError(
                    "Unauthorized: invalid gateway API key".to_string(),
                ),
            )
        })
}

fn resolve_gateway_profile(
    state: &Arc<AppState>,
    headers: &HeaderMap,
) -> Result<crate::core::gateway_access::GatewayAccessProfile, Rejection> {
    let profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&state.app_handle, state)
            .map_err(|err| {
                warp::reject::custom(
                    crate::platforms::openai::codex::server::CodexRejection::InternalError(
                        format!("Failed to load gateway access profiles: {}", err),
                    ),
                )
            })?;

    resolve_gateway_profile_from_profiles(&profiles, headers)
}

fn unified_models_request_filter()
-> impl Filter<Extract = (Option<String>, HeaderMap, Bytes), Error = Rejection> + Clone {
    warp::path!("v1" / "models")
        .and(warp::get())
        .and(optional_raw_query())
        .and(warp::header::headers_cloned())
        .map(|query, headers| (query, headers, Bytes::new()))
        .untuple_one()
}

fn unified_responses_request_filter()
-> impl Filter<Extract = (Option<String>, HeaderMap, Bytes), Error = Rejection> + Clone {
    unified_post_request_filter(warp::path!("v1" / "responses"))
}

fn unified_chat_completions_request_filter()
-> impl Filter<Extract = (Option<String>, HeaderMap, Bytes), Error = Rejection> + Clone {
    unified_post_request_filter(warp::path!("v1" / "chat" / "completions"))
}

fn unified_post_request_filter<F>(
    path_filter: F,
) -> impl Filter<Extract = (Option<String>, HeaderMap, Bytes), Error = Rejection> + Clone
where
    F: Filter<Extract = (), Error = Rejection> + Clone + Send + Sync + 'static,
{
    path_filter
        .and(warp::post())
        .and(optional_raw_query())
        .and(warp::header::headers_cloned())
        .and(warp::body::content_length_limit(20 * 1024 * 1024))
        .and(warp::body::bytes())
}

async fn handle_unified_gateway_request(
    raw_path: String,
    method: Method,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
    state: Arc<AppState>,
) -> Result<Box<dyn Reply>, Rejection> {
    let profile = resolve_gateway_profile(&state, &headers)?;

    match profile.target {
        crate::core::gateway_access::GatewayTarget::Codex => {
            crate::platforms::openai::codex::server::handle_unified_gateway_request(
                raw_path,
                method,
                query,
                headers,
                body,
                Some(profile),
                state,
            )
            .await
        }
        crate::core::gateway_access::GatewayTarget::Augment => {
            crate::platforms::augment::proxy_server::handle_unified_gateway_request(
                raw_path,
                method,
                query,
                headers,
                body,
                Some(profile),
                state,
            )
            .await
        }
        crate::core::gateway_access::GatewayTarget::Antigravity => {
            crate::platforms::antigravity::api_service::server::handle_unified_gateway_request(
                raw_path,
                method,
                query,
                headers,
                body,
                Some(profile),
                state,
            )
            .await
        }
    }
}

fn unified_gateway_routes_from_state(
    state: Arc<AppState>,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let state_filter = warp::any().map(move || state.clone());

    let models_route = unified_models_request_filter()
        .and(state_filter.clone())
        .and_then(|query, headers, body, state| {
            handle_unified_gateway_request(
                "/v1/models".to_string(),
                Method::GET,
                query,
                headers,
                body,
                state,
            )
        });

    let responses_route = unified_responses_request_filter()
        .and(state_filter.clone())
        .and_then(|query, headers, body, state| {
            handle_unified_gateway_request(
                "/v1/responses".to_string(),
                Method::POST,
                query,
                headers,
                body,
                state,
            )
        });

    let chat_completions_route = unified_chat_completions_request_filter()
        .and(state_filter)
        .and_then(|query, headers, body, state| {
            handle_unified_gateway_request(
                "/v1/chat/completions".to_string(),
                Method::POST,
                query,
                headers,
                body,
                state,
            )
        });

    models_route.or(responses_route).or(chat_completions_route)
}

// ==================== 路由处理器 ====================

/// 健康检查处理器
async fn health_handler(port: u16) -> Result<impl Reply, Rejection> {
    let response = HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        port,
    };
    Ok(warp::reply::json(&response))
}

/// 单个 session 导入处理器
async fn import_session_handler(
    request: ImportSessionRequest,
    state: Arc<crate::AppState>,
) -> Result<impl Reply, Rejection> {
    println!(
        "📥 API: Importing single session: {}",
        mask_session(&request.session)
    );

    // 验证 session
    if let Err(e) = validate_session(&request.session) {
        let error_response = ApiErrorResponse {
            error: e,
            code: "INVALID_SESSION".to_string(),
        };
        return Ok(warp::reply::with_status(
            warp::reply::json(&error_response),
            warp::http::StatusCode::BAD_REQUEST,
        ));
    }

    // 调用内部函数导入
    match crate::platforms::augment::oauth::add_token_from_session_internal_with_cache(
        &request.session,
        &state,
    )
    .await
    {
        Ok(response) => {
            // 检查重复 email（与前端逻辑保持一致）
            if let Some(ref email_note) = response.email {
                let email_to_check = email_note.trim().to_lowercase();

                // 从 storage_manager 加载现有 tokens
                let storage_manager = {
                    let guard = state.storage_manager.lock().unwrap();
                    guard.clone()
                };

                if let Some(storage) = storage_manager {
                    match storage.load_tokens().await {
                        Ok(existing_tokens) => {
                            // 检查是否存在相同的 email
                            if existing_tokens.iter().any(|token| {
                                if let Some(ref existing_email) = token.email_note {
                                    existing_email.trim().to_lowercase() == email_to_check
                                } else {
                                    false
                                }
                            }) {
                                println!("⚠️  API: Duplicate email detected: {}", email_note);
                                let error_response = ApiErrorResponse {
                                    error: format!(
                                        "Token with email '{}' already exists",
                                        email_note
                                    ),
                                    code: "DUPLICATE_EMAIL".to_string(),
                                };
                                return Ok(warp::reply::with_status(
                                    warp::reply::json(&error_response),
                                    warp::http::StatusCode::CONFLICT,
                                ));
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "⚠️  API: Failed to load existing tokens for duplicate check: {}",
                                e
                            );
                            // 继续导入，不因为加载失败而阻止导入
                        }
                    }
                }
            }

            // 使用 UUID 生成唯一 ID（与前端逻辑保持一致）
            let id = Uuid::new_v4().to_string();

            // 构造 TokenData（与前端逻辑保持一致）
            let now = chrono::Utc::now();
            let token_data = TokenData {
                id,
                tenant_url: response.tenant_url.clone(),
                access_token: response.access_token.clone(),
                created_at: now,
                updated_at: now,
                portal_url: None, // Session 导入不再获取 portal_url
                email_note: response.email.clone(),
                tag_name: None,
                tag_color: None,
                ban_status: Some(serde_json::Value::String("ACTIVE".to_string())), // Session 导入默认设置为 ACTIVE
                portal_info: None, // Session 导入不再获取 portal_info
                auth_session: Some(request.session.clone()),
                suspensions: None, // Session 导入不再获取 suspensions
                balance_color_mode: None,
                skip_check: Some(false), // 与前端保持一致，默认不跳过检测
                session_updated_at: Some(now), // 设置 session 初始更新时间
                version: 0,              // 本地创建时版本号为0，由数据库分配
            };

            // 保存到存储
            let storage = {
                let storage_guard = state.storage_manager.lock().unwrap();
                storage_guard.as_ref().cloned()
            };

            let storage_result = if let Some(storage) = storage {
                storage
                    .save_token(&token_data)
                    .await
                    .map_err(|e| e.to_string())
            } else {
                Err("Storage manager not initialized".to_string())
            };

            match storage_result {
                Ok(_) => {
                    println!("✅ API: Session imported successfully");

                    // 发送前端刷新事件
                    if let Err(e) = state.app_handle.emit("tokens-updated", ()) {
                        eprintln!("⚠️  Failed to emit tokens-updated event: {}", e);
                    }

                    // 根据 detailed_response 参数返回不同格式
                    if request.detailed_response {
                        // 返回完整响应
                        let result = ImportResult {
                            success: true,
                            token_data: Some(token_data),
                            error: None,
                            session_preview: Some(mask_session(&request.session)),
                        };
                        Ok(warp::reply::with_status(
                            warp::reply::json(&result),
                            warp::http::StatusCode::OK,
                        ))
                    } else {
                        // 返回简化响应
                        let result = SimpleImportResult {
                            success: true,
                            message: Some("Session imported successfully".to_string()),
                            error: None,
                            code: None,
                        };
                        Ok(warp::reply::with_status(
                            warp::reply::json(&result),
                            warp::http::StatusCode::OK,
                        ))
                    }
                }
                Err(e) => {
                    eprintln!("❌ API: Failed to save token: {}", e);
                    let error_response = ApiErrorResponse {
                        error: format!("Failed to save token: {}", e),
                        code: "STORAGE_ERROR".to_string(),
                    };
                    Ok(warp::reply::with_status(
                        warp::reply::json(&error_response),
                        warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                    ))
                }
            }
        }
        Err(e) => {
            eprintln!("❌ API: Failed to import session: {}", e);
            let error_response = ApiErrorResponse {
                error: e,
                code: "IMPORT_ERROR".to_string(),
            };
            Ok(warp::reply::with_status(
                warp::reply::json(&error_response),
                warp::http::StatusCode::UNPROCESSABLE_ENTITY,
            ))
        }
    }
}

/// 批量 session 导入处理器
async fn import_sessions_handler(
    request: ImportSessionsRequest,
    state: Arc<crate::AppState>,
) -> Result<impl Reply, Rejection> {
    println!("📥 API: Importing {} sessions", request.sessions.len());

    // 验证请求
    if request.sessions.is_empty() {
        let error_response = ApiErrorResponse {
            error: "Sessions array cannot be empty".to_string(),
            code: "EMPTY_ARRAY".to_string(),
        };
        return Ok(warp::reply::with_status(
            warp::reply::json(&error_response),
            warp::http::StatusCode::BAD_REQUEST,
        ));
    }

    if request.sessions.len() > 100 {
        let error_response = ApiErrorResponse {
            error: "Too many sessions (max 100)".to_string(),
            code: "TOO_MANY_SESSIONS".to_string(),
        };
        return Ok(warp::reply::with_status(
            warp::reply::json(&error_response),
            warp::http::StatusCode::BAD_REQUEST,
        ));
    }

    // 使用 Semaphore 限制并发
    let semaphore = Arc::new(Semaphore::new(5));
    let mut tasks = Vec::new();

    for session in request.sessions.iter() {
        let session = session.clone();
        let state = state.clone();
        let semaphore = semaphore.clone();

        let task = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();

            // 验证 session
            if let Err(e) = validate_session(&session) {
                return ImportResult {
                    success: false,
                    token_data: None,
                    error: Some(e),
                    session_preview: Some(mask_session(&session)),
                };
            }

            // 导入 session
            match crate::platforms::augment::oauth::add_token_from_session_internal_with_cache(
                &session, &state,
            )
            .await
            {
                Ok(response) => {
                    // 检查重复 email（与前端逻辑保持一致）
                    if let Some(ref email_note) = response.email {
                        let email_to_check = email_note.trim().to_lowercase();

                        // 从 storage_manager 加载现有 tokens
                        let storage_manager = {
                            let guard = state.storage_manager.lock().unwrap();
                            guard.clone()
                        };

                        if let Some(storage) = storage_manager {
                            match storage.load_tokens().await {
                                Ok(existing_tokens) => {
                                    // 检查是否存在相同的 email
                                    if existing_tokens.iter().any(|token| {
                                        if let Some(ref existing_email) = token.email_note {
                                            existing_email.trim().to_lowercase() == email_to_check
                                        } else {
                                            false
                                        }
                                    }) {
                                        println!(
                                            "⚠️  API: Duplicate email detected in batch: {}",
                                            email_note
                                        );
                                        return ImportResult {
                                            success: false,
                                            token_data: None,
                                            error: Some(format!(
                                                "Token with email '{}' already exists",
                                                email_note
                                            )),
                                            session_preview: Some(mask_session(&session)),
                                        };
                                    }
                                }
                                Err(e) => {
                                    eprintln!(
                                        "⚠️  API: Failed to load existing tokens for duplicate check: {}",
                                        e
                                    );
                                    // 继续导入，不因为加载失败而阻止导入
                                }
                            }
                        }
                    }

                    // 使用 UUID 生成唯一 ID（与前端逻辑保持一致）
                    let id = Uuid::new_v4().to_string();

                    let now = chrono::Utc::now();
                    let token_data = TokenData {
                        id,
                        tenant_url: response.tenant_url.clone(),
                        access_token: response.access_token.clone(),
                        created_at: now,
                        updated_at: now,
                        portal_url: None, // Session 导入不再获取 portal_url
                        email_note: response.email.clone(),
                        tag_name: None,
                        tag_color: None,
                        ban_status: Some(serde_json::Value::String("ACTIVE".to_string())), // Session 导入默认设置为 ACTIVE
                        portal_info: None, // Session 导入不再获取 portal_info
                        auth_session: Some(session.clone()),
                        suspensions: None, // Session 导入不再获取 suspensions
                        balance_color_mode: None,
                        skip_check: Some(false), // 与前端保持一致，默认不跳过检测
                        session_updated_at: Some(now), // 设置 session 初始更新时间
                        version: 0,              // 本地创建时版本号为0，由数据库分配
                    };

                    // 保存到存储
                    let storage = {
                        let storage_guard = state.storage_manager.lock().unwrap();
                        storage_guard.as_ref().cloned()
                    };

                    let storage_result = if let Some(storage) = storage {
                        storage
                            .save_token(&token_data)
                            .await
                            .map_err(|e| e.to_string())
                    } else {
                        Err("Storage manager not initialized".to_string())
                    };

                    match storage_result {
                        Ok(_) => ImportResult {
                            success: true,
                            token_data: Some(token_data),
                            error: None,
                            session_preview: Some(mask_session(&session)),
                        },
                        Err(e) => ImportResult {
                            success: false,
                            token_data: None,
                            error: Some(format!("Storage error: {}", e)),
                            session_preview: Some(mask_session(&session)),
                        },
                    }
                }
                Err(e) => ImportResult {
                    success: false,
                    token_data: None,
                    error: Some(e),
                    session_preview: Some(mask_session(&session)),
                },
            }
        });

        tasks.push(task);
    }

    // 等待所有任务完成
    let mut results = Vec::new();
    for task in tasks {
        match task.await {
            Ok(result) => results.push(result),
            Err(e) => {
                results.push(ImportResult {
                    success: false,
                    token_data: None,
                    error: Some(format!("Task error: {}", e)),
                    session_preview: None,
                });
            }
        }
    }

    // 统计结果
    let successful = results.iter().filter(|r| r.success).count();
    let failed = results.len() - successful;

    println!(
        "✅ API: Batch import completed - {}/{} successful",
        successful,
        results.len()
    );

    // 如果有成功导入的 token，发送前端刷新事件
    if successful > 0 {
        if let Err(e) = state.app_handle.emit("tokens-updated", ()) {
            eprintln!("⚠️  Failed to emit tokens-updated event: {}", e);
        }
    }

    // 根据 detailed_response 参数返回不同格式
    if request.detailed_response {
        // 返回完整响应
        let batch_result = BatchImportResult {
            total: results.len(),
            successful,
            failed,
            results,
        };
        Ok(warp::reply::with_status(
            warp::reply::json(&batch_result),
            warp::http::StatusCode::OK,
        ))
    } else {
        // 返回简化响应
        let result = SimpleImportResult {
            success: true,
            message: Some(format!(
                "{} of {} sessions imported successfully",
                successful,
                results.len()
            )),
            error: None,
            code: None,
        };
        Ok(warp::reply::with_status(
            warp::reply::json(&result),
            warp::http::StatusCode::OK,
        ))
    }
}

// ==================== 服务器启动 ====================

/// 启动 API 服务器（固定端口）
pub async fn start_api_server(state: Arc<crate::AppState>, port: u16) -> Result<ApiServer, String> {
    println!("🚀 Starting API Server on port {}...", port);

    match try_bind_server(state.clone(), port).await {
        Ok(server) => {
            println!(
                "✅ API Server started successfully on http://127.0.0.1:{}",
                port
            );
            println!("📡 Available endpoints:");
            println!("   - GET  http://127.0.0.1:{}/api/health", port);
            println!("   - POST http://127.0.0.1:{}/api/import/session", port);
            println!("   - POST http://127.0.0.1:{}/api/import/sessions", port);
            Ok(server)
        }
        Err(e) => Err(format!(
            "Failed to start API server on port {}: {}",
            port, e
        )),
    }
}

/// 尝试在指定端口绑定服务器
async fn try_bind_server(state: Arc<crate::AppState>, port: u16) -> Result<ApiServer, String> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    // 克隆 state 用于各个路由
    let state_for_filters = state.clone();
    let state_filter = warp::any().map(move || state_for_filters.clone());
    let port_filter = warp::any().map(move || port);

    // 健康检查路由
    let health_route = warp::path!("api" / "health")
        .and(warp::get())
        .and(port_filter.clone())
        .and_then(health_handler);

    // 单个 session 导入路由
    let import_session_route = warp::path!("api" / "import" / "session")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 1024)) // 1MB 限制
        .and(warp::body::json())
        .and(state_filter.clone())
        .and_then(import_session_handler);

    // 批量 session 导入路由
    let import_sessions_route = warp::path!("api" / "import" / "sessions")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 1024)) // 1MB 限制
        .and(warp::body::json())
        .and(state_filter.clone())
        .and_then(import_sessions_handler);

    // API 子路由
    let api_routes = health_route
        .or(import_session_route)
        .or(import_sessions_route)
        .boxed();

    // 统一 /v1/* 网关路由
    let unified_gateway_routes = unified_gateway_routes_from_state(state.clone()).boxed();

    // Codex 管理路由（健康检查、pool 状态）
    let codex_routes =
        crate::platforms::openai::codex::server::codex_routes_from_state(state.clone()).boxed();

    let cors = warp::cors()
        .allow_any_origin() // 允许任何来源（因为只监听 localhost）
        .allow_methods(vec!["GET", "POST", "OPTIONS"])
        .allow_headers(vec![
            "Content-Type",
            "Authorization",
            "X-API-Key",
            "Accept",
            "Accept-Encoding",
        ]);

    // 组合所有路由
    let routes = api_routes
        .or(unified_gateway_routes)
        .or(codex_routes)
        .with(cors)
        .recover(handle_rejection);

    // 尝试绑定端口
    let (_addr, server) = warp::serve(routes)
        .try_bind_with_graceful_shutdown(([127, 0, 0, 1], port), async {
            shutdown_rx.await.ok();
        })
        .map_err(|e| format!("Failed to bind to port {}: {}", port, e))?;

    // 在后台启动服务器
    tokio::spawn(server);

    Ok(ApiServer {
        shutdown_tx: Some(shutdown_tx),
        port,
    })
}

/// 处理 warp 拒绝错误
pub(crate) async fn handle_rejection(err: Rejection) -> Result<impl Reply, Rejection> {
    if let Some(_) = err.find::<warp::reject::MethodNotAllowed>() {
        let error_response = ApiErrorResponse {
            error: "Method not allowed".to_string(),
            code: "METHOD_NOT_ALLOWED".to_string(),
        };
        Ok(warp::reply::with_status(
            warp::reply::json(&error_response),
            warp::http::StatusCode::METHOD_NOT_ALLOWED,
        ))
    } else if let Some(rej) = err.find::<crate::platforms::openai::codex::server::CodexRejection>()
    {
        let (status, message, code) = match rej {
            crate::platforms::openai::codex::server::CodexRejection::InvalidRequest(msg) => (
                warp::http::StatusCode::BAD_REQUEST,
                msg.as_str(),
                "invalid_request_error",
            ),
            crate::platforms::openai::codex::server::CodexRejection::TranslationError(msg) => (
                warp::http::StatusCode::BAD_REQUEST,
                msg.as_str(),
                "translation_error",
            ),
            crate::platforms::openai::codex::server::CodexRejection::ExecutionError(msg)
                if msg.to_ascii_lowercase().contains("unauthorized") =>
            {
                (
                    warp::http::StatusCode::UNAUTHORIZED,
                    msg.as_str(),
                    "unauthorized",
                )
            }
            crate::platforms::openai::codex::server::CodexRejection::ExecutionError(msg)
                if msg.to_ascii_lowercase().contains("timed out")
                    || msg.to_ascii_lowercase().contains("timeout") =>
            {
                (
                    warp::http::StatusCode::GATEWAY_TIMEOUT,
                    msg.as_str(),
                    "upstream_timeout",
                )
            }
            crate::platforms::openai::codex::server::CodexRejection::ExecutionError(msg) => (
                warp::http::StatusCode::BAD_GATEWAY,
                msg.as_str(),
                "execution_error",
            ),
            crate::platforms::openai::codex::server::CodexRejection::NoAvailableAccount => (
                warp::http::StatusCode::SERVICE_UNAVAILABLE,
                "No available account in pool",
                "no_available_account",
            ),
            crate::platforms::openai::codex::server::CodexRejection::ServiceUnavailable(msg) => (
                warp::http::StatusCode::SERVICE_UNAVAILABLE,
                msg.as_str(),
                "service_unavailable",
            ),
            crate::platforms::openai::codex::server::CodexRejection::InternalError(msg) => (
                warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                msg.as_str(),
                "internal_error",
            ),
        };

        Ok(warp::reply::with_status(
            warp::reply::json(&json!({
                "error": {
                    "message": message,
                    "type": code,
                    "code": status.as_u16().to_string()
                }
            })),
            status,
        ))
    } else if let Some(rej) =
        err.find::<crate::platforms::augment::proxy_server::AugmentProxyRejection>()
    {
        let (status, message, code) = match rej {
            crate::platforms::augment::proxy_server::AugmentProxyRejection::NoAccounts(msg) => (
                warp::http::StatusCode::SERVICE_UNAVAILABLE,
                msg.as_str(),
                "no_augment_accounts",
            ),
            crate::platforms::augment::proxy_server::AugmentProxyRejection::SidecarNotReady(
                msg,
            ) => (
                warp::http::StatusCode::SERVICE_UNAVAILABLE,
                msg.as_str(),
                "sidecar_not_ready",
            ),
            crate::platforms::augment::proxy_server::AugmentProxyRejection::UpstreamError(msg) => (
                warp::http::StatusCode::BAD_GATEWAY,
                msg.as_str(),
                "augment_upstream_error",
            ),
        };
        Ok(warp::reply::with_status(
            warp::reply::json(&json!({
                "error": {
                    "message": message,
                    "type": code,
                    "code": status.as_u16().to_string()
                }
            })),
            status,
        ))
    } else if let Some(rej) =
        err.find::<crate::platforms::antigravity::api_service::server::AntigravityProxyRejection>()
    {
        let (status, message, code) = match rej {
            crate::platforms::antigravity::api_service::server::AntigravityProxyRejection::NoAccounts(msg) => (
                warp::http::StatusCode::SERVICE_UNAVAILABLE,
                msg.as_str(),
                "no_antigravity_accounts",
            ),
            crate::platforms::antigravity::api_service::server::AntigravityProxyRejection::SidecarNotReady(msg) => (
                warp::http::StatusCode::SERVICE_UNAVAILABLE,
                msg.as_str(),
                "antigravity_sidecar_not_ready",
            ),
            crate::platforms::antigravity::api_service::server::AntigravityProxyRejection::UpstreamError(msg) => (
                warp::http::StatusCode::BAD_GATEWAY,
                msg.as_str(),
                "antigravity_upstream_error",
            ),
        };
        Ok(warp::reply::with_status(
            warp::reply::json(&json!({
                "error": {
                    "message": message,
                    "type": code,
                    "code": status.as_u16().to_string()
                }
            })),
            status,
        ))
    } else if err.is_not_found() {
        let error_response = ApiErrorResponse {
            error: "Endpoint not found".to_string(),
            code: "NOT_FOUND".to_string(),
        };
        Ok(warp::reply::with_status(
            warp::reply::json(&error_response),
            warp::http::StatusCode::NOT_FOUND,
        ))
    } else if let Some(_) = err.find::<warp::filters::body::BodyDeserializeError>() {
        let error_response = ApiErrorResponse {
            error: "Invalid JSON body".to_string(),
            code: "INVALID_JSON".to_string(),
        };
        Ok(warp::reply::with_status(
            warp::reply::json(&error_response),
            warp::http::StatusCode::BAD_REQUEST,
        ))
    } else if let Some(_) = err.find::<warp::reject::PayloadTooLarge>() {
        let error_response = ApiErrorResponse {
            error: "Request payload too large (max 1MB)".to_string(),
            code: "PAYLOAD_TOO_LARGE".to_string(),
        };
        Ok(warp::reply::with_status(
            warp::reply::json(&error_response),
            warp::http::StatusCode::PAYLOAD_TOO_LARGE,
        ))
    } else {
        let error_response = ApiErrorResponse {
            error: "Internal server error".to_string(),
            code: "INTERNAL_ERROR".to_string(),
        };
        Ok(warp::reply::with_status(
            warp::reply::json(&error_response),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gateway_access::{GatewayAccessProfile, GatewayAccessProfiles, GatewayTarget};
    use crate::platforms::augment::sidecar::AugmentSidecar;
    use serde_json::Value;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use warp::http::HeaderMap;
    use warp::http::StatusCode;

    #[tokio::test]
    async fn stop_managed_augment_sidecar_keeps_manager_but_stops_runtime() {
        let temp_dir = tempdir().unwrap();
        let managed_sidecar = tokio::sync::Mutex::new(Some(AugmentSidecar::new(
            temp_dir.path(),
            PathBuf::from("/tmp/cliproxy-server"),
        )));

        stop_managed_augment_sidecar(&managed_sidecar).await;

        let guard = managed_sidecar.lock().await;
        let sidecar = guard
            .as_ref()
            .expect("sidecar manager should remain registered");
        assert!(!sidecar.is_running());
    }

    #[test]
    fn stop_managed_augment_sidecar_blocking_keeps_manager_registered() {
        let temp_dir = tempdir().unwrap();
        let managed_sidecar = tokio::sync::Mutex::new(Some(AugmentSidecar::new(
            temp_dir.path(),
            PathBuf::from("/tmp/cliproxy-server"),
        )));

        stop_managed_augment_sidecar_blocking(&managed_sidecar);

        let guard = managed_sidecar.blocking_lock();
        let sidecar = guard
            .as_ref()
            .expect("sidecar manager should remain registered");
        assert!(!sidecar.is_running());
        assert!(sidecar.api_key().starts_with("sk-atm-internal-"));
    }

    #[tokio::test]
    async fn handle_rejection_maps_augment_no_accounts_to_503() {
        let route = warp::path!("augment" / "v1" / "models")
            .and_then(|| async {
                Err::<warp::reply::Response, Rejection>(warp::reject::custom(
                    crate::platforms::augment::proxy_server::AugmentProxyRejection::NoAccounts(
                        "No available Augment accounts".to_string(),
                    ),
                ))
            })
            .recover(handle_rejection);

        let response = warp::test::request()
            .method("GET")
            .path("/augment/v1/models")
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body["error"]["type"], "no_augment_accounts");
        assert_eq!(body["error"]["message"], "No available Augment accounts");
        assert_eq!(body["error"]["code"], "503");
    }

    #[tokio::test]
    async fn handle_rejection_maps_augment_upstream_error_to_502() {
        let route = warp::path!("augment" / "v1" / "responses")
            .and_then(|| async {
                Err::<warp::reply::Response, Rejection>(warp::reject::custom(
                    crate::platforms::augment::proxy_server::AugmentProxyRejection::UpstreamError(
                        "Failed to forward to sidecar".to_string(),
                    ),
                ))
            })
            .recover(handle_rejection);

        let response = warp::test::request()
            .method("POST")
            .path("/augment/v1/responses")
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body["error"]["type"], "augment_upstream_error");
        assert_eq!(body["error"]["message"], "Failed to forward to sidecar");
        assert_eq!(body["error"]["code"], "502");
    }

    fn gateway_test_profiles() -> GatewayAccessProfiles {
        GatewayAccessProfiles {
            profiles: vec![
                GatewayAccessProfile {
                    id: "codex-default".into(),
                    name: "Codex Default".into(),
                    target: GatewayTarget::Codex,
                    api_key: "sk-codex".into(),
                    enabled: true,
                    member_code: None,
                    role_title: None,
                    persona_summary: None,
                    color: None,
                    notes: None,
                },
                GatewayAccessProfile {
                    id: "augment-default".into(),
                    name: "Augment Default".into(),
                    target: GatewayTarget::Augment,
                    api_key: "sk-augment".into(),
                    enabled: true,
                    member_code: None,
                    role_title: None,
                    persona_summary: None,
                    color: None,
                    notes: None,
                },
                GatewayAccessProfile {
                    id: "antigravity-jdd".into(),
                    name: "JDD Antigravity".into(),
                    target: GatewayTarget::Antigravity,
                    api_key: "sk-ant-jdd".into(),
                    enabled: true,
                    member_code: Some("jdd".into()),
                    role_title: None,
                    persona_summary: None,
                    color: None,
                    notes: None,
                },
            ],
        }
    }

    fn build_unified_gateway_test_route(
        profiles: GatewayAccessProfiles,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
        let profiles_filter = warp::any().map(move || profiles.clone());

        warp::path!("v1" / "models")
            .and(warp::get())
            .and(warp::header::headers_cloned())
            .and(profiles_filter)
            .and_then(
                |headers: HeaderMap, profiles: GatewayAccessProfiles| async move {
                    let profile =
                        resolve_gateway_profile_from_profiles(&profiles, &headers)?;
                    match profile.target {
                        GatewayTarget::Codex => Result::<_, Rejection>::Ok(
                            warp::reply::json(&json!({"backend": "codex"})),
                        ),
                        GatewayTarget::Augment => Err(warp::reject::custom(
                            crate::platforms::augment::proxy_server::AugmentProxyRejection::NoAccounts(
                                "No available Augment accounts".to_string(),
                            ),
                        )),
                        GatewayTarget::Antigravity => Result::<_, Rejection>::Ok(
                            warp::reply::json(&json!({"backend": "antigravity"})),
                        ),
                    }
                },
            )
            .recover(handle_rejection)
    }

    #[tokio::test]
    async fn unified_gateway_routes_codex_key_to_codex_backend() {
        let route = build_unified_gateway_test_route(gateway_test_profiles());

        let response = warp::test::request()
            .method("GET")
            .path("/v1/models")
            .header("authorization", "Bearer sk-codex")
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body["backend"], "codex");
    }

    #[tokio::test]
    async fn unified_gateway_routes_augment_key_to_augment_backend() {
        let route = build_unified_gateway_test_route(gateway_test_profiles());

        let response = warp::test::request()
            .method("GET")
            .path("/v1/models")
            .header("x-api-key", "sk-augment")
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body["error"]["type"], "no_augment_accounts");
    }

    #[tokio::test]
    async fn unified_gateway_routes_antigravity_key_to_antigravity_backend() {
        let route = build_unified_gateway_test_route(gateway_test_profiles());

        let response = warp::test::request()
            .method("GET")
            .path("/v1/models")
            .header("authorization", "Bearer sk-ant-jdd")
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body["backend"], "antigravity");
    }

    #[tokio::test]
    async fn unified_gateway_rejects_unknown_api_key() {
        let route = build_unified_gateway_test_route(gateway_test_profiles());

        let response = warp::test::request()
            .method("GET")
            .path("/v1/models")
            .header("authorization", "Bearer sk-missing")
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body["error"]["type"], "unauthorized");
        assert_eq!(
            body["error"]["message"],
            "Unauthorized: invalid gateway API key"
        );
    }
}
