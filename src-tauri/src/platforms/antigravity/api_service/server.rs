use crate::AppState;
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use hyper::{Body, Response};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
#[cfg(test)]
use warp::Filter;
use warp::http::{HeaderMap, Method, StatusCode};
use warp::{Rejection, Reply};

#[derive(Clone)]
struct AntigravityProxyState {
    antigravity_storage_manager:
        Arc<std::sync::Mutex<Option<Arc<crate::storage::AntigravityDualStorage>>>>,
    antigravity_sidecar:
        Arc<tokio::sync::Mutex<Option<crate::platforms::antigravity::sidecar::AntigravitySidecar>>>,
    log_storage_dir: Option<PathBuf>,
}

impl AntigravityProxyState {
    fn from_app_state(state: &Arc<AppState>) -> Self {
        Self {
            antigravity_storage_manager: state.antigravity_storage_manager.clone(),
            antigravity_sidecar: state.antigravity_sidecar.clone(),
            log_storage_dir: state.app_handle.path().app_data_dir().ok(),
        }
    }
}

#[cfg(test)]
fn optional_raw_query()
-> impl Filter<Extract = (Option<String>,), Error = std::convert::Infallible> + Clone {
    warp::query::raw()
        .map(Some)
        .or(warp::any().map(|| None))
        .unify()
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
    unified_post_request_filter(warp::path!("v1" / "responses"))
}

#[cfg(test)]
fn unified_chat_completions_request_filter()
-> impl Filter<Extract = (Option<String>, HeaderMap, Bytes), Error = Rejection> + Clone {
    unified_post_request_filter(warp::path!("v1" / "chat" / "completions"))
}

#[cfg(test)]
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

pub(crate) async fn handle_unified_gateway_request(
    raw_path: String,
    method: Method,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
    gateway_profile: Option<crate::core::gateway_access::GatewayAccessProfile>,
    state: Arc<AppState>,
) -> Result<Box<dyn Reply>, Rejection> {
    handle_antigravity_proxy(
        raw_path,
        method,
        query,
        headers,
        body,
        gateway_profile,
        AntigravityProxyState::from_app_state(&state),
    )
    .await
}

#[cfg(test)]
fn unified_antigravity_routes(
    state: AntigravityProxyState,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let state_filter = warp::any().map(move || state.clone());

    let models_route = unified_models_request_filter()
        .and(state_filter.clone())
        .and_then(|query, headers, body, state| {
            handle_antigravity_proxy(
                "/v1/models".to_string(),
                Method::GET,
                query,
                headers,
                body,
                None,
                state,
            )
        });

    let responses_route = unified_responses_request_filter()
        .and(state_filter.clone())
        .and_then(|query, headers, body, state| {
            handle_antigravity_proxy(
                "/v1/responses".to_string(),
                Method::POST,
                query,
                headers,
                body,
                None,
                state,
            )
        });

    let chat_completions_route = unified_chat_completions_request_filter()
        .and(state_filter)
        .and_then(|query, headers, body, state| {
            handle_antigravity_proxy(
                "/v1/chat/completions".to_string(),
                Method::POST,
                query,
                headers,
                body,
                None,
                state,
            )
        });

    models_route.or(responses_route).or(chat_completions_route)
}

