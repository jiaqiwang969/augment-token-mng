//! Codex API 服务器（透传模式）
//!
//! 提供本地网关入口，做最小处理后将请求直接透传到 ChatGPT Codex 上游。

use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use hyper::{Body, Response};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::http::{HeaderMap, Method, StatusCode};
use warp::{Filter, Rejection, Reply};

use super::{
    archive::{
        ArchiveTurnCapture, ArchiveTurnContext, ArchiveUsageStats,
        derive_archive_session_identity_from_markers, extract_explicit_session_id,
        extract_prompt_cache_key, extract_turn_metadata,
    },
    archive_storage::CodexArchiveStorage,
    executor::{CodexExecutor, ForwardMeta, ForwardRequest},
    logger::RequestLogger,
    models::{CodexError, RequestLog},
    pool::CodexPool,
    storage::CodexLogStorage,
};
use crate::AppState;
use crate::core::gateway_access::GatewayAccessProfile;
use crate::data::storage::common::traits::AccountStorage;

// ==================== 不支持参数缓存 ====================

const UNSUPPORTED_PARAMS_FILE: &str = "codex_unsupported_params.json";

/// 已知的不支持参数，首次启动时预置
const BUILTIN_UNSUPPORTED_PARAMS: &[&str] = &[
    "max_output_tokens",
    "prompt_cache_retention",
    "safety_identifier",
];

/// 缓存 ChatGPT Codex 后端不支持的请求参数。
/// 内存中用 HashSet 做快速查询，同时持久化到 JSON 文件，重启后自动恢复。
pub struct UnsupportedParamCache {
    params: RwLock<HashSet<String>>,
    file_path: PathBuf,
}

impl UnsupportedParamCache {
    /// 从 JSON 文件加载已知的不支持参数，文件不存在则用内置列表初始化
    pub fn load(app_data_dir: &std::path::Path) -> Self {
        let file_path = app_data_dir.join(UNSUPPORTED_PARAMS_FILE);
        let mut params = if file_path.exists() {
            std::fs::read_to_string(&file_path)
                .ok()
                .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
                .map(|v| v.into_iter().collect::<HashSet<String>>())
                .unwrap_or_default()
        } else {
            HashSet::new()
        };

        // 合并内置的已知不支持参数
        let mut dirty = false;
        for &p in BUILTIN_UNSUPPORTED_PARAMS {
            if params.insert(p.to_string()) {
                dirty = true;
            }
        }

        // 有新增则写回磁盘
        if dirty {
            let vec: Vec<&String> = params.iter().collect();
            if let Ok(json) = serde_json::to_string_pretty(&vec) {
                let _ = std::fs::create_dir_all(app_data_dir);
                let _ = std::fs::write(&file_path, json);
            }
        }

        if !params.is_empty() {
            println!(
                "[Codex] Loaded {} cached unsupported params: {:?}",
                params.len(),
                params
            );
        }
        Self {
            params: RwLock::new(params),
            file_path,
        }
    }

    /// 添加一个不支持的参数，同时写入磁盘
    pub async fn add(&self, param: String) {
        let mut set = self.params.write().await;
        if set.insert(param.clone()) {
            println!("[Codex] Caching unsupported param: {}", param);
            let vec: Vec<&String> = set.iter().collect();
            if let Ok(json) = serde_json::to_string_pretty(&vec) {
                let _ = std::fs::write(&self.file_path, json);
            }
        }
    }

    /// 从 JSON body 中移除所有已知的不支持参数，返回清理后的 body（None 表示无需修改）
    pub async fn strip_known_params(&self, body: &Bytes) -> Option<Bytes> {
        let set = self.params.read().await;
        if set.is_empty() {
            return None;
        }
        let mut root: Value = serde_json::from_slice(body).ok()?;
        let obj = root.as_object_mut()?;
        let mut modified = false;
        for param in set.iter() {
            if obj.remove(param).is_some() {
                modified = true;
            }
        }
        if modified {
            serde_json::to_vec(&root).ok().map(Bytes::from)
        } else {
            None
        }
    }
}

/// Codex API 服务器
pub struct CodexServer {
    port: u16,
}

