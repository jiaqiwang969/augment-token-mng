//! Augment API 代理路由
//!
//! 处理 Augment 后端代理请求，并将统一 /v1/* 流量转发到 CLIProxyAPI sidecar。

use bytes::Bytes;
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use hyper::{Body, Response};
use std::sync::Arc;
use warp::http::{HeaderMap, Method, StatusCode};
use warp::{Filter, Rejection, Reply};

use crate::AppState;
use crate::data::storage::augment::DualStorage;
use crate::data::storage::augment::traits::TokenStorage;
use crate::platforms::augment::sidecar::AugmentSidecar;

#[derive(Clone)]
struct AugmentProxyState {
    storage_manager: Arc<std::sync::Mutex<Option<Arc<DualStorage>>>>,
    augment_sidecar: Arc<tokio::sync::Mutex<Option<AugmentSidecar>>>,
}

impl AugmentProxyState {
    fn from_app_state(state: &Arc<AppState>) -> Self {
        Self {
            storage_manager: state.storage_manager.clone(),
            augment_sidecar: state.augment_sidecar.clone(),
        }
    }
}

/// Augment 代理路由
pub fn augment_routes_from_state(
    state: Arc<AppState>,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    augment_routes(AugmentProxyState::from_app_state(&state))
}

#[cfg(test)]
fn unified_augment_routes(
    state: AugmentProxyState,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let state_filter = warp::any().map(move || state.clone());

    let models_route = unified_models_request_filter()
        .and(state_filter.clone())
        .and_then(|query, headers, body, state| {
            handle_augment_proxy("/v1/models".to_string(), Method::GET, query, headers, body, state)
        });

    let responses_route = unified_responses_request_filter()
        .and(state_filter.clone())
        .and_then(|query, headers, body, state| {
            handle_augment_proxy(
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
            handle_augment_proxy(
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

fn augment_routes(
    state: AugmentProxyState,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let state_filter = warp::any().map(move || state.clone());

    let models_route = models_request_filter().and(state_filter.clone()).and_then(
        |query, headers, body, state| {
            handle_augment_proxy(
                "/augment/v1/models".to_string(),
                Method::GET,
                query,
                headers,
                body,
                state,
            )
        },
    );

    let responses_route = responses_request_filter()
        .and(state_filter.clone())
        .and_then(|query, headers, body, state| {
            handle_augment_proxy(
                "/augment/v1/responses".to_string(),
                Method::POST,
                query,
                headers,
                body,
                state,
            )
        });

    let chat_completions_route = chat_completions_request_filter()
        .and(state_filter)
        .and_then(|query, headers, body, state| {
            handle_augment_proxy(
                "/augment/v1/chat/completions".to_string(),
                Method::POST,
                query,
                headers,
                body,
                state,
            )
        });

    models_route.or(responses_route).or(chat_completions_route)
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

fn models_request_filter()
-> impl Filter<Extract = (Option<String>, HeaderMap, Bytes), Error = Rejection> + Clone {
    warp::path!("augment" / "v1" / "models")
        .and(warp::get())
        .and(optional_raw_query())
        .and(warp::header::headers_cloned())
        .map(|query, headers| (query, headers, Bytes::new()))
        .untuple_one()
}

fn responses_request_filter()
-> impl Filter<Extract = (Option<String>, HeaderMap, Bytes), Error = Rejection> + Clone {
    proxy_post_request_filter(warp::path!("augment" / "v1" / "responses"))
}

fn chat_completions_request_filter()
-> impl Filter<Extract = (Option<String>, HeaderMap, Bytes), Error = Rejection> + Clone {
    proxy_post_request_filter(warp::path!("augment" / "v1" / "chat" / "completions"))
}

#[cfg(test)]
fn unified_models_request_filter()
-> impl Filter<Extract = (Option<String>, HeaderMap, Bytes), Error = Rejection> + Clone {
    warp::path!("v1" / "models")
        .and(warp::get())
        .and(optional_raw_query())
        .and(warp::header::headers_cloned())
        .map(|query, headers| (query, headers, Bytes::new()))
        .untuple_one()
}

#[cfg(test)]
fn unified_responses_request_filter()
-> impl Filter<Extract = (Option<String>, HeaderMap, Bytes), Error = Rejection> + Clone {
    proxy_post_request_filter(warp::path!("v1" / "responses"))
}

#[cfg(test)]
fn unified_chat_completions_request_filter()
-> impl Filter<Extract = (Option<String>, HeaderMap, Bytes), Error = Rejection> + Clone {
    proxy_post_request_filter(warp::path!("v1" / "chat" / "completions"))
}

fn proxy_post_request_filter<F>(
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

async fn handle_augment_proxy(
    raw_path: String,
    method: Method,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
    state: AugmentProxyState,
) -> Result<Box<dyn Reply>, Rejection> {
    let inner_path = inner_path(&raw_path);

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

pub(crate) async fn handle_unified_gateway_request(
    raw_path: String,
    method: Method,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
    state: Arc<AppState>,
) -> Result<Box<dyn Reply>, Rejection> {
    handle_augment_proxy(
        raw_path,
        method,
        query,
        headers,
        body,
        AugmentProxyState::from_app_state(&state),
    )
    .await
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

async fn get_available_tokens(
    state: &AugmentProxyState,
) -> Result<Vec<crate::storage::TokenData>, String> {
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

pub(crate) fn is_token_usable(token: &crate::storage::TokenData) -> bool {
    !token.access_token.trim().is_empty()
        && !token.tenant_url.trim().is_empty()
        && !is_banned(token)
        && has_available_credits(token)
        && !is_expired(token)
}

fn is_banned(token: &crate::storage::TokenData) -> bool {
    let Some(status) = token.ban_status.as_ref().and_then(|ban| ban.as_str()) else {
        return false;
    };

    let status = status.trim().to_ascii_uppercase();
    matches!(status.as_str(), "SUSPENDED" | "INVALID_TOKEN" | "BANNED")
        || status.starts_with("BANNED-")
}

fn has_available_credits(token: &crate::storage::TokenData) -> bool {
    let Some(balance) = token
        .portal_info
        .as_ref()
        .and_then(|info| info.get("credits_balance"))
    else {
        return true;
    };

    parse_portal_number(balance).is_some_and(|balance| balance > 0.0)
}

fn is_expired(token: &crate::storage::TokenData) -> bool {
    let Some(expiry_date) = token
        .portal_info
        .as_ref()
        .and_then(|info| info.get("expiry_date"))
        .and_then(|date| date.as_str())
    else {
        return false;
    };

    parse_portal_expiry(expiry_date)
        .map(|expiry| expiry <= Utc::now())
        .unwrap_or(false)
}

fn parse_portal_number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|value| value as f64))
        .or_else(|| value.as_u64().map(|value| value as f64))
        .or_else(|| {
            value
                .as_str()
                .and_then(|value| value.trim().parse::<f64>().ok())
        })
}

fn parse_portal_expiry(value: &str) -> Option<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|expiry| expiry.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|date| chrono::DateTime::<Utc>::from_naive_utc_and_offset(date, Utc))
        })
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
    use serde_json::{Value, json};
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    use crate::data::storage::augment::{DualStorage, LocalFileStorage};
    use crate::platforms::augment::sidecar::AugmentSidecar;

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
    fn banned_token_is_not_usable() {
        let mut token = sample_token();
        token.ban_status = Some(json!("BANNED"));

        assert!(!is_token_usable(&token));
    }

    #[test]
    fn banned_token_with_reason_is_not_usable() {
        let mut token = sample_token();
        token.ban_status = Some(json!("BANNED-fraud"));

        assert!(!is_token_usable(&token));
    }

    #[test]
    fn active_token_with_required_fields_is_usable() {
        let mut token = sample_token();
        token.ban_status = Some(json!("ACTIVE"));

        assert!(is_token_usable(&token));
    }

    #[test]
    fn token_with_zero_credits_is_not_usable() {
        let mut token = sample_token();
        token.portal_info = Some(json!({
            "credits_balance": 0,
            "credit_total": 4000,
            "expiry_date": "2026-04-05T00:00:00Z"
        }));

        assert!(!is_token_usable(&token));
    }

    #[test]
    fn token_with_positive_credits_is_usable() {
        let mut token = sample_token();
        token.portal_info = Some(json!({
            "credits_balance": 4000,
            "credit_total": 4000,
            "expiry_date": "2026-04-05T00:00:00Z"
        }));

        assert!(is_token_usable(&token));
    }

    #[test]
    fn token_with_string_zero_credits_is_not_usable() {
        let mut token = sample_token();
        token.portal_info = Some(json!({
            "credits_balance": "0.00",
            "credit_total": 4000,
            "expiry_date": "2026-04-05T00:00:00Z"
        }));

        assert!(!is_token_usable(&token));
    }

    #[test]
    fn token_with_string_positive_credits_is_usable() {
        let mut token = sample_token();
        token.portal_info = Some(json!({
            "credits_balance": "9.00",
            "credit_total": 4000,
            "expiry_date": "2026-04-05T00:00:00Z"
        }));

        assert!(is_token_usable(&token));
    }

    #[test]
    fn token_with_float_zero_credits_is_not_usable() {
        let mut token = sample_token();
        token.portal_info = Some(json!({
            "credits_balance": 0.0,
            "credit_total": 4000,
            "expiry_date": "2026-04-05T00:00:00Z"
        }));

        assert!(!is_token_usable(&token));
    }

    #[test]
    fn token_with_past_expiry_date_is_not_usable() {
        let mut token = sample_token();
        token.portal_info = Some(json!({
            "credits_balance": 4000,
            "credit_total": 4000,
            "expiry_date": "2024-04-05T00:00:00Z"
        }));

        assert!(!is_token_usable(&token));
    }

    #[test]
    fn token_with_date_only_past_expiry_date_is_not_usable() {
        let mut token = sample_token();
        token.portal_info = Some(json!({
            "credits_balance": 4000,
            "credit_total": 4000,
            "expiry_date": "2024-04-05"
        }));

        assert!(!is_token_usable(&token));
    }

    #[test]
    fn token_with_invalid_expiry_date_remains_usable() {
        let mut token = sample_token();
        token.portal_info = Some(json!({
            "credits_balance": 4000,
            "credit_total": 4000,
            "expiry_date": "not-a-date"
        }));

        assert!(is_token_usable(&token));
    }

    #[test]
    fn models_route_accepts_get_without_content_length() {
        let route = models_request_filter().map(
            |query: Option<String>, _headers: HeaderMap, body: Bytes| {
                assert_eq!(query, None);
                assert!(body.is_empty());
                warp::reply::with_status("ok", StatusCode::OK)
            },
        );

        let response = futures::executor::block_on(async {
            warp::test::request()
                .method("GET")
                .path("/augment/v1/models")
                .reply(&route)
                .await
        });

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.body(), "ok");
    }

    #[test]
    fn models_route_preserves_query_string_without_requiring_body() {
        let route = models_request_filter().map(
            |query: Option<String>, _headers: HeaderMap, body: Bytes| {
                assert_eq!(query.as_deref(), Some("limit=20"));
                assert!(body.is_empty());
                warp::reply::with_status("ok", StatusCode::OK)
            },
        );

        let response = futures::executor::block_on(async {
            warp::test::request()
                .method("GET")
                .path("/augment/v1/models?limit=20")
                .reply(&route)
                .await
        });

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn responses_route_requires_post() {
        let route = responses_request_filter().map(
            |_query: Option<String>, _headers: HeaderMap, _body: Bytes| {
                warp::reply::with_status("ok", StatusCode::OK)
            },
        );

        let response = futures::executor::block_on(async {
            warp::test::request()
                .method("GET")
                .path("/augment/v1/responses")
                .reply(&route)
                .await
        });

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[test]
    fn chat_completions_route_requires_post() {
        let route = chat_completions_request_filter().map(
            |_query: Option<String>, _headers: HeaderMap, _body: Bytes| {
                warp::reply::with_status("ok", StatusCode::OK)
            },
        );

        let response = futures::executor::block_on(async {
            warp::test::request()
                .method("GET")
                .path("/augment/v1/chat/completions")
                .reply(&route)
                .await
        });

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[test]
    fn augment_route_filters_do_not_match_management_path() {
        let route = models_request_filter()
            .map(
                |_query: Option<String>, _headers: HeaderMap, _body: Bytes| {
                    warp::reply::with_status("models", StatusCode::OK)
                },
            )
            .or(responses_request_filter().map(
                |_query: Option<String>, _headers: HeaderMap, _body: Bytes| {
                    warp::reply::with_status("responses", StatusCode::OK)
                },
            ))
            .unify()
            .or(chat_completions_request_filter().map(
                |_query: Option<String>, _headers: HeaderMap, _body: Bytes| {
                    warp::reply::with_status("chat", StatusCode::OK)
                },
            ))
            .unify();

        let response = futures::executor::block_on(async {
            warp::test::request()
                .method("GET")
                .path("/augment/v0/management")
                .reply(&route)
                .await
        });

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[cfg(unix)]
    fn write_fake_proxy_sidecar_binary(dir: &Path) -> PathBuf {
        let binary_path = dir.join("fake-proxy-sidecar.sh");
        let script = r#"#!/bin/sh
set -eu

CONFIG=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-config" ]; then
    CONFIG="$2"
    shift 2
    continue
  fi
  shift
done

PORT="$(awk '/^port:/ { print $2; exit }' "$CONFIG")"

python3 - "$PORT" <<'PY'
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

port = int(sys.argv[1])

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/v1/models"):
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"data":[{"id":"auggie-test"}]}')
            return

        self.send_response(404)
        self.end_headers()

    def do_POST(self):
        content_length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(content_length).decode("utf-8")

        if self.path.startswith("/v1/chat/completions"):
            payload = {
                "authorization": self.headers.get("Authorization"),
                "path": self.path,
                "body": body,
            }
            raw = json.dumps(payload).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)
            return

        if self.path.startswith("/v1/responses"):
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            self.wfile.write(b'data: {"ok":true}\n\n')
            self.wfile.flush()
            return

        self.send_response(404)
        self.end_headers()

    def log_message(self, fmt, *args):
        return

HTTPServer(("127.0.0.1", port), Handler).serve_forever()
PY
"#;

        fs::write(&binary_path, script).unwrap();
        let mut permissions = fs::metadata(&binary_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&binary_path, permissions).unwrap();
        binary_path
    }

    #[cfg(unix)]
    async fn build_proxy_test_state(temp_dir: &Path) -> AugmentProxyState {
        let local_storage = Arc::new(LocalFileStorage::new_with_path(
            temp_dir.join("proxy-test-tokens.json"),
        ));
        let storage_manager = Arc::new(DualStorage::new(local_storage, None));
        storage_manager.save_token(&sample_token()).await.unwrap();

        AugmentProxyState {
            storage_manager: Arc::new(Mutex::new(Some(storage_manager))),
            augment_sidecar: Arc::new(tokio::sync::Mutex::new(Some(AugmentSidecar::new(
                temp_dir,
                write_fake_proxy_sidecar_binary(temp_dir),
            )))),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn augment_chat_route_lazily_starts_sidecar_and_replaces_authorization() {
        let temp_dir = tempdir().unwrap();
        let state = build_proxy_test_state(temp_dir.path()).await;
        let route = augment_routes(state.clone());

        let response = warp::test::request()
            .method("POST")
            .path("/augment/v1/chat/completions?stream=false")
            .header("authorization", "Bearer user-token")
            .header("content-type", "application/json")
            .body(r#"{"model":"gpt-5","messages":[{"role":"user","content":"hi"}]}"#)
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::OK);

        let payload: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(payload["path"], "/v1/chat/completions?stream=false");

        let authorization = payload["authorization"].as_str().unwrap();
        assert!(authorization.starts_with("Bearer sk-atm-internal-"));
        assert_ne!(authorization, "Bearer user-token");
        assert!(
            payload["body"]
                .as_str()
                .unwrap()
                .contains(r#""model":"gpt-5""#)
        );

        let mut guard = state.augment_sidecar.lock().await;
        if let Some(sidecar) = guard.as_mut() {
            sidecar.stop().await;
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn augment_responses_route_preserves_sse_streams() {
        let temp_dir = tempdir().unwrap();
        let state = build_proxy_test_state(temp_dir.path()).await;
        let route = augment_routes(state.clone());

        let response = warp::test::request()
            .method("POST")
            .path("/augment/v1/responses")
            .header("content-type", "application/json")
            .body(r#"{"model":"gpt-5","input":"hi","stream":true}"#)
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "text/event-stream");
        assert_eq!(response.body(), "data: {\"ok\":true}\n\n");

        let mut guard = state.augment_sidecar.lock().await;
        if let Some(sidecar) = guard.as_mut() {
            sidecar.stop().await;
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn augment_unified_chat_route_uses_v1_path_without_public_prefix() {
        let temp_dir = tempdir().unwrap();
        let state = build_proxy_test_state(temp_dir.path()).await;
        let route = unified_augment_routes(state.clone());

        let response = warp::test::request()
            .method("POST")
            .path("/v1/chat/completions?stream=false")
            .header("authorization", "Bearer user-token")
            .header("content-type", "application/json")
            .body(r#"{"model":"gpt-5","messages":[{"role":"user","content":"hi"}]}"#)
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::OK);

        let payload: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(payload["path"], "/v1/chat/completions?stream=false");

        let authorization = payload["authorization"].as_str().unwrap();
        assert!(authorization.starts_with("Bearer sk-atm-internal-"));
        assert_ne!(authorization, "Bearer user-token");

        let mut guard = state.augment_sidecar.lock().await;
        if let Some(sidecar) = guard.as_mut() {
            sidecar.stop().await;
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn augment_unified_models_route_returns_503_without_available_accounts() {
        let temp_dir = tempdir().unwrap();
        let local_storage = Arc::new(LocalFileStorage::new_with_path(
            temp_dir.path().join("proxy-test-empty-tokens.json"),
        ));
        let storage_manager = Arc::new(DualStorage::new(local_storage, None));
        let state = AugmentProxyState {
            storage_manager: Arc::new(Mutex::new(Some(storage_manager))),
            augment_sidecar: Arc::new(tokio::sync::Mutex::new(Some(AugmentSidecar::new(
                temp_dir.path(),
                write_fake_proxy_sidecar_binary(temp_dir.path()),
            )))),
        };
        let route = unified_augment_routes(state.clone()).recover(
            crate::core::api_server::handle_rejection,
        );

        let response = warp::test::request()
            .method("GET")
            .path("/v1/models")
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body["error"]["type"], "no_augment_accounts");
        assert_eq!(
            body["error"]["message"],
            "No available Augment accounts"
        );
    }
}
