use serde::Serialize;
use serde_json::json;
use std::time::SystemTime;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use super::models::{
    AccountStatus, AugmentTokenResponse, BatchCreditConsumptionResponse, PaymentMethodLinkResult,
    SessionRefreshResult, TokenFromSessionResponse, TokenInfo, TokenStatusResult,
};
use super::modules::{account, oauth};
use crate::data::storage::augment::traits::TokenStorage;
use crate::{AppSessionCache, AppState};

const DEFAULT_SHARED_API_SERVER_PORT: u16 = 8766;
const AUGMENT_GATEWAY_PROFILE_ID: &str = "augment-default";
const AUGMENT_GATEWAY_PROFILE_NAME: &str = "Augment Default";
const AUGMENT_AUTH_POOL_ENV_KEY: &str = "OPENAI_API_KEY_POOL_AUGMENT_1";
const AUGMENT_PROVIDER_ID: &str = "atm-augment";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AugmentProxyStatus {
    pub api_server_running: bool,
    pub api_server_address: Option<String>,
    pub proxy_base_url: String,
    pub sidecar_configured: bool,
    pub sidecar_running: bool,
    pub sidecar_healthy: bool,
    pub total_accounts: usize,
    pub available_accounts: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AugmentGatewayAccessConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub env_key: String,
    pub curl_example: String,
    pub auth_pool_snippet: String,
    pub config_pool_snippet: String,
}

fn api_server_address_for_port(port: u16) -> String {
    format!("http://127.0.0.1:{}", port)
}

fn gateway_base_url_for_port(port: u16) -> String {
    format!("{}/v1", api_server_address_for_port(port))
}

fn current_api_server_port(state: &AppState) -> Option<u16> {
    let guard = state.api_server.lock().unwrap();
    guard.as_ref().map(|server| server.get_port())
}

