//! Codex 透传执行器
//!
//! 负责将本地请求透传到 ChatGPT Codex 上游，并使用账号池进行鉴权与失败切换。

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use reqwest::{Method, RequestBuilder, StatusCode};
use warp::http::HeaderMap;

use super::pool::CodexPool;
use crate::http_client::create_proxy_client;
use crate::platforms::openai::codex::models::{CodexError, CodexPoolAccount};
use crate::proxy_helper::ProxyClient;

/// 透传请求上下文
#[derive(Debug, Clone)]
pub struct ForwardRequest {
    pub method: Method,
    pub path: String,
    pub query: Option<String>,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub format: String,
    pub model: String,
}

/// 透传执行元数据（供上层记录日志）
#[derive(Debug, Clone)]
pub struct ForwardMeta {
    pub account_id: String,
    pub account_email: String,
    pub format: String,
    pub model: String,
    pub started_at: Instant,
}

/// Codex API 执行器
pub struct CodexExecutor {
    pool: Arc<CodexPool>,
    client: ProxyClient,
    upstream_origin: String,
}

impl CodexExecutor {
    pub fn new(pool: Arc<CodexPool>) -> Result<Self, String> {
        let client = create_proxy_client()?;

        Ok(Self {
            pool,
            client,
            upstream_origin: "https://chatgpt.com".to_string(),
        })
    }

    /// 透传执行：返回上游响应（包含原始状态码与头）
    pub async fn forward(
        &self,
        request: ForwardRequest,
    ) -> Result<(reqwest::Response, ForwardMeta), CodexError> {
        let active_count = self.pool.active_count().await;
        if active_count == 0 {
            return Err(CodexError::NoAvailableAccount);
        }

        let mut attempted_ids = HashSet::new();
        let mut selection_budget = active_count.saturating_mul(3).max(1);
        let mut last_transport_error: Option<reqwest::Error> = None;

        while attempted_ids.len() < active_count && selection_budget > 0 {
            selection_budget -= 1;

            let Some(account) = self.pool.next_account().await else {
                break;
            };
            if !attempted_ids.insert(account.id.clone()) {
                continue;
            }

            // 根据账号类型决定上游 URL
            let upstream_url = if account.is_api_account {
                let base = account.api_base_url.as_deref().unwrap_or("");
                let api_path = map_api_upstream_path(&request.path);
                build_upstream_url(
                    base.trim_end_matches('/'),
                    &api_path,
                    request.query.as_deref(),
                )
            } else {
                let mapped_path = map_upstream_path(&request.path)?;
                build_upstream_url(
                    &self.upstream_origin,
                    &mapped_path,
                    request.query.as_deref(),
                )
            };

            let meta = ForwardMeta {
                account_id: account.id.clone(),
                account_email: account.email.clone(),
                format: request.format.clone(),
                model: request.model.clone(),
                started_at: Instant::now(),
            };

            let response = match self.send_once(&upstream_url, &request, &account).await {
                Ok(resp) => resp,
                Err(err) => {
                    self.pool.record_failure(&account.id, None).await;
                    if is_retryable_transport_error(&err) && attempted_ids.len() < active_count {
                        last_transport_error = Some(err);
                        continue;
                    }
                    return Err(CodexError::ExecutionError(format_transport_error(&err)));
                }
            };

            let status = response.status();
            if status.is_success() {
                self.pool.record_success(&account.id).await;
                return Ok((response, meta));
            }

            self.pool
                .record_failure(&account.id, Some(status.as_u16()))
                .await;

            if should_retry_status(status) && attempted_ids.len() < active_count {
                continue;
            }

            return Ok((response, meta));
        }

        if let Some(err) = last_transport_error {
            return Err(CodexError::ExecutionError(format_transport_error(&err)));
        }

        Err(CodexError::NoAvailableAccount)
    }

