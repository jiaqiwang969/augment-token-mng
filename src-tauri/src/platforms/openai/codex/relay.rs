use std::time::Duration;

use chrono::Utc;
use tauri::Emitter;

use super::commands::{
    current_api_server_port, gateway_api_key_for_target, gateway_server_url,
    get_or_load_codex_config,
};
use super::pool::{CodexRelayHealthSnapshot, CodexRelayLayerHealth};
use crate::AppState;
use crate::core::gateway_access::GatewayTarget;

const RELAY_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const CODEX_RELAY_HEALTH_CHANGED_EVENT: &str = "codex-relay-health-changed";

#[derive(Debug, Clone, PartialEq, Eq)]
enum CodexRelayProbeFailure {
    HttpStatus(u16),
    Timeout,
    InvalidJson,
    EmptyModelList,
    Transport(String),
}

impl std::fmt::Display for CodexRelayProbeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HttpStatus(status) => write!(f, "returned HTTP {}", status),
            Self::Timeout => write!(f, "timed out"),
            Self::InvalidJson => write!(f, "returned invalid JSON"),
            Self::EmptyModelList => write!(f, "returned an empty model list"),
            Self::Transport(message) => write!(f, "request failed: {}", message),
        }
    }
}

impl CodexRelayProbeFailure {
    fn from_reqwest(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::Timeout
        } else {
            Self::Transport(err.to_string())
        }
    }
}

fn models_payload_is_ready(body: &[u8]) -> bool {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) else {
        return false;
    };

    json.get("data")
        .and_then(|data| data.as_array())
        .is_some_and(|models| !models.is_empty())
}

fn evaluate_models_endpoint_response(
    status: u16,
    body: &[u8],
) -> Result<(), CodexRelayProbeFailure> {
    if !(200..300).contains(&status) {
        return Err(CodexRelayProbeFailure::HttpStatus(status));
    }

    let json = serde_json::from_slice::<serde_json::Value>(body)
        .map_err(|_| CodexRelayProbeFailure::InvalidJson)?;
    if json.get("data").and_then(|data| data.as_array()).is_none() {
        Err(CodexRelayProbeFailure::EmptyModelList)
    } else if !models_payload_is_ready(body) {
        Err(CodexRelayProbeFailure::EmptyModelList)
    } else {
        Ok(())
    }
}

fn build_probe_layer_health(
    previous: &CodexRelayLayerHealth,
    checked_at: i64,
    result: Result<(), CodexRelayProbeFailure>,
) -> CodexRelayLayerHealth {
    let mut next = previous.clone();
    next.healthy = result.is_ok();
    next.last_checked_at = Some(checked_at);

    match result {
        Ok(()) => {
            next.last_success_at = Some(checked_at);
            next.last_error = None;
        }
        Err(err) => {
            next.last_error = Some(err.to_string());
        }
    }

    next
}

fn build_skipped_layer_health(
    previous: &CodexRelayLayerHealth,
    reason: impl Into<String>,
) -> CodexRelayLayerHealth {
    let mut next = previous.clone();
    next.healthy = false;
    next.last_checked_at = None;
    next.last_error = Some(reason.into());
    next
}

async fn run_models_probe(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<(), CodexRelayProbeFailure> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .timeout(RELAY_PROBE_TIMEOUT)
        .send()
        .await
        .map_err(CodexRelayProbeFailure::from_reqwest)?;
    let status = response.status().as_u16();
    let body = response
        .bytes()
        .await
        .map_err(CodexRelayProbeFailure::from_reqwest)?;

    evaluate_models_endpoint_response(status, body.as_ref())
}

async fn probe_relay_layer(
    client: &reqwest::Client,
    label: &str,
    base_url: &str,
    api_key: &str,
    previous: &CodexRelayLayerHealth,
    checked_at: i64,
) -> CodexRelayLayerHealth {
    let result = run_models_probe(client, base_url, api_key)
        .await
        .map_err(|err| CodexRelayProbeFailure::Transport(format!("{} {}", label, err)));

    build_probe_layer_health(previous, checked_at, result)
}

pub(crate) async fn probe_local_relay(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    previous: &CodexRelayLayerHealth,
    checked_at: i64,
) -> CodexRelayLayerHealth {
    probe_relay_layer(
        client,
        "local relay",
        base_url,
        api_key,
        previous,
        checked_at,
    )
    .await
}

pub(crate) async fn probe_public_relay(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    previous: &CodexRelayLayerHealth,
    checked_at: i64,
) -> CodexRelayLayerHealth {
    probe_relay_layer(
        client,
        "public relay",
        base_url,
        api_key,
        previous,
        checked_at,
    )
    .await
}

fn store_codex_relay_health_snapshot(
    state: &AppState,
    snapshot: CodexRelayHealthSnapshot,
) -> CodexRelayHealthSnapshot {
    let changed = {
        let mut guard = state.codex_relay_health_snapshot.lock().unwrap();
        if *guard == snapshot {
            false
        } else {
            *guard = snapshot.clone();
            true
        }
    };

    if changed {
        let _ = state
            .app_handle
            .emit(CODEX_RELAY_HEALTH_CHANGED_EVENT, snapshot.clone());
    }

    snapshot
}