async fn handle_antigravity_proxy(
    raw_path: String,
    method: Method,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
    gateway_profile: Option<crate::core::gateway_access::GatewayAccessProfile>,
    state: AntigravityProxyState,
) -> Result<Box<dyn Reply>, Rejection> {
    let accounts = get_available_accounts(&state)
        .await
        .map_err(|e| warp::reject::custom(AntigravityProxyRejection::NoAccounts(e)))?;

    if accounts.is_empty() {
        return Err(warp::reject::custom(AntigravityProxyRejection::NoAccounts(
            "No available Antigravity accounts".into(),
        )));
    }

    let (base_url, api_key) = {
        let mut guard = state.antigravity_sidecar.lock().await;
        let sidecar = guard.as_mut().ok_or_else(|| {
            warp::reject::custom(AntigravityProxyRejection::SidecarNotReady(
                "Antigravity sidecar not initialized (cliproxy-server binary not found)".into(),
            ))
        })?;
        sidecar
            .ensure_running(&accounts)
            .await
            .map_err(|e| warp::reject::custom(AntigravityProxyRejection::SidecarNotReady(e)))?;
        (sidecar.base_url(), sidecar.api_key().to_string())
    };

    let upstream_url = build_upstream_url(&base_url, &raw_path, query.as_deref());
    let client = reqwest::Client::new();
    let request_body = body.clone();
    let request_format = infer_request_format(&raw_path);
    let request_model = extract_model_from_json_bytes(&request_body).unwrap_or_default();
    let mut req_builder = client.request(
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::POST),
        &upstream_url,
    );

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
        warp::reject::custom(AntigravityProxyRejection::UpstreamError(format!(
            "Failed to forward to sidecar: {}",
            e
        )))
    })?;

    let upstream_status = StatusCode::from_u16(upstream_response.status().as_u16())
        .unwrap_or(StatusCode::BAD_GATEWAY);
    let upstream_headers = upstream_response.headers().clone();
    let is_stream = upstream_headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.contains("text/event-stream"));

    if is_stream {
        let response = build_streaming_response(
            upstream_status,
            &upstream_headers,
            upstream_response,
            request_format,
            request_model,
            gateway_profile.clone(),
            state.log_storage_dir.clone(),
        )
        .map_err(|e| warp::reject::custom(AntigravityProxyRejection::UpstreamError(e)))?;
        return Ok(Box::new(response) as Box<dyn Reply>);
    }

    let body_bytes = upstream_response.bytes().await.map_err(|e| {
        warp::reject::custom(AntigravityProxyRejection::UpstreamError(format!(
            "Failed to read sidecar response: {}",
            e
        )))
    })?;

    let mut builder = Response::builder().status(upstream_status);
    for (name, value) in upstream_headers.iter() {
        let normalized = name.as_str().to_ascii_lowercase();
        if normalized == "transfer-encoding" || normalized == "connection" {
            continue;
        }
        builder = builder.header(name, value);
    }

    let model = extract_model_from_json_bytes(&body_bytes).unwrap_or(request_model);
    let usage = extract_usage_from_json_bytes(&body_bytes);
    let error_message = if upstream_status.is_success() {
        None
    } else {
        extract_error_message(&body_bytes)
    };
    let log = build_request_log(
        model,
        request_format,
        if upstream_status.is_success() {
            "success"
        } else {
            "error"
        },
        usage,
        error_message,
        gateway_profile.as_ref(),
    );
    persist_request_log(state.log_storage_dir.clone(), log).await;

    let response = builder.body(Body::from(body_bytes)).map_err(|e| {
        warp::reject::custom(AntigravityProxyRejection::UpstreamError(e.to_string()))
    })?;

    Ok(Box::new(response) as Box<dyn Reply>)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct UsageStats {
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
}

#[derive(Default)]
struct SseUsageExtractor {
    pending: String,
    usage: UsageStats,
}

impl SseUsageExtractor {
    fn ingest_chunk(&mut self, chunk: &Bytes) {
        let text = String::from_utf8_lossy(chunk);
        self.pending.push_str(&text);

        while let Some(idx) = self.pending.find("\n\n") {
            let event = self.pending[..idx].to_string();
            self.pending.drain(..idx + 2);
            self.parse_event_block(&event);
        }
    }

    fn finish(&mut self) {
        if self.pending.trim().is_empty() {
            return;
        }

        let event = std::mem::take(&mut self.pending);
        self.parse_event_block(&event);
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
        let is_completed = value
            .get("type")
            .and_then(|v| v.as_str())
            .is_some_and(|kind| kind.eq_ignore_ascii_case("response.completed"));
        if !is_completed {
            return;
        }

        if let Some(usage) = value
            .pointer("/response/usage")
            .or_else(|| value.get("usage"))
        {
            self.usage = usage_stats_from_value(usage);
        }
    }
}

fn extract_usage_from_json_bytes(body: &Bytes) -> UsageStats {
    let Ok(root) = serde_json::from_slice::<Value>(body) else {
        return UsageStats::default();
    };

    let Some(usage) = root
        .get("usage")
        .or_else(|| root.get("response").and_then(|value| value.get("usage")))
    else {
        return UsageStats::default();
    };

    usage_stats_from_value(usage)
}