impl CodexServer {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct UsageStats {
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
}

/// Codex API 路由
pub fn codex_routes_from_state(
    state: Arc<AppState>,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let state_filter = warp::any().map(move || state.clone());

    // 健康检查
    let health = warp::path!("health")
        .and(warp::get())
        .and(state_filter.clone())
        .map(|state: Arc<AppState>| {
            warp::reply::json(&serde_json::json!({
                "status": "ok",
                "service": "codex-api",
                "enabled": state.codex_server.lock().unwrap().is_some(),
            }))
        });

    // GET /pool/status
    let pool_status = warp::path!("pool" / "status")
        .and(warp::get())
        .and(state_filter.clone())
        .and_then(|state: Arc<AppState>| async move {
            ensure_codex_enabled(&state)?;
            let (pool, _, _, _, _) = get_runtime_or_reject(&state)?;
            Result::<_, Rejection>::Ok(warp::reply::json(&pool.status().await))
        });

    health.or(pool_status)
}

pub(crate) async fn handle_unified_gateway_request(
    path: String,
    method: Method,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
    gateway_profile: Option<GatewayAccessProfile>,
    state: Arc<AppState>,
) -> Result<Box<dyn Reply>, Rejection> {
    ensure_codex_enabled(&state)?;

    if method == Method::GET && path == "/v1/models" {
        return Ok(Box::new(warp::reply::json(&get_models())) as Box<dyn Reply>);
    }

    handle_passthrough_internal(
        path,
        method,
        query,
        headers,
        body,
        gateway_profile,
        state,
        false,
    )
    .await
}

async fn handle_passthrough_internal(
    path: String,
    method: Method,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
    gateway_profile: Option<GatewayAccessProfile>,
    state: Arc<AppState>,
    validate_key_first: bool,
) -> Result<Box<dyn Reply>, Rejection> {
    println!("[Codex Server] Incoming request: {} {}", method, path);
    if !is_supported_proxy_path(&path) {
        return Err(warp::reject::not_found());
    }

    ensure_codex_enabled(&state)?;
    if validate_key_first {
        validate_api_key(&state, &headers)?;
    }

    let (pool, executor, logger, storage, archive_storage) = get_runtime_or_reject(&state)?;
    let request_format = infer_request_format(&path).to_string();
    let request_model =
        extract_model_from_json_bytes(&body).unwrap_or_else(|| "unknown".to_string());
    let mut archive_capture = build_archive_turn_capture(
        &path,
        &method,
        &headers,
        &body,
        &request_format,
        &request_model,
        gateway_profile.as_ref(),
    );

    let is_responses = request_format == "openai-responses";

    let (mut body, stream_forced) = if is_responses {
        let result = normalize_responses_body(&body);
        (result.body.unwrap_or(body), result.stream_forced)
    } else {
        (body, false)
    };

    // 用缓存移除已知的不支持参数
    if is_responses {
        if let Some(stripped) = state
            .codex_unsupported_params
            .strip_known_params(&body)
            .await
        {
            body = stripped;
        }
    }

    // 最多重试 MAX_UNSUPPORTED_PARAM_RETRIES 次以处理未知的不支持参数
    const MAX_UNSUPPORTED_PARAM_RETRIES: usize = 5;
    let mut retries = 0;

    loop {
        let forward_request = ForwardRequest {
            method: method.clone(),
            path: path.clone(),
            query: query.clone(),
            headers: headers.clone(),
            body: body.clone(),
            format: request_format.clone(),
            model: request_model.clone(),
        };

        let (upstream_response, meta) = match executor.forward(forward_request).await {
            Ok(ok) => ok,
            Err(err) => {
                let is_no_account = matches!(err, CodexError::NoAvailableAccount);
                let err_text = err.to_string();
                add_failed_log(
                    logger.clone(),
                    storage.clone(),
                    &request_model,
                    &request_format,
                    err_text.clone(),
                    gateway_profile.as_ref(),
                )
                .await;

                let rejection = if is_no_account {
                    CodexRejection::NoAvailableAccount
                } else {
                    CodexRejection::ExecutionError(err_text)
                };
                return Err(warp::reject::custom(rejection));
            }
        };

        let upstream_status = StatusCode::from_u16(upstream_response.status().as_u16())
            .unwrap_or(StatusCode::BAD_GATEWAY);
        let upstream_headers = upstream_response.headers().clone();
        archive_capture.set_selected_account(meta.account_id.clone(), meta.account_email.clone());
        archive_capture.set_response_headers(&upstream_headers);

        // 如果是 402/403，异步更新数据库中的 forbidden 状态
        if upstream_status == StatusCode::PAYMENT_REQUIRED
            || upstream_status == StatusCode::FORBIDDEN
        {
            let state_clone = state.clone();
            let account_id = meta.account_id.clone();
            tokio::spawn(async move {
                mark_account_forbidden(&state_clone, &account_id).await;
            });
        }

        // 对于非流式响应，检查是否包含 "Unsupported parameter" 错误并自动重试
        if is_responses
            && retries < MAX_UNSUPPORTED_PARAM_RETRIES
            && !is_event_stream(&upstream_headers)
        {
            let peek_bytes = match upstream_response.bytes().await {
                Ok(b) => b,
                Err(err) => {
                    pool.record_failure(&meta.account_id, None).await;
                    let err_text = format!("Failed to read upstream response body: {}", err);
                    if let Some(archive_store) = archive_storage.as_ref() {
                        let _ = archive_capture.finish_response(
                            "error",
                            Some(err_text.clone()),
                            ArchiveUsageStats::default(),
                            archive_store.as_ref(),
                        );
                    }
                    add_failed_log(
                        logger,
                        storage,
                        &request_model,
                        &request_format,
                        err_text.clone(),
                        gateway_profile.as_ref(),
                    )
                    .await;
                    return Err(warp::reject::custom(CodexRejection::ExecutionError(
                        err_text,
                    )));
                }
            };

            if let Some(param) = extract_unsupported_param(&peek_bytes) {
                println!(
                    "[Codex] Upstream rejected unsupported param '{}', stripping and retrying ({}/{})",
                    param,
                    retries + 1,
                    MAX_UNSUPPORTED_PARAM_RETRIES
                );
                state.codex_unsupported_params.add(param.clone()).await;
                body = remove_json_key(&body, &param);
                retries += 1;
                continue;
            }

            // 不是不支持参数的错误，正常返回
            let usage = extract_usage_from_json_bytes(&peek_bytes);
            if upstream_status.is_success() && usage.total_tokens > 0 {
                pool.record_usage(&meta.account_id, usage.total_tokens)
                    .await;
            }

            let log_model = if request_model == "unknown" {
                extract_model_from_json_bytes(&peek_bytes).unwrap_or(request_model)
            } else {
                request_model
            };
            let error_message = if upstream_status.is_success() {
                None
            } else {
                extract_error_message(&peek_bytes)
            };
            let log = build_request_log(
                &meta,
                log_model,
                if upstream_status.is_success() {
                    "success"
                } else {
                    "error"
                },
                usage,
                error_message.clone(),
                gateway_profile.as_ref(),
            );
            record_log(logger, storage, log).await;
            if let Some(archive_store) = archive_storage.as_ref() {
                archive_capture.append_response_chunk(peek_bytes.clone());
                let _ = archive_capture.finish_response(
                    if upstream_status.is_success() {
                        "success"
                    } else {
                        "error"
                    },
                    error_message.clone(),
                    archive_usage_from(usage),
                    archive_store.as_ref(),
                );
            }

            let response = build_buffered_response(upstream_status, &upstream_headers, peek_bytes)
                .map_err(|e| warp::reject::custom(CodexRejection::InternalError(e.to_string())))?;
            return Ok(Box::new(response) as Box<dyn Reply>);
        }

        // 流式响应或非 responses 格式
        if is_event_stream(&upstream_headers) {
            if stream_forced {
                let response = destream_responses_sse(
                    upstream_status,
                    &upstream_headers,
                    upstream_response,
                    pool,
                    logger,
                    storage,
                    archive_storage.clone(),
                    meta,
                    request_model,
                    archive_capture,
                    gateway_profile.clone(),
                )
                .await
                .map_err(|e| warp::reject::custom(CodexRejection::InternalError(e)))?;
                return Ok(Box::new(response) as Box<dyn Reply>);
            }

            let response = build_streaming_response_with_metrics(
                upstream_status,
                &upstream_headers,
                upstream_response,
                pool,
                logger,
                storage,
                archive_storage.clone(),
                meta,
                request_model,
                archive_capture,
                gateway_profile.clone(),
            )
            .map_err(|e| warp::reject::custom(CodexRejection::InternalError(e.to_string())))?;
            return Ok(Box::new(response) as Box<dyn Reply>);
        }

        let upstream_bytes = match upstream_response.bytes().await {
            Ok(bytes) => bytes,
            Err(err) => {
                pool.record_failure(&meta.account_id, None).await;
                let err_text = format!("Failed to read upstream response body: {}", err);
                if let Some(archive_store) = archive_storage.as_ref() {
                    let _ = archive_capture.finish_response(
                        "error",
                        Some(err_text.clone()),
                        ArchiveUsageStats::default(),
                        archive_store.as_ref(),
                    );
                }
                add_failed_log(
                    logger,
                    storage,
                    &request_model,
                    &request_format,
                    err_text.clone(),
                    gateway_profile.as_ref(),
                )
                .await;
                return Err(warp::reject::custom(CodexRejection::ExecutionError(
                    err_text,
                )));
            }
        };

        let usage = extract_usage_from_json_bytes(&upstream_bytes);
        if upstream_status.is_success() && usage.total_tokens > 0 {
            pool.record_usage(&meta.account_id, usage.total_tokens)
                .await;
        }

        let log_model = if request_model == "unknown" {
            extract_model_from_json_bytes(&upstream_bytes).unwrap_or(request_model)
        } else {
            request_model
        };
        let error_message = if upstream_status.is_success() {
            None
        } else {
            extract_error_message(&upstream_bytes)
        };
        let log = build_request_log(
            &meta,
            log_model,
            if upstream_status.is_success() {
                "success"
            } else {
                "error"
            },
            usage,
            error_message.clone(),
            gateway_profile.as_ref(),
        );
        record_log(logger, storage, log).await;
        if let Some(archive_store) = archive_storage.as_ref() {
            archive_capture.append_response_chunk(upstream_bytes.clone());
            let _ = archive_capture.finish_response(
                if upstream_status.is_success() {
                    "success"
                } else {
                    "error"
                },
                error_message.clone(),
                archive_usage_from(usage),
                archive_store.as_ref(),
            );
        }

        let response = build_buffered_response(upstream_status, &upstream_headers, upstream_bytes)
            .map_err(|e| warp::reject::custom(CodexRejection::InternalError(e.to_string())))?;
        return Ok(Box::new(response) as Box<dyn Reply>);
    }
}

/// 收集上游 SSE 流，提取 response.completed 事件中的完整响应对象，
/// 以普通 JSON 返回给不需要流式的客户端。
async fn destream_responses_sse(
    status: StatusCode,
    headers: &HeaderMap,
    response: reqwest::Response,
    pool: Arc<CodexPool>,
    logger: Arc<RwLock<RequestLogger>>,
    storage: Option<Arc<CodexLogStorage>>,
    archive_storage: Option<Arc<CodexArchiveStorage>>,
    meta: ForwardMeta,
    request_model: String,
    mut archive_capture: ArchiveTurnCapture,
    gateway_profile: Option<GatewayAccessProfile>,
) -> Result<Response<Body>, String> {
    let mut stream = response.bytes_stream();
    let mut extractor = SseMetricsExtractor::default();
    let mut all_data = String::new();
    archive_capture.set_response_headers(headers);

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                extractor.ingest_chunk(&bytes);
                all_data.push_str(&String::from_utf8_lossy(&bytes));
                archive_capture.append_response_chunk(bytes.clone());
            }
            Err(err) => {
                let err_text = format!("Failed to read upstream stream: {}", err);
                pool.record_failure(&meta.account_id, None).await;
                if let Some(archive_store) = archive_storage.as_ref() {
                    let _ = archive_capture
                        .finish_with_stream_error(err_text.clone(), archive_store.as_ref());
                }
                return Err(err_text);
            }
        }
    }
    extractor.finish();

    // 记录 usage
    let usage = extractor.usage;
    if status.is_success() && usage.total_tokens > 0 {
        pool.record_usage(&meta.account_id, usage.total_tokens)
            .await;
    }

    let log_model = if request_model == "unknown" {
        extractor.model.clone().unwrap_or(request_model.clone())
    } else {
        request_model.clone()
    };
    let error_message = if status.is_success() {
        None
    } else {
        extractor.error_message.clone()
    };
    let log = build_request_log(
        &meta,
        log_model,
        if status.is_success() {
            "success"
        } else {
            "error"
        },
        usage,
        error_message.clone(),
        gateway_profile.as_ref(),
    );
    record_log(logger, storage, log).await;
    if let Some(archive_store) = archive_storage.as_ref() {
        let _ = archive_capture.finish_response(
            if status.is_success() {
                "success"
            } else {
                "error"
            },
            error_message.clone(),
            archive_usage_from(usage),
            archive_store.as_ref(),
        );
    }

    // 从 SSE 事件中提取 response.completed 的 response 对象
    let response_json = extract_completed_response(&all_data)
        .unwrap_or_else(|| json!({"error": "Failed to extract response from SSE stream"}));

    let body_bytes = serde_json::to_vec(&response_json)
        .map_err(|e| format!("Failed to serialize response: {}", e))?;

    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body_bytes))
        .map_err(|e| format!("Failed to build destreamed response: {}", e))
}

