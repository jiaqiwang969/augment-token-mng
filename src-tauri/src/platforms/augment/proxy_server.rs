//! Augment API 代理路由
//!
//! 将 /augment/v1/* 请求转发到 CLIProxyAPI sidecar，实现 Codex CLI → Augment 的透明代理。

use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use hyper::{Body, Response};
use std::sync::Arc;
use warp::http::{HeaderMap, Method, StatusCode};
use warp::path::FullPath;
use warp::{Filter, Rejection, Reply};

use crate::AppState;
use crate::data::storage::augment::traits::TokenStorage;

/// Augment 代理路由
pub fn augment_routes_from_state(
    state: Arc<AppState>,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let state_filter = warp::any().map(move || state.clone());

    // 匹配 /augment/* 下的所有请求
    warp::path("augment")
        .and(warp::path::full())
        .and(warp::method())
        .and(optional_raw_query())
        .and(warp::header::headers_cloned())
        .and(warp::body::content_length_limit(20 * 1024 * 1024))
        .and(warp::body::bytes())
        .and(state_filter)
        .and_then(handle_augment_proxy)
}

fn optional_raw_query()
-> impl Filter<Extract = (Option<String>,), Error = std::convert::Infallible> + Clone {
    warp::query::raw()
        .map(Some)
        .or(warp::any().map(|| None))
        .unify()
}

fn inner_path(raw_path: &str) -> String {
    raw_path
        .strip_prefix("/augment")
        .unwrap_or(raw_path)
        .to_string()
}

fn build_upstream_url(base_url: &str, path: &str, raw_query: Option<&str>) -> String {
    let mut upstream_url = format!("{}{}", base_url.trim_end_matches('/'), path);
    if let Some(query) = raw_query.filter(|query| !query.is_empty()) {
        upstream_url.push('?');
        upstream_url.push_str(query);
    }
    upstream_url
}

fn should_forward_request_header(name: &str) -> bool {
    !name.eq_ignore_ascii_case("host")
        && !name.eq_ignore_ascii_case("authorization")
        && !name.eq_ignore_ascii_case("content-length")
}

async fn handle_augment_proxy(
    full_path: FullPath,
    method: Method,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
    state: Arc<AppState>,
) -> Result<Box<dyn Reply>, Rejection> {
    // 从 /augment/v1/responses 提取 /v1/responses
    let raw_path = full_path.as_str();
    let inner_path = inner_path(raw_path);

    println!(
        "[AugmentProxy] {} {} → sidecar {}",
        method, raw_path, inner_path
    );

    // 获取可用的 Augment 账号
    let tokens = get_available_tokens(&state)
        .await
        .map_err(|e| warp::reject::custom(AugmentProxyRejection::NoAccounts(e)))?;

    if tokens.is_empty() {
        return Err(warp::reject::custom(AugmentProxyRejection::NoAccounts(
            "No available Augment accounts".into(),
        )));
    }

    // 确保 sidecar 正在运行
    let (base_url, api_key) = {
        let mut guard = state.augment_sidecar.lock().await;
        let sidecar = guard.as_mut().ok_or_else(|| {
            warp::reject::custom(AugmentProxyRejection::SidecarNotReady(
                "Sidecar not initialized (cliproxy-server binary not found)".into(),
            ))
        })?;
        sidecar
            .ensure_running(&tokens)
            .await
            .map_err(|e| warp::reject::custom(AugmentProxyRejection::SidecarNotReady(e)))?;
        (sidecar.base_url(), sidecar.api_key().to_string())
    };

    // 构建转发 URL
    let upstream_url = build_upstream_url(&base_url, &inner_path, query.as_deref());

    // 转发请求到 sidecar
    let client = reqwest::Client::new();
    let mut req_builder = client.request(
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::POST),
        &upstream_url,
    );

    // 复制请求头，替换 Authorization
    for (name, value) in headers.iter() {
        if !should_forward_request_header(name.as_str()) {
            continue;
        }
        req_builder = req_builder.header(name, value);
    }
    req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));

    if !body.is_empty() {
        req_builder = req_builder.body(body);
    }

    let upstream_response = req_builder.send().await.map_err(|e| {
        warp::reject::custom(AugmentProxyRejection::UpstreamError(format!(
            "Failed to forward to sidecar: {}",
            e
        )))
    })?;

    let upstream_status = StatusCode::from_u16(upstream_response.status().as_u16())
        .unwrap_or(StatusCode::BAD_GATEWAY);
    let upstream_headers = upstream_response.headers().clone();

    // 判断是否是 SSE 流式响应
    let is_stream = upstream_headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map_or(false, |ct| ct.contains("text/event-stream"));

    if is_stream {
        let response =
            build_streaming_response(upstream_status, &upstream_headers, upstream_response)
                .map_err(|e| warp::reject::custom(AugmentProxyRejection::UpstreamError(e)))?;
        return Ok(Box::new(response) as Box<dyn Reply>);
    }

    // 非流式：直接返回
    let body_bytes = upstream_response.bytes().await.map_err(|e| {
        warp::reject::custom(AugmentProxyRejection::UpstreamError(format!(
            "Failed to read sidecar response: {}",
            e
        )))
    })?;

    let mut builder = Response::builder().status(upstream_status);
    for (name, value) in upstream_headers.iter() {
        let n = name.as_str().to_lowercase();
        if n == "transfer-encoding" || n == "connection" {
            continue;
        }
        builder = builder.header(name, value);
    }

    let response = builder
        .body(Body::from(body_bytes))
        .map_err(|e| warp::reject::custom(AugmentProxyRejection::UpstreamError(e.to_string())))?;

    Ok(Box::new(response) as Box<dyn Reply>)
}