fn usage_stats_from_value(usage: &Value) -> UsageStats {
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

fn to_i64(value: Option<&Value>) -> i64 {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()))
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
    gateway_profile: Option<&crate::core::gateway_access::GatewayAccessProfile>,
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

fn build_request_log(
    model: String,
    format: &str,
    status: &str,
    usage: UsageStats,
    error_message: Option<String>,
    gateway_profile: Option<&crate::core::gateway_access::GatewayAccessProfile>,
) -> crate::platforms::antigravity::api_service::models::RequestLog {
    let metadata = build_gateway_profile_log_metadata(gateway_profile);

    crate::platforms::antigravity::api_service::models::RequestLog {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp(),
        account_id: "antigravity-sidecar".to_string(),
        account_email: String::new(),
        model,
        format: format.to_string(),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        status: status.to_string(),
        error_message,
        request_duration_ms: None,
        gateway_profile_id: metadata.gateway_profile_id,
        gateway_profile_name: metadata.gateway_profile_name,
        member_code: metadata.member_code,
        role_title: metadata.role_title,
        display_label: metadata.display_label,
        api_key_suffix: metadata.api_key_suffix,
        color: metadata.color,
    }
}

fn extract_model_from_json_bytes(body: &Bytes) -> Option<String> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("model")
                .and_then(|model| model.as_str())
                .map(str::to_string)
        })
}