/// 从 SSE 文本中提取 response.completed 事件的 response 对象
fn extract_completed_response(sse_text: &str) -> Option<Value> {
    // 按双换行分割事件块
    for block in sse_text.split("\n\n") {
        let mut event_type = None;
        let mut data_lines = Vec::new();

        for line in block.lines() {
            let line = line.trim_start();
            if let Some(rest) = line.strip_prefix("event:") {
                event_type = Some(rest.trim());
            } else if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim_start());
            }
        }

        // 查找 response.completed 事件
        if event_type == Some("response.completed") || event_type == Some("response.done") {
            if data_lines.is_empty() {
                continue;
            }
            let data = data_lines.join("\n");
            if let Ok(value) = serde_json::from_str::<Value>(&data) {
                // response.completed 事件的 data 中有 response 字段
                if let Some(resp) = value.get("response").cloned() {
                    return Some(resp);
                }
                // 如果没有 response 字段，整个 data 可能就是响应
                return Some(value);
            }
        }
    }

    // 也尝试 \r\n\r\n 分割
    for block in sse_text.split("\r\n\r\n") {
        let mut event_type = None;
        let mut data_lines = Vec::new();

        for line in block.lines() {
            let line = line.trim_start();
            if let Some(rest) = line.strip_prefix("event:") {
                event_type = Some(rest.trim());
            } else if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim_start());
            }
        }

        if event_type == Some("response.completed") || event_type == Some("response.done") {
            if data_lines.is_empty() {
                continue;
            }
            let data = data_lines.join("\n");
            if let Ok(value) = serde_json::from_str::<Value>(&data) {
                if let Some(resp) = value.get("response").cloned() {
                    return Some(resp);
                }
                return Some(value);
            }
        }
    }

    None
}