    async fn send_once(
        &self,
        url: &str,
        request: &ForwardRequest,
        account: &CodexPoolAccount,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let builder = self.client.request(request.method.clone(), url);
        let builder = apply_forward_headers(builder, &request.headers, account);

        builder.body(request.body.clone()).send().await
    }
}

fn map_upstream_path(path: &str) -> Result<String, CodexError> {
    if path == "/v1" {
        return Ok("/backend-api/codex".to_string());
    }

    if let Some(tail) = path.strip_prefix("/v1/") {
        return Ok(format!("/backend-api/codex/{}", tail));
    }

    if path == "/backend-api/codex" || path.starts_with("/backend-api/codex/") {
        return Ok(path.to_string());
    }

    Err(CodexError::InvalidRequest(format!(
        "Unsupported Codex path: {}",
        path
    )))
}

/// API 账号直接透传 OpenAI 兼容路径，不做 /backend-api/codex 映射。
/// 因为 api_base_url 通常已包含 /v1（如 https://code.ppchat.vip/v1），
/// 所以需要去掉请求路径中的 /v1 前缀，避免拼出 /v1/v1/responses。
fn map_api_upstream_path(path: &str) -> String {
    if let Some(tail) = path.strip_prefix("/v1") {
        // "/v1/responses" -> "/responses", "/v1" -> ""
        if tail.is_empty() {
            String::new()
        } else {
            tail.to_string()
        }
    } else {
        path.to_string()
    }
}

fn build_upstream_url(origin: &str, path: &str, raw_query: Option<&str>) -> String {
    let mut url = format!("{}{}", origin, path);
    if let Some(query) = raw_query.map(str::trim).filter(|q| !q.is_empty()) {
        url.push('?');
        url.push_str(query);
    }
    url
}

fn apply_forward_headers(
    mut builder: RequestBuilder,
    headers: &HeaderMap,
    account: &CodexPoolAccount,
) -> RequestBuilder {
    let mut has_user_agent = false;
    let mut has_openai_beta = false;
    let mut has_originator = false;

    for (name, value) in headers.iter() {
        if should_strip_request_header(name.as_str()) {
            continue;
        }

        if name.as_str().eq_ignore_ascii_case("user-agent") {
            has_user_agent = true;
        }
        if name.as_str().eq_ignore_ascii_case("openai-beta") {
            has_openai_beta = true;
        }
        if name.as_str().eq_ignore_ascii_case("originator") {
            has_originator = true;
        }

        builder = builder.header(name, value.clone());
    }

    if account.is_api_account {
        // API 账号：只设 Authorization，不设 chatgpt-account-id / originator
        builder = builder.header("Authorization", format!("Bearer {}", account.access_token));
    } else {
        // OAuth 账号：原有逻辑
        builder = builder
            .header("Authorization", format!("Bearer {}", account.access_token))
            .header("chatgpt-account-id", account.chatgpt_account_id.clone());

        if !has_originator {
            builder = builder.header("originator", "codex_cli_rs");
        }
    }

    if !has_user_agent {
        builder = builder.header("User-Agent", "codex_cli_rs/0.98.0");
    }
    if !has_openai_beta {
        builder = builder.header("OpenAI-Beta", "responses=experimental");
    }

    builder
}

fn should_strip_request_header(header_name: &str) -> bool {
    matches!(
        header_name.to_ascii_lowercase().as_str(),
        "host"
            | "content-length"
            | "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "authorization"
            | "x-api-key"
            | "chatgpt-account-id"
    )
}

fn should_retry_status(status: StatusCode) -> bool {
    matches!(
        status.as_u16(),
        401 | 403 | 408 | 429 | 500 | 502 | 503 | 504
    )
}

fn is_retryable_transport_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect()
}

fn format_transport_error(err: &reqwest::Error) -> String {
    if err.is_timeout() || err.is_connect() {
        return format!(
            "Request failed: {}. Upstream connection timed out; check proxy settings and network reachability.",
            err
        );
    }

    format!("Request failed: {}", err)
}
