//! Augment API 代理路由
//!
//! 将 /augment/v1/* 请求转发到 CLIProxyAPI sidecar，实现 Codex CLI → Augment 的透明代理。

use bytes::Bytes;
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use hyper::{Body, Response};
use std::sync::Arc;
use warp::http::{HeaderMap, Method, StatusCode};
use warp::{Filter, Rejection, Reply};

use crate::AppState;
use crate::data::storage::augment::traits::TokenStorage;

/// Augment 代理路由
pub fn augment_routes_from_state(
    state: Arc<AppState>,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let state_filter = warp::any().map(move || state.clone());

    let models_route = models_request_filter()
        .and(state_filter.clone())
        .and_then(|query, headers, body, state| {
            handle_augment_proxy("/augment/v1/models", Method::GET, query, headers, body, state)
        });

    let responses_route = responses_request_filter()
        .and(state_filter.clone())
        .and_then(|query, headers, body, state| {
            handle_augment_proxy(
                "/augment/v1/responses",
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
                "/augment/v1/chat/completions",
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
    raw_path: &'static str,
    method: Method,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
    state: Arc<AppState>,
) -> Result<Box<dyn Reply>, Rejection> {
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
        && has_available_credits(token)
        && !is_expired(token)
}

fn is_banned(token: &crate::storage::TokenData) -> bool {
    let status = token.ban_status.as_ref().and_then(|ban| ban.as_str());
    matches!(status, Some("SUSPENDED" | "INVALID_TOKEN"))
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
        .or_else(|| value.as_str().and_then(|value| value.trim().parse::<f64>().ok()))
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
        let route = models_request_filter().map(|query: Option<String>, _headers: HeaderMap, body: Bytes| {
            assert_eq!(query, None);
            assert!(body.is_empty());
            warp::reply::with_status("ok", StatusCode::OK)
        });

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
        let route = models_request_filter().map(|query: Option<String>, _headers: HeaderMap, body: Bytes| {
            assert_eq!(query.as_deref(), Some("limit=20"));
            assert!(body.is_empty());
            warp::reply::with_status("ok", StatusCode::OK)
        });

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
        let route = responses_request_filter().map(|_query: Option<String>, _headers: HeaderMap, _body: Bytes| {
            warp::reply::with_status("ok", StatusCode::OK)
        });

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
        let route = chat_completions_request_filter().map(|_query: Option<String>, _headers: HeaderMap, _body: Bytes| {
            warp::reply::with_status("ok", StatusCode::OK)
        });

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
            .map(|_query: Option<String>, _headers: HeaderMap, _body: Bytes| {
                warp::reply::with_status("models", StatusCode::OK)
            })
            .or(responses_request_filter().map(|_query: Option<String>, _headers: HeaderMap, _body: Bytes| {
                warp::reply::with_status("responses", StatusCode::OK)
            }))
            .unify()
            .or(chat_completions_request_filter().map(|_query: Option<String>, _headers: HeaderMap, _body: Bytes| {
                warp::reply::with_status("chat", StatusCode::OK)
            }))
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
}