pub(crate) async fn refresh_codex_relay_health_snapshot(
    app: &tauri::AppHandle,
    state: &AppState,
) -> CodexRelayHealthSnapshot {
    let previous = state.codex_relay_health_snapshot.lock().unwrap().clone();
    let checked_at = Utc::now().timestamp();

    let config = match get_or_load_codex_config(app, state) {
        Ok(config) => config,
        Err(err) => {
            let mut snapshot = previous.clone();
            snapshot.local = build_probe_layer_health(
                &previous.local,
                checked_at,
                Err(CodexRelayProbeFailure::Transport(err)),
            );
            snapshot.public =
                build_skipped_layer_health(&previous.public, "public relay probe skipped");
            snapshot.refresh_overall();
            return store_codex_relay_health_snapshot(state, snapshot);
        }
    };

    let api_key = match gateway_api_key_for_target(app, state, GatewayTarget::Codex) {
        Ok(Some(api_key)) if !api_key.trim().is_empty() => api_key,
        Ok(_) => {
            let mut snapshot = previous.clone();
            snapshot.local = build_probe_layer_health(
                &previous.local,
                checked_at,
                Err(CodexRelayProbeFailure::Transport(
                    "Codex gateway API key is not configured".to_string(),
                )),
            );
            snapshot.public =
                build_skipped_layer_health(&previous.public, "public relay probe skipped");
            snapshot.refresh_overall();
            return store_codex_relay_health_snapshot(state, snapshot);
        }
        Err(err) => {
            let mut snapshot = previous.clone();
            snapshot.local = build_probe_layer_health(
                &previous.local,
                checked_at,
                Err(CodexRelayProbeFailure::Transport(err)),
            );
            snapshot.public =
                build_skipped_layer_health(&previous.public, "public relay probe skipped");
            snapshot.refresh_overall();
            return store_codex_relay_health_snapshot(state, snapshot);
        }
    };

    let client = reqwest::Client::new();
    let local_base_url = gateway_server_url(current_api_server_port(state));

    let local = probe_local_relay(
        &client,
        &local_base_url,
        &api_key,
        &previous.local,
        checked_at,
    )
    .await;
    let public = if local.healthy {
        probe_public_relay(
            &client,
            &config.relay.public_base_url,
            &api_key,
            &previous.public,
            checked_at,
        )
        .await
    } else {
        build_skipped_layer_health(
            &previous.public,
            "public relay probe skipped until local relay recovers",
        )
    };

    let mut snapshot = previous;
    snapshot.local = local;
    snapshot.public = public;
    snapshot.refresh_overall();
    store_codex_relay_health_snapshot(state, snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_relay_probe_marks_healthy_when_2xx_contains_models() {
        let body = br#"{"data":[{"id":"gpt-5","object":"model"}],"object":"list"}"#;

        assert!(models_payload_is_ready(body));
        assert!(evaluate_models_endpoint_response(200, body).is_ok());
    }

    #[test]
    fn codex_relay_probe_marks_unhealthy_for_http_errors_timeouts_and_invalid_payloads() {
        let ready_body = br#"{"data":[{"id":"gpt-5","object":"model"}],"object":"list"}"#;
        let empty_body = br#"{"data":[],"object":"list"}"#;
        let invalid_body = br#"{"data":"#;

        assert!(evaluate_models_endpoint_response(401, ready_body).is_err());
        assert!(evaluate_models_endpoint_response(502, ready_body).is_err());
        assert!(evaluate_models_endpoint_response(200, empty_body).is_err());
        assert!(evaluate_models_endpoint_response(200, invalid_body).is_err());
        assert!(
            build_probe_layer_health(
                &CodexRelayLayerHealth::default(),
                123,
                Err(CodexRelayProbeFailure::Timeout)
            )
            .last_error
            .is_some()
        );
    }

    #[test]
    fn codex_relay_probe_public_down_is_only_reported_when_local_is_healthy() {
        let mut snapshot = CodexRelayHealthSnapshot::default();
        snapshot.local = CodexRelayLayerHealth {
            healthy: true,
            last_checked_at: Some(1),
            ..Default::default()
        };
        snapshot.public = CodexRelayLayerHealth {
            healthy: false,
            last_checked_at: Some(1),
            ..Default::default()
        };
        snapshot.refresh_overall();
        assert_eq!(snapshot.overall.state, "public_down");

        snapshot.local = CodexRelayLayerHealth {
            healthy: false,
            last_checked_at: Some(2),
            ..Default::default()
        };
        snapshot.public = CodexRelayLayerHealth {
            healthy: false,
            last_checked_at: Some(2),
            ..Default::default()
        };
        snapshot.refresh_overall();
        assert_eq!(snapshot.overall.state, "local_down");
    }
}
