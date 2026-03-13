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

use crate::data::storage::augment::traits::TokenStorage;
use crate::AppState;

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

fn optional_raw_query() -> impl Filter<Extract = (Option<String>,), Error = std::convert::Infallible> + Clone
{
    warp::query::raw()
        .map(Some)
        .or(warp::any().map(|| None))
        .unify()
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
    let inner_path = raw_path
        .strip_prefix("/augment")
        .unwrap_or(raw_path)
        .to_string();

    println!(
        "[AugmentProxy] {} {} → sidecar {}",
        method, raw_path, inner_path
    );

    // 获取可用的 Augment 账号
    let tokens = get_available_tokens(&state).await.map_err(|e| {
        warp::reject::custom(AugmentProxyRejection::NoAccounts(e))
    })?;

    if tokens.is_empty() {
        return Err(warp::reject::custom(
            AugmentProxyRejection::NoAccounts("No available Augment accounts".into()),
        ));
    }

    // 确保 sidecar 正在运行
    let (base_url, api_key) = {
        let sidecar_opt = {
            let guard = state.augment_sidecar.lock().unwrap();
            guard.is_some()
        };
        if !sidecar_opt {
            return Err(warp::reject::custom(
                AugmentProxyRejection::SidecarNotReady("Sidecar not initialized (cliproxy-server binary not found)".into()),
            ));
        }

        // Sync accounts and ensure running - need to drop lock before await
        {
            let mut guard = state.augment_sidecar.lock().unwrap();
            let sidecar = guard.as_mut().unwrap();
            sidecar.sync_accounts(&tokens).map_err(|e| {
                warp::reject::custom(AugmentProxyRejection::SidecarNotReady(e))
            })?;
        }

        // Now do the async ensure_running outside the lock
        // We use a tokio Mutex pattern: extract what we need, do async work, then update
        let needs_start = {
            let guard = state.augment_sidecar.lock().unwrap();
            let sidecar = guard.as_ref().unwrap();
            !sidecar.is_running()
        };

        if needs_start {
            // Start sidecar - we need mutable access but can't hold std::sync::Mutex across await
            // Use a tokio::task::spawn_blocking approach or restructure
            let state_clone = state.clone();
            let tokens_clone = tokens.clone();
            let start_result = tokio::task::spawn_blocking(move || {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    let mut guard = state_clone.augment_sidecar.lock().unwrap();
                    let sidecar = guard.as_mut().unwrap();
                    sidecar.start(&tokens_clone).await
                })
            })
            .await
            .map_err(|e| {
                warp::reject::custom(AugmentProxyRejection::SidecarNotReady(e.to_string()))
            })?;

            if let Err(e) = start_result {
                return Err(warp::reject::custom(
                    AugmentProxyRejection::SidecarNotReady(e),
                ));
            }
        }

        let guard = state.augment_sidecar.lock().unwrap();
        let sidecar = guard.as_ref().unwrap();
        (sidecar.base_url(), sidecar.api_key().to_string())
    };

    // 构建转发 URL
    let mut upstream_url = format!("{}{}", base_url, inner_path);
    if let Some(ref q) = query {
        if !q.is_empty() {
            upstream_url.push('?');
            upstream_url.push_str(q);
        }
    }

    // 转发请求到 sidecar
    let client = reqwest::Client::new();
    let mut req_builder = client.request(
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::POST),
        &upstream_url,
    );

    // 复制请求头，替换 Authorization
    for (name, value) in headers.iter() {
        let name_str = name.as_str().to_lowercase();
        if name_str == "host" || name_str == "authorization" || name_str == "content-length" {
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

    let upstream_status =
        StatusCode::from_u16(upstream_response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let upstream_headers = upstream_response.headers().clone();

    // 判断是否是 SSE 流式响应
    let is_stream = upstream_headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map_or(false, |ct| ct.contains("text/event-stream"));

    if is_stream {
        let response =
            build_streaming_response(upstream_status, &upstream_headers, upstream_response)
                .map_err(|e| {
                    warp::reject::custom(AugmentProxyRejection::UpstreamError(e))
                })?;
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
        .map_err(|e| {
            warp::reject::custom(AugmentProxyRejection::UpstreamError(e.to_string()))
        })?;

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

async fn get_available_tokens(
    state: &AppState,
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
    Ok(tokens
        .into_iter()
        .filter(|t| {
            !t.access_token.is_empty()
                && !t.tenant_url.is_empty()
                && !is_banned(t)
        })
        .collect())
}

fn is_banned(token: &crate::storage::TokenData) -> bool {
    if let Some(ref ban) = token.ban_status {
        if let Some(s) = ban.as_str() {
            return s == "SUSPENDED" || s == "INVALID_TOKEN";
        }
    }
    false
}

// ==================== 错误类型 ====================

#[derive(Debug)]
pub enum AugmentProxyRejection {
    NoAccounts(String),
    SidecarNotReady(String),
    UpstreamError(String),
}

impl warp::reject::Reject for AugmentProxyRejection {}