fn normalize_api_key(api_key: Option<String>) -> Option<String> {
    api_key.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn generate_gateway_api_key() -> String {
    format!(
        "sk-{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn build_auth_pool_snippet(api_key: &str) -> String {
    serde_json::to_string_pretty(&json!({
        AUGMENT_AUTH_POOL_ENV_KEY: api_key
    }))
    .expect("auth-pool snippet should serialize")
}

fn build_config_pool_snippet(base_url: &str) -> String {
    format!(
        r#"[model_providers.{provider}]
base_url = "{base_url}"
env_key = "{env_key}"

[[model_providers.{provider}.account_pool]]
base_url = "{base_url}"
env_key = "{env_key}"
"#,
        provider = AUGMENT_PROVIDER_ID,
        base_url = base_url,
        env_key = AUGMENT_AUTH_POOL_ENV_KEY,
    )
}

fn build_curl_example(base_url: &str, api_key: &str) -> String {
    let chat_url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    format!(
        "curl {chat_url} \\\n  -H \"Authorization: Bearer {api_key}\" \\\n  -H \"Content-Type: application/json\" \\\n  -d '{{\n    \"model\": \"gemini-3.1-pro\",\n    \"messages\": [{{\"role\": \"user\", \"content\": \"Hello\"}}]\n  }}'",
        chat_url = chat_url,
        api_key = api_key,
    )
}

fn build_augment_gateway_access_config(
    base_url: String,
    api_key: String,
) -> AugmentGatewayAccessConfig {
    AugmentGatewayAccessConfig {
        base_url: base_url.clone(),
        api_key: Some(api_key.clone()),
        env_key: AUGMENT_AUTH_POOL_ENV_KEY.to_string(),
        curl_example: build_curl_example(&base_url, &api_key),
        auth_pool_snippet: build_auth_pool_snippet(&api_key),
        config_pool_snippet: build_config_pool_snippet(&base_url),
    }
}

fn ensure_augment_gateway_profile(
    app: &tauri::AppHandle,
    state: &AppState,
    api_key: Option<String>,
) -> Result<String, String> {
    let gateway_api_key = normalize_api_key(api_key).unwrap_or_else(generate_gateway_api_key);
    let mut profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(app, state)?;
    profiles
        .profiles
        .retain(|profile| profile.target != crate::core::gateway_access::GatewayTarget::Augment);
    profiles
        .profiles
        .push(crate::core::gateway_access::GatewayAccessProfile {
            id: AUGMENT_GATEWAY_PROFILE_ID.to_string(),
            name: AUGMENT_GATEWAY_PROFILE_NAME.to_string(),
            target: crate::core::gateway_access::GatewayTarget::Augment,
            api_key: gateway_api_key.clone(),
            enabled: true,
        });
    crate::core::gateway_access::set_gateway_access_profiles(app, state, profiles)?;
    Ok(gateway_api_key)
}

fn summarize_augment_proxy_status(
    api_server_port: Option<u16>,
    sidecar_configured: bool,
    sidecar_running: bool,
    sidecar_healthy: bool,
    tokens: &[crate::storage::TokenData],
) -> AugmentProxyStatus {
    let api_server_address = api_server_port.map(api_server_address_for_port);
    let gateway_base_url =
        gateway_base_url_for_port(api_server_port.unwrap_or(DEFAULT_SHARED_API_SERVER_PORT));
    let available_accounts = tokens
        .iter()
        .filter(|token| super::proxy_server::is_token_usable(token))
        .count();

    AugmentProxyStatus {
        api_server_running: api_server_port.is_some(),
        api_server_address,
        proxy_base_url: gateway_base_url,
        sidecar_configured,
        sidecar_running,
        sidecar_healthy,
        total_accounts: tokens.len(),
        available_accounts,
    }
}

#[tauri::command]
pub async fn generate_auth_url(state: State<'_, AppState>) -> Result<String, String> {
    let augment_oauth_state = oauth::create_augment_oauth_state();
    let auth_url = oauth::generate_augment_authorize_url(&augment_oauth_state)
        .map_err(|e| format!("Failed to generate auth URL: {}", e))?;

    *state.augment_oauth_state.lock().unwrap() = Some(augment_oauth_state);

    Ok(auth_url)
}

#[tauri::command]
pub async fn generate_augment_auth_url(state: State<'_, AppState>) -> Result<String, String> {
    let augment_oauth_state = oauth::create_augment_oauth_state();
    let auth_url = oauth::generate_augment_authorize_url(&augment_oauth_state)
        .map_err(|e| format!("Failed to generate Augment auth URL: {}", e))?;

    *state.augment_oauth_state.lock().unwrap() = Some(augment_oauth_state);

    Ok(auth_url)
}

#[tauri::command]
pub async fn get_token(
    code: String,
    state: State<'_, AppState>,
) -> Result<AugmentTokenResponse, String> {
    let augment_oauth_state = {
        let guard = state.augment_oauth_state.lock().unwrap();
        guard
            .clone()
            .ok_or("No Augment OAuth state found. Please generate auth URL first.")?
    };

    oauth::complete_augment_oauth_flow(&augment_oauth_state, &code)
        .await
        .map_err(|e| format!("Failed to complete OAuth flow: {}", e))
}

#[tauri::command]
pub async fn get_augment_token(
    code: String,
    state: State<'_, AppState>,
) -> Result<AugmentTokenResponse, String> {
    let augment_oauth_state = {
        let guard = state.augment_oauth_state.lock().unwrap();
        guard
            .clone()
            .ok_or("No Augment OAuth state found. Please generate auth URL first.")?
    };

    oauth::complete_augment_oauth_flow(&augment_oauth_state, &code)
        .await
        .map_err(|e| format!("Failed to complete Augment OAuth flow: {}", e))
}

#[tauri::command]
pub async fn get_augment_proxy_status(
    state: State<'_, AppState>,
) -> Result<AugmentProxyStatus, String> {
    let api_server_port = {
        let guard = state.api_server.lock().unwrap();
        guard.as_ref().map(|server| server.get_port())
    };

    let tokens = {
        let storage = {
            let guard = state.storage_manager.lock().unwrap();
            guard.as_ref().cloned()
        };

        match storage {
            Some(storage) => storage
                .load_tokens()
                .await
                .map_err(|e| format!("Failed to load Augment tokens: {}", e))?,
            None => Vec::new(),
        }
    };

    let (sidecar_configured, sidecar_running, health_probe) = {
        let guard = state.augment_sidecar.lock().await;
        match guard.as_ref() {
            Some(sidecar) => {
                let running = sidecar.is_running();
                let probe = if running {
                    Some((sidecar.base_url(), sidecar.api_key().to_string()))
                } else {
                    None
                };
                (true, running, probe)
            }
            None => (false, false, None),
        }
    };

    let sidecar_healthy = if let Some((base_url, api_key)) = health_probe {
        super::sidecar::probe_sidecar_health(&base_url, &api_key).await
    } else {
        false
    };

    Ok(summarize_augment_proxy_status(
        api_server_port,
        sidecar_configured,
        sidecar_running,
        sidecar_healthy,
        &tokens,
    ))
}

#[tauri::command]
pub async fn get_augment_gateway_access_config(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AugmentGatewayAccessConfig, String> {
    let profiles = crate::core::gateway_access::get_or_load_gateway_access_profiles(
        &app,
        state.inner(),
    )?;
    let api_key = match profiles
        .profiles
        .iter()
        .find(|profile| profile.target == crate::core::gateway_access::GatewayTarget::Augment)
        .map(|profile| profile.api_key.clone())
        .and_then(|value| normalize_api_key(Some(value)))
    {
        Some(existing) => existing,
        None => ensure_augment_gateway_profile(&app, state.inner(), None)?,
    };

    Ok(build_augment_gateway_access_config(
        gateway_base_url_for_port(
            current_api_server_port(state.inner()).unwrap_or(DEFAULT_SHARED_API_SERVER_PORT),
        ),
        api_key,
    ))
}

#[tauri::command]
pub async fn set_augment_gateway_access_config(
    app: AppHandle,
    state: State<'_, AppState>,
    api_key: Option<String>,
) -> Result<AugmentGatewayAccessConfig, String> {
    let api_key = ensure_augment_gateway_profile(&app, state.inner(), api_key)?;

    Ok(build_augment_gateway_access_config(
        gateway_base_url_for_port(
            current_api_server_port(state.inner()).unwrap_or(DEFAULT_SHARED_API_SERVER_PORT),
        ),
        api_key,
    ))
}

#[tauri::command]
pub async fn check_account_status(
    token: String,
    tenant_url: String,
) -> Result<AccountStatus, String> {
    account::check_account_ban_status(&token, &tenant_url)
        .await
        .map_err(|e| format!("Failed to check account status: {}", e))
}

#[tauri::command]
pub async fn batch_check_tokens_status(
    tokens: Vec<TokenInfo>,
    state: State<'_, AppState>,
) -> Result<Vec<TokenStatusResult>, String> {
    account::batch_check_account_status(tokens, state.app_session_cache.clone())
        .await
        .map_err(|e| format!("Failed to batch check tokens status: {}", e))
}

#[tauri::command]
pub async fn batch_check_tokens_status_simple(
    tokens: Vec<TokenInfo>,
) -> Result<Vec<TokenStatusResult>, String> {
    account::batch_check_account_status_simple(tokens)
        .await
        .map_err(|e| format!("Failed to batch check tokens status (simple): {}", e))
}

/// 批量获取 Credit 消费数据(stats 和 chart),使用缓存的 app_session
#[tauri::command]
pub async fn fetch_batch_credit_consumption(
    auth_session: String,
    state: State<'_, AppState>,
) -> Result<BatchCreditConsumptionResponse, String> {
    println!("fetch_batch_credit_consumption called");
    let cached_app_session = {
        let cache = state.app_session_cache.lock().unwrap();
        cache.get(&auth_session).map(|c| c.app_session.clone())
    };

    if let Some(app_session) = cached_app_session {
        println!("Using cached app_session for credit consumption");

        match account::get_batch_credit_consumption_with_app_session(&app_session).await {
            Ok(result) => {
                println!("Successfully fetched credit data with cached app_session");
                return Ok(result);
            }
            Err(e) => {
                println!("Cached app_session failed: {}, will refresh", e);
            }
        }
    }

    println!("Exchanging auth_session for new app_session...");
    let app_session = account::exchange_auth_session_for_app_session(&auth_session).await?;
    println!(
        "New app session obtained: {}",
        &app_session[..20.min(app_session.len())]
    );

    {
        let mut cache = state.app_session_cache.lock().unwrap();
        cache.insert(
            auth_session.clone(),
            AppSessionCache {
                app_session: app_session.clone(),
                created_at: SystemTime::now(),
            },
        );
        println!("App session cached for future use");
    }

    account::get_batch_credit_consumption_with_app_session(&app_session).await
}

#[tauri::command]
pub async fn add_token_from_session(
    session: String,
    app: AppHandle,
    _state: State<'_, AppState>,
) -> Result<TokenFromSessionResponse, String> {
    let _ = app.emit("session-import-progress", "sessionImportExtractingToken");
    let token_response = oauth::extract_token_from_session(&session).await?;

    let _ = app.emit("session-import-progress", "sessionImportComplete");

    Ok(TokenFromSessionResponse {
        access_token: token_response.access_token,
        tenant_url: token_response.tenant_url,
        email: token_response.email,
    })
}

#[tauri::command]
pub async fn fetch_payment_method_link_command(
    auth_session: String,
) -> Result<PaymentMethodLinkResult, String> {
    let full_cookies = account::exchange_auth_session_for_full_cookies(&auth_session).await?;
    let payment_link = account::fetch_payment_method_link(&full_cookies).await?;

    Ok(PaymentMethodLinkResult {
        payment_method_link: payment_link,
    })
}

/// Session 刷新请求
#[derive(serde::Deserialize)]
pub struct SessionRefreshRequest {
    pub id: String,
    pub session: String,
}

/// 批量刷新 auth_session
/// 前端传入 { id, session } 列表，后端只负责刷新并返回新的 session
#[tauri::command]
pub async fn batch_refresh_sessions(
    requests: Vec<SessionRefreshRequest>,
) -> Result<Vec<SessionRefreshResult>, String> {
    let mut results = Vec::new();

    for request in requests {
        match oauth::refresh_auth_session(&request.session).await {
            Ok(new_session) => {
                results.push(SessionRefreshResult {
                    token_id: request.id,
                    success: true,
                    error: None,
                    new_session: Some(new_session),
                });
            }
            Err(e) => {
                results.push(SessionRefreshResult {
                    token_id: request.id,
                    success: false,
                    error: Some(e.to_string()),
                    new_session: None,
                });
            }
        }
    }

    Ok(results)
}

// ============ 团队管理相关 Commands ============

/// 获取团队信息
#[tauri::command]
pub async fn fetch_team_info(
    auth_session: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let app_session = get_or_refresh_app_session(&auth_session, &state).await?;
    account::fetch_team_info(&app_session).await
}

/// 修改团队席位数
#[tauri::command]
pub async fn update_team_seats(
    auth_session: String,
    seats: u32,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let app_session = get_or_refresh_app_session(&auth_session, &state).await?;
    account::update_team_seats(&app_session, seats).await
}

/// 邀请团队成员
#[tauri::command]
pub async fn invite_team_members(
    auth_session: String,
    emails: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let app_session = get_or_refresh_app_session(&auth_session, &state).await?;
    account::invite_team_members(&app_session, emails).await
}

/// 删除团队成员
#[tauri::command]
pub async fn delete_team_member(
    auth_session: String,
    user_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let app_session = get_or_refresh_app_session(&auth_session, &state).await?;
    account::delete_team_member(&app_session, &user_id).await
}

/// 删除团队邀请
#[tauri::command]
pub async fn delete_team_invitation(
    auth_session: String,
    invitation_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let app_session = get_or_refresh_app_session(&auth_session, &state).await?;
    account::delete_team_invitation(&app_session, &invitation_id).await
}

// ============ 辅助函数 ============

/// 获取或刷新 app_session
async fn get_or_refresh_app_session(
    auth_session: &str,
    state: &State<'_, AppState>,
) -> Result<String, String> {
    // 先检查缓存
    let cached = {
        let cache = state.app_session_cache.lock().unwrap();
        cache.get(auth_session).map(|c| c.app_session.clone())
    };

    if let Some(app_session) = cached {
        return Ok(app_session);
    }

    // 缓存未命中，交换新的 app_session
    let app_session = account::exchange_auth_session_for_app_session(auth_session).await?;

    // 存入缓存
    {
        let mut cache = state.app_session_cache.lock().unwrap();
        cache.insert(
            auth_session.to_string(),
            AppSessionCache {
                app_session: app_session.clone(),
                created_at: SystemTime::now(),
            },
        );
    }

    Ok(app_session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    fn sample_storage_token() -> crate::storage::TokenData {
        let now = Utc.with_ymd_and_hms(2026, 3, 14, 8, 0, 0).unwrap();
        crate::storage::TokenData {
            id: "token-1".to_string(),
            tenant_url: "https://tenant.augmentcode.com/".to_string(),
            access_token: "access-token-1".to_string(),
            created_at: now,
            updated_at: now,
            portal_url: None,
            email_note: Some("user@example.com".to_string()),
            tag_name: None,
            tag_color: None,
            ban_status: Some(json!("ACTIVE")),
            portal_info: Some(json!({
                "credits_balance": 4000,
                "credit_total": 4000,
                "expiry_date": "2027-03-14T00:00:00Z"
            })),
            auth_session: None,
            suspensions: None,
            balance_color_mode: None,
            skip_check: None,
            session_updated_at: None,
            version: 0,
        }
    }

    #[test]
    fn augment_proxy_status_counts_only_usable_accounts() {
        let usable = sample_storage_token();

        let mut banned = sample_storage_token();
        banned.id = "token-2".to_string();
        banned.ban_status = Some(json!("BANNED-fraud"));

        let mut expired = sample_storage_token();
        expired.id = "token-3".to_string();
        expired.portal_info = Some(json!({
            "credits_balance": 4000,
            "credit_total": 4000,
            "expiry_date": "2024-03-14T00:00:00Z"
        }));

        let mut zero_credit = sample_storage_token();
        zero_credit.id = "token-4".to_string();
        zero_credit.portal_info = Some(json!({
            "credits_balance": 0,
            "credit_total": 4000,
            "expiry_date": "2027-03-14T00:00:00Z"
        }));

        let status = summarize_augment_proxy_status(
            None,
            true,
            false,
            false,
            &[usable, banned, expired, zero_credit],
        );

        assert!(!status.api_server_running);
        assert_eq!(status.proxy_base_url, "http://127.0.0.1:8766/v1");
        assert_eq!(status.total_accounts, 4);
        assert_eq!(status.available_accounts, 1);
    }

    #[test]
    fn augment_proxy_status_marks_sidecar_unconfigured_when_manager_missing() {
        let status = summarize_augment_proxy_status(None, false, false, false, &[]);

        assert!(!status.sidecar_configured);
        assert!(!status.sidecar_running);
        assert!(!status.sidecar_healthy);
        assert_eq!(status.proxy_base_url, "http://127.0.0.1:8766/v1");
    }

    #[test]
    fn augment_proxy_status_uses_running_api_server_address_when_present() {
        let status = summarize_augment_proxy_status(Some(9123), true, true, true, &[]);

        assert!(status.api_server_running);
        assert_eq!(
            status.api_server_address.as_deref(),
            Some("http://127.0.0.1:9123")
        );
        assert_eq!(status.proxy_base_url, "http://127.0.0.1:9123/v1");
        assert!(status.sidecar_configured);
        assert!(status.sidecar_running);
        assert!(status.sidecar_healthy);
    }

    #[test]
    fn augment_gateway_snippets_use_unified_v1_base_url() {
        let config = build_augment_gateway_access_config(
            "http://127.0.0.1:8766/v1".to_string(),
            "sk-auggie".to_string(),
        );

        assert_eq!(config.base_url, "http://127.0.0.1:8766/v1");
        assert_eq!(config.api_key.as_deref(), Some("sk-auggie"));
        assert!(
            config
                .curl_example
                .contains("http://127.0.0.1:8766/v1/chat/completions")
        );
        assert!(config.curl_example.contains("\"model\": \"gemini-3.1-pro\""));
        assert!(
            config
                .auth_pool_snippet
                .contains("\"OPENAI_API_KEY_POOL_AUGMENT_1\": \"sk-auggie\"")
        );
        assert!(
            config
                .config_pool_snippet
                .contains("base_url = \"http://127.0.0.1:8766/v1\"")
        );
        assert!(
            config
                .config_pool_snippet
                .contains("env_key = \"OPENAI_API_KEY_POOL_AUGMENT_1\"")
        );
    }

    #[test]
    fn augment_gateway_generated_keys_use_sk_prefix() {
        let first = generate_gateway_api_key();
        let second = generate_gateway_api_key();

        assert!(first.starts_with("sk-"));
        assert!(second.starts_with("sk-"));
        assert_ne!(first, second);
    }
}
