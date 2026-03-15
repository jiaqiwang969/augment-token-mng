use crate::antigravity::models::token::{TokenResponse, UserInfo};
use crate::http_client::create_proxy_client;

const CLIENT_ID_ENV: &str = "ATM_ANTIGRAVITY_OAUTH_CLIENT_ID";
const CLIENT_SECRET_ENV: &str = "ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET";
const LEGACY_CLIENT_ID_ENV: &str = "CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_ID";
const LEGACY_CLIENT_SECRET_ENV: &str = "CLIPROXY_ANTIGRAVITY_OAUTH_CLIENT_SECRET";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USER_INFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";

enum EnvValue {
    Missing,
    Empty,
    Present(String),
}

fn read_env_value(name: &str) -> EnvValue {
    match std::env::var(name) {
        Ok(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                EnvValue::Empty
            } else {
                EnvValue::Present(trimmed.to_string())
            }
        }
        Err(_) => EnvValue::Missing,
    }
}

fn missing_env_error(primary: &str, legacy: &str) -> String {
    format!(
        "Missing required Antigravity OAuth environment variable: {} (legacy fallback: {}). Add it to .env.antigravity or export it before running make dev.",
        primary, legacy
    )
}

fn empty_env_error(primary: &str) -> String {
    format!(
        "Antigravity OAuth environment variable is empty: {}. Set it in .env.antigravity or export it before running make dev.",
        primary
    )
}

fn empty_legacy_env_error(primary: &str, legacy: &str) -> String {
    format!(
        "Antigravity OAuth legacy environment variable is empty: {}. Prefer configuring {} in .env.antigravity.",
        legacy, primary
    )
}

fn required_env(primary: &str, legacy: &str) -> Result<String, String> {
    match read_env_value(primary) {
        EnvValue::Present(value) => return Ok(value),
        EnvValue::Empty => return Err(empty_env_error(primary)),
        EnvValue::Missing => {}
    }

    match read_env_value(legacy) {
        EnvValue::Present(value) => Ok(value),
        EnvValue::Empty => Err(empty_legacy_env_error(primary, legacy)),
        EnvValue::Missing => Err(missing_env_error(primary, legacy)),
    }
}

fn oauth_client_id() -> Result<String, String> {
    required_env(CLIENT_ID_ENV, LEGACY_CLIENT_ID_ENV)
}

fn oauth_client_secret() -> Result<String, String> {
    required_env(CLIENT_SECRET_ENV, LEGACY_CLIENT_SECRET_ENV)
}

/// 生成 OAuth 授权 URL
pub fn get_auth_url(redirect_uri: &str) -> Result<String, String> {
    let client_id = oauth_client_id()?;

    // 使用 Antigravity 需要的完整 scopes
    let scopes = vec![
        "https://www.googleapis.com/auth/cloud-platform",
        "https://www.googleapis.com/auth/userinfo.email",
        "https://www.googleapis.com/auth/userinfo.profile",
        "https://www.googleapis.com/auth/cclog",
        "https://www.googleapis.com/auth/experimentsandconfigs",
    ]
    .join(" ");

    let params = vec![
        ("client_id", client_id),
        ("redirect_uri", redirect_uri.to_string()),
        ("response_type", "code".to_string()),
        ("scope", scopes),
        ("access_type", "offline".to_string()),
        ("prompt", "consent".to_string()),
        ("include_granted_scopes", "true".to_string()),
    ];

    let url = url::Url::parse_with_params(AUTH_URL, &params)
        .map_err(|e| format!("Invalid Auth URL: {}", e))?;
    Ok(url.to_string())
}

/// 使用 Authorization Code 交换 Token
pub async fn exchange_code(code: &str, redirect_uri: &str) -> Result<TokenResponse, String> {
    let client = create_proxy_client()?;
    let client_id = oauth_client_id()?;
    let client_secret = oauth_client_secret()?;

    let params = vec![
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code", code.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("grant_type", "authorization_code".to_string()),
    ];

    let response = client
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token exchange request failed: {}", e))?;

    if response.status().is_success() {
        response
            .json::<TokenResponse>()
            .await
            .map_err(|e| format!("Failed to parse token response: {}", e))
    } else {
        let error_text = response.text().await.unwrap_or_default();
        Err(format!("Token exchange failed: {}", error_text))
    }
}

