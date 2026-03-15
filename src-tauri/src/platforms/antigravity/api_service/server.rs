use crate::AppState;
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use hyper::{Body, Response};
use std::sync::Arc;
use warp::http::{HeaderMap, Method, StatusCode};
#[cfg(test)]
use warp::Filter;
use warp::{Rejection, Reply};

#[derive(Clone)]
struct AntigravityProxyState {
    antigravity_storage_manager: Arc<std::sync::Mutex<Option<Arc<crate::storage::AntigravityDualStorage>>>>,
    antigravity_sidecar: Arc<tokio::sync::Mutex<Option<crate::platforms::antigravity::sidecar::AntigravitySidecar>>>,
}

impl AntigravityProxyState {
    fn from_app_state(state: &Arc<AppState>) -> Self {
        Self {
            antigravity_storage_manager: state.antigravity_storage_manager.clone(),
            antigravity_sidecar: state.antigravity_sidecar.clone(),
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
    _gateway_profile: Option<crate::core::gateway_access::GatewayAccessProfile>,
    state: Arc<AppState>,
) -> Result<Box<dyn Reply>, Rejection> {
    handle_antigravity_proxy(
        raw_path,
        method,
        query,
        headers,
        body,
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
    state: AntigravityProxyState,
) -> Result<Box<dyn Reply>, Rejection> {
    let accounts = get_available_accounts(&state)
        .await
        .map_err(|e| warp::reject::custom(AntigravityProxyRejection::NoAccounts(e)))?;

    if accounts.is_empty() {
        return Err(warp::reject::custom(
            AntigravityProxyRejection::NoAccounts("No available Antigravity accounts".into()),
        ));
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
        let response =
            build_streaming_response(upstream_status, &upstream_headers, upstream_response)
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

    let response = builder
        .body(Body::from(body_bytes))
        .map_err(|e| warp::reject::custom(AntigravityProxyRejection::UpstreamError(e.to_string())))?;

    Ok(Box::new(response) as Box<dyn Reply>)
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

async fn get_available_accounts(state: &AntigravityProxyState) -> Result<Vec<crate::platforms::antigravity::models::Account>, String> {
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
        };
        let route =
            unified_antigravity_routes(state.clone()).recover(crate::core::api_server::handle_rejection);

        let response = warp::test::request()
            .method("GET")
            .path("/v1/models")
            .reply(&route)
            .await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body["error"]["type"], "no_antigravity_accounts");
        assert_eq!(body["error"]["message"], "No available Antigravity accounts");
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
