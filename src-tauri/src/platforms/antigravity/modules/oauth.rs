use crate::antigravity::models::token::{TokenResponse, UserInfo};
use crate::http_client::create_proxy_client;

const CLIENT_ID_ENV: &str = "ATM_ANTIGRAVITY_OAUTH_CLIENT_ID";
const CLIENT_SECRET_ENV: &str = "ATM_ANTIGRAVITY_OAUTH_CLIENT_SECRET";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USER_INFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";

fn required_env(name: &str) -> Result<String, String> {
    let value = std::env::var(name)
        .map_err(|_| format!("Missing required Antigravity OAuth environment variable: {}", name))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!(
            "Antigravity OAuth environment variable is empty: {}",
            name
        ));
    }
    Ok(trimmed.to_string())
}

fn oauth_client_id() -> Result<String, String> {
    required_env(CLIENT_ID_ENV)
}

fn oauth_client_secret() -> Result<String, String> {
    required_env(CLIENT_SECRET_ENV)
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