/// 使用 refresh_token 刷新 access_token
pub async fn refresh_access_token(refresh_token: &str) -> Result<TokenResponse, String> {
    let client = create_proxy_client()?;
    let client_id = oauth_client_id()?;
    let client_secret = oauth_client_secret()?;

    let params = vec![
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("refresh_token", refresh_token.to_string()),
        ("grant_type", "refresh_token".to_string()),
    ];

    let response = client
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Refresh request failed: {}", e))?;

    if response.status().is_success() {
        let mut token_data = response
            .json::<TokenResponse>()
            .await
            .map_err(|e| format!("Failed to parse refresh response: {}", e))?;

        // 刷新时可能不返回新的 refresh_token，保留原有的
        if token_data.refresh_token.is_none() {
            token_data.refresh_token = Some(refresh_token.to_string());
        }

        Ok(token_data)
    } else {
        let error_text = response.text().await.unwrap_or_default();
        Err(format!("Refresh failed: {}", error_text))
    }
}

/// 获取用户信息
pub async fn get_user_info(access_token: &str) -> Result<UserInfo, String> {
    let client = create_proxy_client()?;

    let response = client
        .get(USER_INFO_URL)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("User info request failed: {}", e))?;

    if response.status().is_success() {
        response
            .json::<UserInfo>()
            .await
            .map_err(|e| format!("Failed to parse user info: {}", e))
    } else {
        let error_text = response.text().await.unwrap_or_default();
        Err(format!("Get user info failed: {}", error_text))
    }
}

/// 检查并在需要时刷新 Token
/// 返回最新的 TokenData
pub async fn ensure_fresh_token(
    current_token: &crate::antigravity::models::TokenData,
) -> Result<crate::antigravity::models::TokenData, String> {
    let now = chrono::Utc::now().timestamp();

    // 如果还有超过 5 分钟有效期，直接返回
    if current_token.expiry_timestamp > now + 300 {
        return Ok(current_token.clone());
    }

    // 需要刷新
    let response = refresh_access_token(&current_token.refresh_token).await?;

    // 构造新 TokenData
    Ok(crate::antigravity::models::TokenData::new(
        response.access_token,
        response
            .refresh_token
            .unwrap_or(current_token.refresh_token.clone()),
        response.expires_in,
        current_token.email.clone(),
        current_token.project_id.clone(),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn clear_env(names: &[&str]) {
        for name in names {
            unsafe {
                std::env::remove_var(name);
            }
        }
    }

    #[test]
    fn primary_env_takes_precedence_over_legacy() {
        let _guard = env_lock();
        clear_env(&[
            CLIENT_ID_ENV,
            LEGACY_CLIENT_ID_ENV,
            CLIENT_SECRET_ENV,
            LEGACY_CLIENT_SECRET_ENV,
        ]);

        unsafe {
            std::env::set_var(CLIENT_ID_ENV, "atm-client-id");
            std::env::set_var(LEGACY_CLIENT_ID_ENV, "legacy-client-id");
        }

        assert_eq!(oauth_client_id().unwrap(), "atm-client-id");

        clear_env(&[
            CLIENT_ID_ENV,
            LEGACY_CLIENT_ID_ENV,
            CLIENT_SECRET_ENV,
            LEGACY_CLIENT_SECRET_ENV,
        ]);
    }

    #[test]
    fn legacy_env_is_used_when_primary_is_missing() {
        let _guard = env_lock();
        clear_env(&[
            CLIENT_ID_ENV,
            LEGACY_CLIENT_ID_ENV,
            CLIENT_SECRET_ENV,
            LEGACY_CLIENT_SECRET_ENV,
        ]);

        unsafe {
            std::env::set_var(LEGACY_CLIENT_SECRET_ENV, "legacy-client-secret");
        }

        assert_eq!(oauth_client_secret().unwrap(), "legacy-client-secret");

        clear_env(&[
            CLIENT_ID_ENV,
            LEGACY_CLIENT_ID_ENV,
            CLIENT_SECRET_ENV,
            LEGACY_CLIENT_SECRET_ENV,
        ]);
    }

    #[test]
    fn missing_env_error_mentions_primary_and_legacy_names() {
        let _guard = env_lock();
        clear_env(&[
            CLIENT_ID_ENV,
            LEGACY_CLIENT_ID_ENV,
            CLIENT_SECRET_ENV,
            LEGACY_CLIENT_SECRET_ENV,
        ]);

        let error = oauth_client_id().unwrap_err();
        assert!(error.contains(CLIENT_ID_ENV));
        assert!(error.contains(LEGACY_CLIENT_ID_ENV));
        assert!(error.contains(".env.antigravity"));
    }
}