fn build_streaming_response_with_metrics(
    status: StatusCode,
    headers: &HeaderMap,
    response: reqwest::Response,
    pool: Arc<CodexPool>,
    logger: Arc<RwLock<RequestLogger>>,
    storage: Option<Arc<CodexLogStorage>>,
    archive_storage: Option<Arc<CodexArchiveStorage>>,
    meta: ForwardMeta,
    request_model: String,
    mut archive_capture: ArchiveTurnCapture,
    gateway_profile: Option<GatewayAccessProfile>,
) -> Result<Response<Body>, String> {
    let mut builder = Response::builder().status(status);
    for (name, value) in headers.iter() {
        if should_strip_response_header(name.as_str()) {
            continue;
        }
        builder = builder.header(name, value);
    }

    let mut upstream_stream = response.bytes_stream();
    let (mut tx, rx) = futures::channel::mpsc::channel::<Result<Bytes, std::io::Error>>(16);
    archive_capture.set_response_headers(headers);

    tokio::spawn(async move {
        let mut extractor = SseMetricsExtractor::default();
        let mut stream_interrupted = false;
        let mut stream_error: Option<String> = None;

        while let Some(chunk) = upstream_stream.next().await {
            match chunk {
                Ok(bytes) => {
                    extractor.ingest_chunk(&bytes);
                    archive_capture.append_response_chunk(bytes.clone());
                    if tx.send(Ok(bytes)).await.is_err() {
                        stream_interrupted = true;
                        stream_error =
                            Some("Client disconnected before stream completion".to_string());
                        break;
                    }
                }
                Err(err) => {
                    let err_text = format!("Failed to read upstream stream chunk: {}", err);
                    extractor.error_message.get_or_insert(err_text.clone());
                    stream_interrupted = true;
                    stream_error = Some(err_text.clone());
                    let _ = tx
                        .send(Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            err_text,
                        )))
                        .await;
                    break;
                }
            }
        }

        extractor.finish();
        let usage = extractor.usage;
        if status.is_success() && usage.total_tokens > 0 {
            pool.record_usage(&meta.account_id, usage.total_tokens)
                .await;
        }

        let log_model = if request_model == "unknown" {
            extractor.model.unwrap_or(request_model)
        } else {
            request_model
        };
        let error_message = if status.is_success() {
            None
        } else {
            extractor.error_message
        };
        if let Some(archive_store) = archive_storage.as_ref() {
            let _ = if stream_interrupted {
                archive_capture.finish_with_stream_error(
                    stream_error
                        .clone()
                        .unwrap_or_else(|| "Stream interrupted".to_string()),
                    archive_store.as_ref(),
                )
            } else {
                archive_capture.finish_response(
                    if status.is_success() {
                        "success"
                    } else {
                        "error"
                    },
                    error_message.clone(),
                    archive_usage_from(usage),
                    archive_store.as_ref(),
                )
            };
        }
        let log = build_request_log(
            &meta,
            log_model,
            if status.is_success() {
                "success"
            } else {
                "error"
            },
            usage,
            error_message,
            gateway_profile.as_ref(),
        );
        record_log(logger, storage, log).await;
    });

    builder
        .body(Body::wrap_stream(rx))
        .map_err(|e| format!("Failed to build streaming response: {}", e))
}