fn extract_error_message(body: &Bytes) -> Option<String> {
    let Ok(root) = serde_json::from_slice::<Value>(body) else {
        return None;
    };

    root.pointer("/error/message")
        .or_else(|| root.get("message"))
        .or_else(|| root.get("detail"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn infer_request_format(path: &str) -> &'static str {
    if path.contains("/chat/completions") {
        "openai-chat-completions"
    } else if path.contains("/responses") {
        "openai-responses"
    } else {
        "openai-models"
    }
}

#[derive(Debug)]
pub enum AntigravityProxyRejection {
    NoAccounts(String),
    SidecarNotReady(String),
    UpstreamError(String),
}

impl warp::reject::Reject for AntigravityProxyRejection {}

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

fn build_streaming_response(
    status: StatusCode,
    headers: &HeaderMap,
    response: reqwest::Response,
    request_format: &'static str,
    request_model: String,
    gateway_profile: Option<crate::core::gateway_access::GatewayAccessProfile>,
    log_storage_dir: Option<PathBuf>,
) -> Result<Response<Body>, String> {
    let mut builder = Response::builder().status(status);
    for (name, value) in headers.iter() {
        let normalized = name.as_str().to_ascii_lowercase();
        if normalized == "transfer-encoding" || normalized == "connection" {
            continue;
        }
        builder = builder.header(name, value);
    }

    let mut upstream_stream = response.bytes_stream();
    let (mut tx, rx) = futures::channel::mpsc::channel::<Result<Bytes, std::io::Error>>(16);

    tokio::spawn(async move {
        let mut usage_extractor = SseUsageExtractor::default();
        while let Some(chunk) = upstream_stream.next().await {
            match chunk {
                Ok(bytes) => {
                    usage_extractor.ingest_chunk(&bytes);
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

        usage_extractor.finish();

        let log = build_request_log(
            request_model,
            request_format,
            if status.is_success() {
                "success"
            } else {
                "error"
            },
            usage_extractor.usage,
            None,
            gateway_profile.as_ref(),
        );
        persist_request_log(log_storage_dir, log).await;
    });

    let body = Body::wrap_stream(rx);
    builder.body(body).map_err(|e| e.to_string())
}

async fn persist_request_log(
    log_storage_dir: Option<PathBuf>,
    log: crate::platforms::antigravity::api_service::models::RequestLog,
) {
    let Some(data_dir) = log_storage_dir else {
        return;
    };

    let Ok(storage) =
        crate::platforms::antigravity::api_service::logger::AntigravityLogStorage::new(data_dir)
    else {
        return;
    };

    storage.add_log(log).await;
}

async fn get_available_accounts(
    state: &AntigravityProxyState,
) -> Result<Vec<crate::platforms::antigravity::models::Account>, String> {
    use crate::data::storage::common::traits::AccountStorage;

    let storage = {
        let guard = state.antigravity_storage_manager.lock().unwrap();
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "Antigravity storage not initialized".to_string())?
    };

    let accounts = storage
        .load_accounts()
        .await
        .map_err(|e| format!("Failed to load Antigravity accounts: {}", e))?;

    Ok(accounts.into_iter().filter(is_account_usable).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gateway_access::{GatewayAccessProfile, GatewayTarget};
    use crate::data::storage::common::traits::AccountStorage;
    use crate::platforms::antigravity::models::{Account, QuotaData, TokenData};
    use serde_json::Value;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use tempfile::tempdir;
    use warp::http::StatusCode;

    fn sample_account(email: &str) -> Account {
        let mut account = Account::new(
            "account-1".into(),
            email.into(),
            TokenData::new(
                "access-token-1".into(),
                "refresh-token-1".into(),
                3600,
                Some(email.into()),
                Some("project-123".into()),
                Some("session-1".into()),
            ),
        );
        account.quota = Some(QuotaData::new());
        account
    }

    #[cfg(unix)]
    fn write_fake_proxy_sidecar_binary(dir: &Path) -> PathBuf {
        let binary_path = dir.join("fake-antigravity-proxy-sidecar.sh");
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
            self.wfile.write(b'{"data":[{"id":"antigravity-test"}]}')
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
    async fn build_proxy_test_state(temp_dir: &Path) -> AntigravityProxyState {
        let local_storage = Arc::new(crate::storage::AntigravityLocalStorage::new_with_path(
            temp_dir.join("proxy-test-accounts.json"),
        ));
        let storage_manager = Arc::new(crate::storage::AntigravityDualStorage::new(
            local_storage,
            None,
            false,
        ));
        storage_manager
            .save_account(&sample_account("jdd@lingkong.xyz"))
            .await
            .unwrap();

        AntigravityProxyState {
            antigravity_storage_manager: Arc::new(Mutex::new(Some(storage_manager))),
            antigravity_sidecar: Arc::new(tokio::sync::Mutex::new(Some(
                crate::platforms::antigravity::sidecar::AntigravitySidecar::new(
                    temp_dir,
                    write_fake_proxy_sidecar_binary(temp_dir),
                ),
            ))),
            log_storage_dir: Some(temp_dir.to_path_buf()),
        }
    }

    #[test]
    fn account_with_empty_access_token_is_not_usable() {
        let mut account = sample_account("jdd@lingkong.xyz");
        account.token.access_token = "  ".into();

        assert!(!is_account_usable(&account));
    }

    #[test]
    fn disabled_account_is_not_usable() {
        let mut account = sample_account("jdd@lingkong.xyz");
        account.disabled = true;

        assert!(!is_account_usable(&account));
    }

    #[test]
    fn deleted_account_is_not_usable() {
        let mut account = sample_account("jdd@lingkong.xyz");
        account.deleted = true;

        assert!(!is_account_usable(&account));
    }

    #[test]
    fn forbidden_quota_account_is_not_usable() {
        let mut account = sample_account("jdd@lingkong.xyz");
        account.quota = Some(QuotaData {
            is_forbidden: true,
            ..QuotaData::new()
        });

        assert!(!is_account_usable(&account));
    }

    #[test]
    fn account_with_valid_token_is_usable() {
        assert!(is_account_usable(&sample_account("jdd@lingkong.xyz")));
    }

    #[test]
    fn antigravity_usage_defaults_to_zero_when_usage_missing() {
        let usage = extract_usage_from_json_bytes(&Bytes::from_static(br#"{"id":"resp_1"}"#));

        assert_eq!(usage.total_tokens, 0);
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
    }

    #[test]
    fn antigravity_streaming_usage_only_finalizes_from_completed_event() {
        let mut extractor = SseUsageExtractor::default();

        extractor.ingest_chunk(&Bytes::from_static(
            br#"data: {"type":"response.output_text.delta","usage":{"input_tokens":99,"output_tokens":1,"total_tokens":100}}

"#,
        ));
        extractor.ingest_chunk(&Bytes::from_static(
            br#"data: {"type":"response.completed","response":{"usage":{"input_tokens":12,"output_tokens":34,"total_tokens":46}}}

"#,
        ));
        extractor.finish();

        assert_eq!(
            extractor.usage,
            UsageStats {
                input_tokens: 12,
                output_tokens: 34,
                total_tokens: 46,
            }
        );
    }

    #[test]
    fn antigravity_request_log_captures_member_identity_fields() {
        let profile = GatewayAccessProfile {
            id: "ant-jdd".to_string(),
            name: "姜大大".to_string(),
            target: GatewayTarget::Antigravity,
            api_key: "sk-ant-jdd-a4f29c7e".to_string(),
            enabled: true,
            member_code: Some("jdd".to_string()),
            role_title: Some("产品与方法论".to_string()),
            persona_summary: Some("高频输出".to_string()),
            color: Some("#4c6ef5".to_string()),
            notes: Some("高频使用成员".to_string()),
        };

        let log = build_request_log(
            "claude-sonnet-4-5".to_string(),
            "openai-responses",
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

    #[cfg(unix)]
    #[tokio::test]
    async fn antigravity_unified_models_route_returns_sidecar_models() {
        let temp_dir = tempdir().unwrap();
        let state = build_proxy_test_state(temp_dir.path()).await;
        let route = unified_antigravity_routes(state.clone());

        let response = warp::test::request()
            .method("GET")
            .path("/v1/models")
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body["data"][0]["id"], "antigravity-test");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn antigravity_unified_chat_route_replaces_authorization() {
        let temp_dir = tempdir().unwrap();
        let state = build_proxy_test_state(temp_dir.path()).await;
        let route = unified_antigravity_routes(state.clone());

        let response = warp::test::request()
            .method("POST")
            .path("/v1/chat/completions?stream=false")
            .header("authorization", "Bearer user-token")
            .header("content-type", "application/json")
            .body(r#"{"model":"claude-sonnet-4","messages":[{"role":"user","content":"hi"}]}"#)
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::OK);

        let payload: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(payload["path"], "/v1/chat/completions?stream=false");

        let authorization = payload["authorization"].as_str().unwrap();
        assert!(authorization.starts_with("Bearer sk-atm-antigravity-internal-"));
        assert_ne!(authorization, "Bearer user-token");
        assert!(
            payload["body"]
                .as_str()
                .unwrap()
                .contains(r#""model":"claude-sonnet-4""#)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn antigravity_unified_responses_route_preserves_sse_streams() {
        let temp_dir = tempdir().unwrap();
        let state = build_proxy_test_state(temp_dir.path()).await;
        let route = unified_antigravity_routes(state.clone());

        let response = warp::test::request()
            .method("POST")
            .path("/v1/responses")
            .header("content-type", "application/json")
            .body(r#"{"model":"claude-sonnet-4","input":"hi","stream":true}"#)
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "text/event-stream");
        assert_eq!(response.body(), "data: {\"ok\":true}\n\n");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn antigravity_unified_models_route_returns_503_without_available_accounts() {
        let temp_dir = tempdir().unwrap();
        let local_storage = Arc::new(crate::storage::AntigravityLocalStorage::new_with_path(
            temp_dir.path().join("proxy-test-empty-accounts.json"),
        ));
        let storage_manager = Arc::new(crate::storage::AntigravityDualStorage::new(
            local_storage,
            None,
            false,
        ));
        let state = AntigravityProxyState {
            antigravity_storage_manager: Arc::new(Mutex::new(Some(storage_manager))),
            antigravity_sidecar: Arc::new(tokio::sync::Mutex::new(Some(
                crate::platforms::antigravity::sidecar::AntigravitySidecar::new(
                    temp_dir.path(),
                    write_fake_proxy_sidecar_binary(temp_dir.path()),
                ),
            ))),
            log_storage_dir: Some(temp_dir.path().to_path_buf()),
        };
        let route = unified_antigravity_routes(state.clone())
            .recover(crate::core::api_server::handle_rejection);

        let response = warp::test::request()
            .method("GET")
            .path("/v1/models")
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body["error"]["type"], "no_antigravity_accounts");
        assert_eq!(
            body["error"]["message"],
            "No available Antigravity accounts"
        );
    }
}

fn is_account_usable(account: &crate::platforms::antigravity::models::Account) -> bool {
    !account.token.access_token.trim().is_empty()
        && !account.disabled
        && !account.deleted
        && !account
            .quota
            .as_ref()
            .is_some_and(|quota| quota.is_forbidden)
}