fn build_streaming_response(
    status: StatusCode,
    headers: &HeaderMap,
    response: reqwest::Response,
) -> Result<Response<Body>, String> {
    let mut builder = Response::builder().status(status);
    for (name, value) in headers.iter() {
        let n = name.as_str().to_lowercase();
        if n == "transfer-encoding" || n == "connection" {
            continue;
        }
        builder = builder.header(name, value);
    }

    let mut upstream_stream = response.bytes_stream();
    let (mut tx, rx) = futures::channel::mpsc::channel::<Result<Bytes, std::io::Error>>(16);

    tokio::spawn(async move {
        while let Some(chunk) = upstream_stream.next().await {
            match chunk {
                Ok(bytes) => {
                    if tx.send(Ok(bytes)).await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx
                        .send(Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            err.to_string(),
                        )))
                        .await;
                    break;
                }
            }
        }
    });

    let body = Body::wrap_stream(rx);
    builder.body(body).map_err(|e| e.to_string())
}

async fn get_available_tokens(state: &AppState) -> Result<Vec<crate::storage::TokenData>, String> {
    let storage = {
        let guard = state.storage_manager.lock().unwrap();
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "Augment storage not initialized".to_string())?
    };

    let tokens = storage
        .load_tokens()
        .await
        .map_err(|e| format!("Failed to load tokens: {}", e))?;

    // 过滤可用账号
    Ok(tokens.into_iter().filter(is_token_usable).collect())
}

fn is_token_usable(token: &crate::storage::TokenData) -> bool {
    !token.access_token.trim().is_empty()
        && !token.tenant_url.trim().is_empty()
        && !is_banned(token)
}

fn is_banned(token: &crate::storage::TokenData) -> bool {
    let status = token.ban_status.as_ref().and_then(|ban| ban.as_str());
    matches!(status, Some("SUSPENDED" | "INVALID_TOKEN"))
}

// ==================== 错误类型 ====================

#[derive(Debug)]
pub enum AugmentProxyRejection {
    NoAccounts(String),
    SidecarNotReady(String),
    UpstreamError(String),
}

impl warp::reject::Reject for AugmentProxyRejection {}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    fn sample_token() -> crate::storage::TokenData {
        let now = Utc.with_ymd_and_hms(2026, 3, 13, 1, 2, 3).unwrap();

        crate::storage::TokenData {
            id: "token-1".to_string(),
            tenant_url: "https://tenant.augmentcode.com/".to_string(),
            access_token: "access-token-1".to_string(),
            created_at: now,
            updated_at: now,
            portal_url: None,
            email_note: None,
            tag_name: None,
            tag_color: None,
            ban_status: None,
            portal_info: None,
            auth_session: None,
            suspensions: None,
            balance_color_mode: None,
            skip_check: None,
            session_updated_at: None,
            version: 0,
        }
    }

    #[test]
    fn inner_path_strips_augment_prefix() {
        assert_eq!(inner_path("/augment/v1/responses"), "/v1/responses");
        assert_eq!(
            inner_path("/augment/v1/chat/completions"),
            "/v1/chat/completions"
        );
        assert_eq!(inner_path("/v1/models"), "/v1/models");
    }

    #[test]
    fn upstream_url_preserves_raw_query() {
        assert_eq!(
            build_upstream_url("http://127.0.0.1:9000", "/v1/models", Some("a=1&b=2")),
            "http://127.0.0.1:9000/v1/models?a=1&b=2"
        );
        assert_eq!(
            build_upstream_url("http://127.0.0.1:9000", "/v1/models", None),
            "http://127.0.0.1:9000/v1/models"
        );
    }

    #[test]
    fn authorization_header_is_not_forwarded() {
        assert!(!should_forward_request_header("authorization"));
        assert!(!should_forward_request_header("Authorization"));
        assert!(!should_forward_request_header("content-length"));
        assert!(should_forward_request_header("x-request-id"));
    }

    #[test]
    fn token_without_access_token_is_not_usable() {
        let mut token = sample_token();
        token.access_token = "   ".to_string();

        assert!(!is_token_usable(&token));
    }

    #[test]
    fn token_without_tenant_url_is_not_usable() {
        let mut token = sample_token();
        token.tenant_url = "   ".to_string();

        assert!(!is_token_usable(&token));
    }

    #[test]
    fn suspended_token_is_not_usable() {
        let mut token = sample_token();
        token.ban_status = Some(json!("SUSPENDED"));

        assert!(!is_token_usable(&token));
    }

    #[test]
    fn invalid_token_status_is_not_usable() {
        let mut token = sample_token();
        token.ban_status = Some(json!("INVALID_TOKEN"));

        assert!(!is_token_usable(&token));
    }

    #[test]
    fn active_token_with_required_fields_is_usable() {
        let mut token = sample_token();
        token.ban_status = Some(json!("ACTIVE"));

        assert!(is_token_usable(&token));
    }
}