#[derive(Default)]
struct SseMetricsExtractor {
    pending: String,
    usage: UsageStats,
    model: Option<String>,
    error_message: Option<String>,
}

impl SseMetricsExtractor {
    fn ingest_chunk(&mut self, chunk: &Bytes) {
        let text = String::from_utf8_lossy(chunk);
        self.pending.push_str(&text);

        while let Some(idx) = self.pending.find("\n\n") {
            let event = self.pending[..idx].to_string();
            self.pending.drain(..idx + 2);
            self.parse_event_block(&event);
        }

        while let Some(idx) = self.pending.find("\r\n\r\n") {
            let event = self.pending[..idx].to_string();
            self.pending.drain(..idx + 4);
            self.parse_event_block(&event);
        }
    }

    fn finish(&mut self) {
        if !self.pending.trim().is_empty() {
            let event = std::mem::take(&mut self.pending);
            self.parse_event_block(&event);
        }
    }

    fn parse_event_block(&mut self, block: &str) {
        let mut data_lines = Vec::new();

        for line in block.lines() {
            let line = line.trim_start();
            if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim_start());
            }
        }

        if data_lines.is_empty() {
            return;
        }

        let data = data_lines.join("\n");
        if data.trim() == "[DONE]" {
            return;
        }

        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            return;
        };

        self.extract_fields(&value);
    }

    fn extract_fields(&mut self, value: &Value) {
        // 提取 model
        if self.model.is_none() {
            self.model = value
                .pointer("/response/model")
                .or_else(|| value.get("model"))
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
        }

        // 提取 usage - ChatGPT Codex 流式响应中 usage 在 response.completed 事件的 response.usage 下
        if let Some(usage) = value
            .pointer("/response/usage")
            .or_else(|| value.get("usage"))
        {
            let input_tokens = to_i64(
                usage
                    .get("input_tokens")
                    .or_else(|| usage.get("prompt_tokens")),
            );
            let output_tokens = to_i64(
                usage
                    .get("output_tokens")
                    .or_else(|| usage.get("completion_tokens")),
            );
            let total_tokens = {
                let explicit_total = to_i64(usage.get("total_tokens"));
                if explicit_total > 0 {
                    explicit_total
                } else {
                    input_tokens + output_tokens
                }
            };

            self.usage = UsageStats {
                input_tokens,
                output_tokens,
                total_tokens,
            };
        }

        // 提取错误信息
        if self.error_message.is_none() {
            self.error_message = value
                .pointer("/response/error/message")
                .or_else(|| value.pointer("/error/message"))
                .or_else(|| value.get("message"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string());
        }
    }
}

fn build_buffered_response(
    status: StatusCode,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response<Body>, String> {
    let mut builder = Response::builder().status(status);
    for (name, value) in headers.iter() {
        if should_strip_response_header(name.as_str()) {
            continue;
        }
        builder = builder.header(name, value);
    }

    builder
        .body(Body::from(body))
        .map_err(|e| format!("Failed to build response: {}", e))
}

fn should_strip_response_header(header_name: &str) -> bool {
    matches!(
        header_name.to_ascii_lowercase().as_str(),
        "content-length"
            | "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn is_event_stream(headers: &HeaderMap) -> bool {
    // ChatGPT Codex API 默认返回 SSE 格式，即使没有设置 content-type
    headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase().contains("text/event-stream"))
        .unwrap_or(true) // 当 content-type 缺失时，默认假设为 SSE
}

fn is_supported_proxy_path(path: &str) -> bool {
    path == "/v1"
        || path.starts_with("/v1/")
        || path == "/backend-api/codex"
        || path.starts_with("/backend-api/codex/")
}

fn infer_request_format(path: &str) -> &'static str {
    if path.ends_with("/chat/completions") {
        "openai-chat"
    } else if path.ends_with("/messages") {
        "claude"
    } else if path.contains("/responses") {
        "openai-responses"
    } else {
        "passthrough"
    }
}

/// 规范化 Responses API 请求体的结果
struct NormalizeResult {
    /// 规范化后的 body（None 表示无需修改）
    body: Option<Bytes>,
    /// 是否强制将 stream 从 false/缺失 改为 true
    stream_forced: bool,
}

/// 规范化 Responses API 请求体，使其兼容 ChatGPT Codex 后端：
/// - 将字符串 `input` 转换为消息对象数组
/// - 补充缺失的 `instructions` 字段
/// - 强制 `stream: true`
fn normalize_responses_body(body: &Bytes) -> NormalizeResult {
    let Some(mut root) = serde_json::from_slice::<Value>(body).ok() else {
        return NormalizeResult {
            body: None,
            stream_forced: false,
        };
    };
    let Some(obj) = root.as_object_mut() else {
        return NormalizeResult {
            body: None,
            stream_forced: false,
        };
    };
    let mut modified = false;
    let mut stream_forced = false;

    // input 字符串 → 数组
    if let Some(input) = obj.get("input") {
        if let Some(text) = input.as_str() {
            let text = text.to_string();
            obj.insert(
                "input".to_string(),
                json!([{
                    "role": "user",
                    "content": [{"type": "input_text", "text": text}]
                }]),
            );
            modified = true;
        }
    }

    // 补充缺失的 instructions
    if !obj.contains_key("instructions") {
        obj.insert(
            "instructions".to_string(),
            json!("You are a helpful assistant."),
        );
        modified = true;
    }

    // ChatGPT Codex 后端要求 stream 必须为 true
    match obj.get("stream") {
        Some(v) if v.as_bool() == Some(true) => {}
        _ => {
            obj.insert("stream".to_string(), json!(true));
            stream_forced = true;
            modified = true;
        }
    }

    let new_body = if modified {
        serde_json::to_vec(&root).ok().map(Bytes::from)
    } else {
        None
    };

    NormalizeResult {
        body: new_body,
        stream_forced,
    }
}

/// 从上游错误响应中提取 "Unsupported parameter" 的参数名
fn extract_unsupported_param(body: &Bytes) -> Option<String> {
    let text = String::from_utf8_lossy(body);
    // 在整个响应文本中搜索 "Unsupported parameter:" 模式
    // 上游错误格式可能是 JSON 也可能不是，直接做文本匹配更稳健
    let prefix = "Unsupported parameter:";
    let idx = text.find(prefix)?;
    let rest = &text[idx + prefix.len()..];
    // 取冒号后面的参数名，去掉引号和空格
    let param: String = rest
        .trim()
        .trim_start_matches(|c: char| c == '\'' || c == '"' || c == '`')
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if param.is_empty() { None } else { Some(param) }
}

/// 从 JSON body 中移除指定的 key
fn remove_json_key(body: &Bytes, key: &str) -> Bytes {
    if let Ok(mut root) = serde_json::from_slice::<Value>(body) {
        if let Some(obj) = root.as_object_mut() {
            obj.remove(key);
            if let Ok(new_body) = serde_json::to_vec(&root) {
                return Bytes::from(new_body);
            }
        }
    }
    body.clone()
}

fn extract_model_from_json_bytes(body: &Bytes) -> Option<String> {
    serde_json::from_slice::<Value>(body).ok().and_then(|v| {
        v.get("model")
            .and_then(|m| m.as_str())
            .map(|m| m.to_string())
    })
}

fn extract_usage_from_json_bytes(body: &Bytes) -> UsageStats {
    let Ok(root) = serde_json::from_slice::<Value>(body) else {
        return UsageStats::default();
    };

    let Some(usage) = root
        .get("usage")
        .or_else(|| root.get("response").and_then(|v| v.get("usage")))
    else {
        return UsageStats::default();
    };

    let input_tokens = to_i64(
        usage
            .get("input_tokens")
            .or_else(|| usage.get("prompt_tokens")),
    );
    let output_tokens = to_i64(
        usage
            .get("output_tokens")
            .or_else(|| usage.get("completion_tokens")),
    );
    let total_tokens = {
        let explicit_total = to_i64(usage.get("total_tokens"));
        if explicit_total > 0 {
            explicit_total
        } else {
            input_tokens + output_tokens
        }
    };

    UsageStats {
        input_tokens,
        output_tokens,
        total_tokens,
    }
}

fn extract_error_message(body: &Bytes) -> Option<String> {
    if let Ok(root) = serde_json::from_slice::<Value>(body) {
        if let Some(msg) = root
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            return Some(msg.to_string());
        }

        if let Some(msg) = root
            .get("message")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            return Some(msg.to_string());
        }

        if let Some(msg) = root
            .get("detail")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            return Some(msg.to_string());
        }
    }

    let text = String::from_utf8_lossy(body).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text.chars().take(300).collect())
    }
}

fn to_i64(value: Option<&Value>) -> i64 {
    value
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_u64().and_then(|n| i64::try_from(n).ok()))
        })
        .unwrap_or(0)
}

#[derive(Default)]
struct GatewayProfileLogMetadata {
    gateway_profile_id: Option<String>,
    gateway_profile_name: Option<String>,
    member_code: Option<String>,
    role_title: Option<String>,
    display_label: Option<String>,
    api_key_suffix: Option<String>,
    color: Option<String>,
}

fn trimmed_profile_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn build_gateway_profile_display_label(
    name: Option<&str>,
    member_code: Option<&str>,
    role_title: Option<&str>,
) -> Option<String> {
    let mut parts = Vec::new();

    if let Some(name) = trimmed_profile_value(name) {
        parts.push(name);
    }
    if let Some(member_code) = trimmed_profile_value(member_code) {
        parts.push(member_code);
    }
    if let Some(role_title) = trimmed_profile_value(role_title) {
        parts.push(role_title);
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

fn extract_gateway_api_key_suffix(api_key: &str) -> Option<String> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return None;
    }

    trimmed
        .rsplit('-')
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn build_gateway_profile_log_metadata(
    gateway_profile: Option<&GatewayAccessProfile>,
) -> GatewayProfileLogMetadata {
    let Some(profile) = gateway_profile else {
        return GatewayProfileLogMetadata::default();
    };

    let gateway_profile_name = trimmed_profile_value(Some(profile.name.as_str()));
    let member_code = trimmed_profile_value(profile.member_code.as_deref());
    let role_title = trimmed_profile_value(profile.role_title.as_deref());

    GatewayProfileLogMetadata {
        gateway_profile_id: trimmed_profile_value(Some(profile.id.as_str())),
        gateway_profile_name: gateway_profile_name.clone(),
        member_code: member_code.clone(),
        role_title: role_title.clone(),
        display_label: build_gateway_profile_display_label(
            gateway_profile_name.as_deref(),
            member_code.as_deref(),
            role_title.as_deref(),
        ),
        api_key_suffix: extract_gateway_api_key_suffix(&profile.api_key),
        color: trimmed_profile_value(profile.color.as_deref()),
    }
}

fn build_archive_turn_capture(
    path: &str,
    method: &Method,
    headers: &HeaderMap,
    body: &Bytes,
    request_format: &str,
    request_model: &str,
    gateway_profile: Option<&GatewayAccessProfile>,
) -> ArchiveTurnCapture {
    let metadata = build_gateway_profile_log_metadata(gateway_profile);
    let mut markers = extract_turn_metadata(headers);
    markers.prompt_cache_key = extract_prompt_cache_key(body);
    markers.explicit_session_id = extract_explicit_session_id(headers, body);

    let gateway_profile_id = metadata
        .gateway_profile_id
        .clone()
        .unwrap_or_else(|| "legacy".to_string());
    let session = derive_archive_session_identity_from_markers(
        &gateway_profile_id,
        markers.explicit_session_id.as_deref(),
        markers.prompt_cache_key.as_deref(),
        markers.turn_id.as_deref(),
    );

    ArchiveTurnCapture::new(ArchiveTurnContext {
        gateway_profile_id,
        gateway_profile_name: metadata
            .gateway_profile_name
            .clone()
            .or_else(|| Some("Legacy".to_string())),
        member_code: metadata.member_code.clone(),
        display_label: metadata.display_label.clone(),
        prompt_cache_key: markers.prompt_cache_key.clone(),
        explicit_session_id: markers.explicit_session_id.clone(),
        markers,
        session,
        request_path: path.to_string(),
        request_method: method.as_str().to_string(),
        model: request_model.to_string(),
        format: request_format.to_string(),
        request_headers: headers.clone(),
        request_body: body.clone(),
        request_started_at: chrono::Utc::now().timestamp(),
    })
}

fn archive_usage_from(usage: UsageStats) -> ArchiveUsageStats {
    ArchiveUsageStats {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
    }
}

fn build_request_log(
    meta: &ForwardMeta,
    model: String,
    status: &str,
    usage: UsageStats,
    error_message: Option<String>,
    gateway_profile: Option<&GatewayAccessProfile>,
) -> RequestLog {
    let metadata = build_gateway_profile_log_metadata(gateway_profile);

    RequestLog {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp(),
        account_id: meta.account_id.clone(),
        account_email: meta.account_email.clone(),
        model,
        format: meta.format.clone(),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        status: status.to_string(),
        error_message,
        request_duration_ms: Some(meta.started_at.elapsed().as_millis() as i64),
        gateway_profile_id: metadata.gateway_profile_id,
        gateway_profile_name: metadata.gateway_profile_name,
        member_code: metadata.member_code,
        role_title: metadata.role_title,
        display_label: metadata.display_label,
        api_key_suffix: metadata.api_key_suffix,
        color: metadata.color,
    }
}

async fn record_log(
    logger: Arc<RwLock<RequestLogger>>,
    storage: Option<Arc<CodexLogStorage>>,
    log: RequestLog,
) {
    let mut guard = logger.write().await;
    guard.add_log(log.clone());

    // 同时写入 SQLite 存储
    if let Some(s) = storage {
        s.add_log(log).await;
    }
}

/// 更新账户的 forbidden 状态到数据库
async fn mark_account_forbidden(state: &Arc<AppState>, account_id: &str) {
    let storage = {
        let guard = state.openai_storage_manager.lock().unwrap();
        guard.clone()
    };
    let Some(storage) = storage else {
        return;
    };

    // 获取账户并更新 quota.is_forbidden
    if let Ok(Some(mut account)) = storage.get_account(account_id).await {
        if let Some(ref mut quota) = account.quota {
            quota.is_forbidden = true;
        } else {
            let mut quota = crate::platforms::openai::models::QuotaData::new();
            quota.is_forbidden = true;
            account.quota = Some(quota);
        }
        let _ = storage.update_account(&account).await;
    }
}

async fn add_failed_log(
    logger: Arc<RwLock<RequestLogger>>,
    storage: Option<Arc<CodexLogStorage>>,
    model: &str,
    format: &str,
    error: String,
    gateway_profile: Option<&GatewayAccessProfile>,
) {
    let metadata = build_gateway_profile_log_metadata(gateway_profile);
    let log = RequestLog {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp(),
        account_id: String::new(),
        account_email: String::new(),
        model: model.to_string(),
        format: format.to_string(),
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        status: "error".to_string(),
        error_message: Some(error),
        request_duration_ms: None,
        gateway_profile_id: metadata.gateway_profile_id,
        gateway_profile_name: metadata.gateway_profile_name,
        member_code: metadata.member_code,
        role_title: metadata.role_title,
        display_label: metadata.display_label,
        api_key_suffix: metadata.api_key_suffix,
        color: metadata.color,
    };
    let mut guard = logger.write().await;
    guard.add_log(log.clone());

    // 同时写入 SQLite 存储
    if let Some(s) = storage {
        s.add_log(log).await;
    }
}

fn ensure_codex_enabled(state: &Arc<AppState>) -> Result<(), Rejection> {
    if state.codex_server.lock().unwrap().is_some() {
        return Ok(());
    }

    Err(warp::reject::custom(CodexRejection::ServiceUnavailable(
        "Codex service is disabled".to_string(),
    )))
}

fn get_runtime_or_reject(
    state: &Arc<AppState>,
) -> Result<
    (
        Arc<CodexPool>,
        Arc<CodexExecutor>,
        Arc<RwLock<RequestLogger>>,
        Option<Arc<CodexLogStorage>>,
        Option<Arc<CodexArchiveStorage>>,
    ),
    Rejection,
> {
    let pool = state.codex_pool.lock().unwrap().clone().ok_or_else(|| {
        warp::reject::custom(CodexRejection::ServiceUnavailable(
            "Codex pool is not initialized".to_string(),
        ))
    })?;

    let executor = state
        .codex_executor
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| {
            warp::reject::custom(CodexRejection::ServiceUnavailable(
                "Codex executor is not initialized".to_string(),
            ))
        })?;

    let logger = state.codex_logger.lock().unwrap().clone().ok_or_else(|| {
        warp::reject::custom(CodexRejection::ServiceUnavailable(
            "Codex logger is not initialized".to_string(),
        ))
    })?;

    let storage = state.codex_log_storage.lock().unwrap().clone();
    let archive_storage = state.codex_archive_storage.lock().unwrap().clone();

    Ok((pool, executor, logger, storage, archive_storage))
}

fn validate_api_key(state: &Arc<AppState>, headers: &HeaderMap) -> Result<(), Rejection> {
    let configured_key = state
        .codex_server_config
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|cfg| cfg.api_key.as_ref())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    // API Key 必须设置，未设置则拒绝请求
    let Some(expected) = configured_key else {
        return Err(warp::reject::custom(CodexRejection::ExecutionError(
            "Unauthorized: API key not configured".to_string(),
        )));
    };

    let mut candidates: Vec<&str> = Vec::new();

    if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if let Some(token) = extract_bearer_token(auth) {
            candidates.push(token);
        }
    }

    if let Some(key) = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        candidates.push(key);
    }

    if candidates.into_iter().any(|provided| provided == expected) {
        return Ok(());
    }

    Err(warp::reject::custom(CodexRejection::ExecutionError(
        "Unauthorized: invalid API key".to_string(),
    )))
}

fn extract_bearer_token(header: &str) -> Option<&str> {
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

fn get_models() -> serde_json::Value {
    json!({
        "object": "list",
        "data": [
            {
                "id": "gpt-5",
                "object": "model",
                "created": 1728000000,
                "owned_by": "openai"
            },
            {
                "id": "gpt-5-codex",
                "object": "model",
                "created": 1728000000,
                "owned_by": "openai"
            },
            {
                "id": "gpt-4.1",
                "object": "model",
                "created": 1728000000,
                "owned_by": "openai"
            },
            {
                "id": "gpt-4o",
                "object": "model",
                "created": 1728000000,
                "owned_by": "openai"
            }
        ]
    })
}

// ==================== 错误类型 ====================

#[derive(Debug)]
pub enum CodexRejection {
    NoAvailableAccount,
    InvalidRequest(String),
    TranslationError(String),
    ExecutionError(String),
    ServiceUnavailable(String),
    InternalError(String),
}

impl warp::reject::Reject for CodexRejection {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gateway_access::{GatewayAccessProfile, GatewayTarget};
    use crate::platforms::openai::codex::archive::ArchiveSessionConfidence;
    use crate::platforms::openai::codex::archive_storage::CodexArchiveStorage;
    use bytes::Bytes;
    use tempfile::tempdir;
    use warp::http::HeaderValue;

    #[test]
    fn build_request_log_captures_member_identity_fields() {
        let meta = ForwardMeta {
            account_id: "account-1".to_string(),
            account_email: "user@example.com".to_string(),
            format: "openai-responses".to_string(),
            model: "gpt-5".to_string(),
            started_at: std::time::Instant::now(),
        };
        let profile = GatewayAccessProfile {
            id: "codex-jdd".to_string(),
            name: "姜大大".to_string(),
            target: GatewayTarget::Codex,
            api_key: "sk-team-jdd-a4f29c7e".to_string(),
            enabled: true,
            member_code: Some("jdd".to_string()),
            role_title: Some("产品与方法论".to_string()),
            persona_summary: Some("高频输出，偏产品与方法论视角".to_string()),
            color: Some("#4c6ef5".to_string()),
            notes: Some("核心体验 owner".to_string()),
        };

        let log = build_request_log(
            &meta,
            "gpt-5".to_string(),
            "success",
            UsageStats {
                input_tokens: 10,
                output_tokens: 20,
                total_tokens: 30,
            },
            None,
            Some(&profile),
        );
        let row = serde_json::to_value(&log).unwrap();

        assert_eq!(row["member_code"], "jdd");
        assert_eq!(row["role_title"], "产品与方法论");
        assert_eq!(row["display_label"], "姜大大 · jdd · 产品与方法论");
        assert_eq!(row["api_key_suffix"], "a4f29c7e");
        assert_eq!(row["color"], "#4c6ef5");
    }

    #[test]
    fn build_archive_turn_capture_prefers_explicit_project_marker() {
        let profile = GatewayAccessProfile {
            id: "codex-jdd".to_string(),
            name: "姜大大".to_string(),
            target: GatewayTarget::Codex,
            api_key: "sk-team-jdd-a4f29c7e".to_string(),
            enabled: true,
            member_code: Some("jdd".to_string()),
            role_title: Some("产品与方法论".to_string()),
            persona_summary: Some("高频输出，偏产品与方法论视角".to_string()),
            color: Some("#4c6ef5".to_string()),
            notes: Some("核心体验 owner".to_string()),
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Codex-Turn-Metadata",
            HeaderValue::from_static(r#"{"turn_id":"turn-1","sandbox":"seatbelt"}"#),
        );
        let body = Bytes::from_static(
            br#"{
                "model":"gpt-5.4",
                "prompt_cache_key":"task9-archive-20260315-0955",
                "metadata":{"project_id":"repo:atm/archive"},
                "input":"hello"
            }"#,
        );
        let temp_dir = tempdir().unwrap();
        let storage = CodexArchiveStorage::new(temp_dir.path().to_path_buf()).unwrap();

        let capture = build_archive_turn_capture(
            "/v1/responses",
            &Method::POST,
            &headers,
            &body,
            "openai-responses",
            "gpt-5.4",
            Some(&profile),
        );
        capture
            .finish_completed("success", ArchiveUsageStats::default(), &storage)
            .unwrap();

        let turn = storage.get_turn("turn-1").unwrap().unwrap();

        assert_eq!(
            turn.archive_session_id,
            "codex-jdd:explicit:repo:atm/archive"
        );
        assert_eq!(
            turn.explicit_session_id.as_deref(),
            Some("repo:atm/archive")
        );
        assert_eq!(
            turn.prompt_cache_key.as_deref(),
            Some("task9-archive-20260315-0955")
        );
        assert_eq!(turn.confidence, ArchiveSessionConfidence::High);
    }
}
