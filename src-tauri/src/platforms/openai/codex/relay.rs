use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use chrono::Utc;
use tauri::Emitter;
use tokio::process::Command;
use tokio::sync::Mutex as TokioMutex;

use super::commands::{
    current_api_server_port, ensure_local_codex_relay_core_running,
    ensure_local_codex_relay_running, gateway_api_key_for_target, gateway_server_url,
    get_or_load_codex_config,
};
use super::pool::{CodexRelayConfig, CodexRelayHealthSnapshot, CodexRelayLayerHealth};
use crate::AppState;
use crate::core::gateway_access::GatewayTarget;

const RELAY_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const CODEX_RELAY_HEALTH_CHANGED_EVENT: &str = "codex-relay-health-changed";
static RELAY_REPAIR_LOCK: LazyLock<TokioMutex<()>> = LazyLock::new(|| TokioMutex::new(()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexRelayRepairAction {
    RepairLocal,
    ProbeLocal,
    RepairPublic,
    ProbePublic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodexRelayRepairTarget {
    Local,
    Public,
}

#[derive(Debug, Clone)]
struct ResolvedRelayConfig {
    public_base_url: String,
    host: Option<String>,
    remote_port: u16,
    local_port: u16,
    control_socket: PathBuf,
    auto_repair_cooldown_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodexRelayProbeFailure {
    HttpStatus(u16),
    Timeout,
    InvalidJson,
    EmptyModelList,
    Transport(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodexRelayProbeOutcome {
    pub(crate) health: CodexRelayLayerHealth,
    pub(crate) failure: Option<CodexRelayProbeFailure>,
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

pub(crate) fn build_repair_sequence(
    snapshot: &CodexRelayHealthSnapshot,
) -> Vec<CodexRelayRepairAction> {
    let mut sequence = Vec::new();
    let local_needs_repair = snapshot.local.last_checked_at.is_some() && !snapshot.local.healthy;
    let public_needs_repair = snapshot.public.last_checked_at.is_some() && !snapshot.public.healthy;

    if local_needs_repair {
        sequence.push(CodexRelayRepairAction::RepairLocal);
        sequence.push(CodexRelayRepairAction::ProbeLocal);
    }

    if public_needs_repair {
        sequence.push(CodexRelayRepairAction::RepairPublic);
        sequence.push(CodexRelayRepairAction::ProbePublic);
    }

    sequence
}

pub(crate) fn repair_attempt_allowed(
    layer: &CodexRelayLayerHealth,
    now: i64,
    _cooldown_seconds: u64,
    manual: bool,
) -> bool {
    if layer.repair_in_progress {
        return false;
    }

    if manual {
        return true;
    }

    layer.cooldown_until.is_none_or(|ts| now >= ts)
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

fn format_probe_failure(label: &str, err: &CodexRelayProbeFailure) -> String {
    format!("{} {}", label, err)
}

fn build_probe_outcome(
    label: &str,
    previous: &CodexRelayLayerHealth,
    checked_at: i64,
    result: Result<(), CodexRelayProbeFailure>,
) -> CodexRelayProbeOutcome {
    let failure = result.as_ref().err().cloned();
    let mut health = build_probe_layer_health(previous, checked_at, result);

    if let Some(err) = failure.as_ref() {
        health.last_error = Some(format_probe_failure(label, err));
    }

    CodexRelayProbeOutcome { health, failure }
}

fn build_skipped_layer_health(
    previous: &CodexRelayLayerHealth,
    reason: impl Into<String>,
) -> CodexRelayLayerHealth {
    let mut next = previous.clone();
    next.healthy = false;
    next.last_error = Some(reason.into());
    next
}

fn relay_env_file_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("ATM_RELAY_ENV_FILE") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    std::env::current_dir()
        .ok()
        .and_then(|current_dir| relay_env_file_path_from(&current_dir))
}

fn relay_env_file_path_from(start_dir: &Path) -> Option<PathBuf> {
    start_dir
        .ancestors()
        .map(|dir| dir.join(".env.relay"))
        .find(|candidate| candidate.is_file())
}

fn parse_relay_env_file() -> std::collections::HashMap<String, String> {
    let Some(path) = relay_env_file_path() else {
        return std::collections::HashMap::new();
    };
    let Ok(content) = std::fs::read_to_string(path) else {
        return std::collections::HashMap::new();
    };

    let mut values = std::collections::HashMap::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        let trimmed_key = key.trim();
        let trimmed_value = value.trim().trim_matches('"').trim_matches('\'');
        if !trimmed_key.is_empty() && !trimmed_value.is_empty() {
            values.insert(trimmed_key.to_string(), trimmed_value.to_string());
        }
    }

    values
}

fn relay_setting_string(
    env_file: &std::collections::HashMap<String, String>,
    key: &str,
) -> Option<String> {
    std::env::var(key)
        .ok()
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .or_else(|| env_file.get(key).cloned())
}

fn relay_setting_u16(
    env_file: &std::collections::HashMap<String, String>,
    key: &str,
) -> Option<u16> {
    relay_setting_string(env_file, key).and_then(|value| value.parse::<u16>().ok())
}

fn default_control_socket(remote_port: u16) -> PathBuf {
    let home = std::env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"));
    home.join(".ssh")
        .join(format!("atm-relay-{}.sock", remote_port))
}

fn resolve_relay_config(relay: &CodexRelayConfig) -> ResolvedRelayConfig {
    let env_file = parse_relay_env_file();
    let default_public_base_url = "https://lingkong.xyz/v1";

    let public_base_url = if relay.public_base_url.trim().is_empty()
        || (relay.public_base_url == default_public_base_url
            && relay_setting_string(&env_file, "ATM_RELAY_PUBLIC_BASE_URL").is_some())
    {
        relay_setting_string(&env_file, "ATM_RELAY_PUBLIC_BASE_URL")
            .unwrap_or_else(|| default_public_base_url.to_string())
    } else {
        relay.public_base_url.trim().to_string()
    };

    let host = relay
        .host
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| relay_setting_string(&env_file, "ATM_RELAY_HOST"));

    let remote_port = if relay.remote_port == 19090 {
        relay_setting_u16(&env_file, "ATM_RELAY_REMOTE_PORT").unwrap_or(relay.remote_port)
    } else {
        relay.remote_port
    };

    let local_port = if relay.local_port == 8766 {
        relay_setting_u16(&env_file, "ATM_RELAY_LOCAL_PORT").unwrap_or(relay.local_port)
    } else {
        relay.local_port
    };

    let control_socket = relay
        .control_socket
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| relay_setting_string(&env_file, "ATM_RELAY_CONTROL_SOCKET").map(PathBuf::from))
        .unwrap_or_else(|| default_control_socket(remote_port));

    ResolvedRelayConfig {
        public_base_url,
        host,
        remote_port,
        local_port,
        control_socket,
        auto_repair_cooldown_seconds: relay.auto_repair_cooldown_seconds,
    }
}

pub(crate) fn resolved_public_relay_base_url(relay: &CodexRelayConfig) -> String {
    resolve_relay_config(relay).public_base_url
}

fn layer_mut<'a>(
    snapshot: &'a mut CodexRelayHealthSnapshot,
    target: CodexRelayRepairTarget,
) -> &'a mut CodexRelayLayerHealth {
    match target {
        CodexRelayRepairTarget::Local => &mut snapshot.local,
        CodexRelayRepairTarget::Public => &mut snapshot.public,
    }
}

fn store_repair_progress(
    state: &AppState,
    target: CodexRelayRepairTarget,
    now: i64,
    cooldown_seconds: u64,
    in_progress: bool,
    result: Option<String>,
) -> CodexRelayHealthSnapshot {
    let mut snapshot = state.codex_relay_health_snapshot.lock().unwrap().clone();
    let layer = layer_mut(&mut snapshot, target);

    if in_progress {
        layer.repair_in_progress = true;
        layer.last_repair_attempt_at = Some(now);
        layer.last_repair_result = None;
        layer.cooldown_until = Some(now + cooldown_seconds as i64);
    } else {
        layer.repair_in_progress = false;
        layer.last_repair_result = result;
        layer.cooldown_until = Some(now + cooldown_seconds as i64);
    }

    snapshot.refresh_overall();
    store_codex_relay_health_snapshot(state, snapshot)
}

async fn ssh_output(args: &[String]) -> Result<std::process::Output, String> {
    let mut command = Command::new("ssh");
    command.args(args);
    command.output().await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "ssh is not installed or not found in PATH".to_string()
        } else {
            format!("failed to execute ssh: {}", error)
        }
    })
}

async fn check_public_relay_control_socket(
    host: &str,
    control_socket: &PathBuf,
) -> Result<bool, String> {
    let args = vec![
        "-S".to_string(),
        control_socket.display().to_string(),
        "-O".to_string(),
        "check".to_string(),
        host.to_string(),
    ];
    let output = ssh_output(&args).await?;
    Ok(output.status.success())
}

async fn repair_public_relay(config: &ResolvedRelayConfig) -> Result<String, String> {
    let host = config
        .host
        .as_ref()
        .ok_or_else(|| "ATM relay host is not configured".to_string())?;

    if let Some(parent) = config.control_socket.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!("failed to prepare ssh control socket directory: {}", error)
        })?;
    }

    if check_public_relay_control_socket(host, &config.control_socket).await? {
        return Ok(format!(
            "relay already running: {} -> 127.0.0.1:{} -> 127.0.0.1:{}",
            host, config.remote_port, config.local_port
        ));
    }

    let _ = std::fs::remove_file(&config.control_socket);

    let args = vec![
        "-fN".to_string(),
        "-M".to_string(),
        "-S".to_string(),
        config.control_socket.display().to_string(),
        "-o".to_string(),
        "ExitOnForwardFailure=yes".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=30".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=3".to_string(),
        "-R".to_string(),
        format!(
            "127.0.0.1:{}:127.0.0.1:{}",
            config.remote_port, config.local_port
        ),
        host.to_string(),
    ];
    let output = ssh_output(&args).await?;
    if output.status.success() {
        Ok(format!(
            "relay started: {} -> 127.0.0.1:{} -> 127.0.0.1:{}",
            host, config.remote_port, config.local_port
        ))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(format!(
                "ssh relay start failed with status {}",
                output.status
            ))
        } else {
            Err(format!("ssh relay start failed: {}", stderr))
        }
    }
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
) -> CodexRelayProbeOutcome {
    let result = run_models_probe(client, base_url, api_key).await;

    build_probe_outcome(label, previous, checked_at, result)
}

pub(crate) async fn probe_local_relay(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    previous: &CodexRelayLayerHealth,
    checked_at: i64,
) -> CodexRelayProbeOutcome {
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
) -> CodexRelayProbeOutcome {
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
    let relay_config = resolve_relay_config(&config.relay);

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

    let local_probe = probe_local_relay(
        &client,
        &local_base_url,
        &api_key,
        &previous.local,
        checked_at,
    )
    .await;
    let public = if local_probe.failure.is_none() {
        probe_public_relay(
            &client,
            &relay_config.public_base_url,
            &api_key,
            &previous.public,
            checked_at,
        )
        .await
        .health
    } else {
        build_skipped_layer_health(
            &previous.public,
            "public relay probe skipped until local relay recovers",
        )
    };

    let mut snapshot = previous;
    snapshot.local = local_probe.health;
    snapshot.public = public;
    snapshot.refresh_overall();
    store_codex_relay_health_snapshot(state, snapshot)
}

pub(crate) async fn repair_codex_relay_health_auto(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<CodexRelayHealthSnapshot, String> {
    let _repair_guard = RELAY_REPAIR_LOCK
        .try_lock()
        .map_err(|_| "Codex relay repair is already in progress".to_string())?;

    let mut snapshot = refresh_codex_relay_health_snapshot(app, state).await;
    let sequence = build_repair_sequence(&snapshot);
    if sequence.is_empty() {
        return Ok(snapshot);
    }

    let config = get_or_load_codex_config(app, state)?;
    let relay_config = resolve_relay_config(&config.relay);
    let now = Utc::now().timestamp();

    if sequence.contains(&CodexRelayRepairAction::RepairLocal)
        && repair_attempt_allowed(
            &snapshot.local,
            now,
            relay_config.auto_repair_cooldown_seconds,
            false,
        )
    {
        store_repair_progress(
            state,
            CodexRelayRepairTarget::Local,
            now,
            relay_config.auto_repair_cooldown_seconds,
            true,
            None,
        );
        let local_result = ensure_local_codex_relay_core_running(app, state)
            .await
            .map(|_| ());
        store_repair_progress(
            state,
            CodexRelayRepairTarget::Local,
            now,
            relay_config.auto_repair_cooldown_seconds,
            false,
            Some(match local_result {
                Ok(()) => "Local relay restart requested".to_string(),
                Err(error) => error,
            }),
        );
        snapshot = refresh_codex_relay_health_snapshot(app, state).await;
    }

    if !snapshot.local.healthy {
        return Ok(snapshot);
    }

    if sequence.contains(&CodexRelayRepairAction::RepairPublic)
        && !snapshot.public.healthy
        && repair_attempt_allowed(
            &snapshot.public,
            now,
            relay_config.auto_repair_cooldown_seconds,
            false,
        )
    {
        store_repair_progress(
            state,
            CodexRelayRepairTarget::Public,
            now,
            relay_config.auto_repair_cooldown_seconds,
            true,
            None,
        );
        let public_result = repair_public_relay(&relay_config).await;
        store_repair_progress(
            state,
            CodexRelayRepairTarget::Public,
            now,
            relay_config.auto_repair_cooldown_seconds,
            false,
            Some(match public_result {
                Ok(message) => message,
                Err(error) => error,
            }),
        );
        snapshot = refresh_codex_relay_health_snapshot(app, state).await;
    }

    Ok(snapshot)
}

pub(crate) async fn repair_codex_relay_health(
    app: &tauri::AppHandle,
    state: &AppState,
    manual: bool,
) -> Result<CodexRelayHealthSnapshot, String> {
    let _repair_guard = RELAY_REPAIR_LOCK
        .try_lock()
        .map_err(|_| "Codex relay repair is already in progress".to_string())?;

    let mut snapshot = refresh_codex_relay_health_snapshot(app, state).await;
    let sequence = build_repair_sequence(&snapshot);
    if sequence.is_empty() {
        return Ok(snapshot);
    }

    let config = get_or_load_codex_config(app, state)?;
    let relay_config = resolve_relay_config(&config.relay);
    let now = Utc::now().timestamp();

    if sequence.contains(&CodexRelayRepairAction::RepairLocal) {
        if repair_attempt_allowed(
            &snapshot.local,
            now,
            relay_config.auto_repair_cooldown_seconds,
            manual,
        ) {
            store_repair_progress(
                state,
                CodexRelayRepairTarget::Local,
                now,
                relay_config.auto_repair_cooldown_seconds,
                true,
                None,
            );
            let local_result = if manual {
                ensure_local_codex_relay_running(app, state, true).await
            } else {
                ensure_local_codex_relay_core_running(app, state)
                    .await
                    .map(|_| ())
            };
            store_repair_progress(
                state,
                CodexRelayRepairTarget::Local,
                now,
                relay_config.auto_repair_cooldown_seconds,
                false,
                Some(match local_result {
                    Ok(()) => "Local relay restart requested".to_string(),
                    Err(error) => error,
                }),
            );
            snapshot = refresh_codex_relay_health_snapshot(app, state).await;
        }

        if !snapshot.local.healthy {
            return Ok(snapshot);
        }
    }

    if sequence.contains(&CodexRelayRepairAction::RepairPublic)
        && !snapshot.public.healthy
        && repair_attempt_allowed(
            &snapshot.public,
            now,
            relay_config.auto_repair_cooldown_seconds,
            manual,
        )
    {
        store_repair_progress(
            state,
            CodexRelayRepairTarget::Public,
            now,
            relay_config.auto_repair_cooldown_seconds,
            true,
            None,
        );
        let public_result = repair_public_relay(&relay_config).await;
        store_repair_progress(
            state,
            CodexRelayRepairTarget::Public,
            now,
            relay_config.auto_repair_cooldown_seconds,
            false,
            Some(match public_result {
                Ok(message) => message,
                Err(error) => error,
            }),
        );
        snapshot = refresh_codex_relay_health_snapshot(app, state).await;
    }

    Ok(snapshot)
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
    fn codex_relay_probe_outcome_preserves_typed_failures() {
        let outcome = build_probe_outcome(
            "public relay",
            &CodexRelayLayerHealth::default(),
            123,
            Err(CodexRelayProbeFailure::HttpStatus(502)),
        );

        assert_eq!(
            outcome.failure,
            Some(CodexRelayProbeFailure::HttpStatus(502))
        );
        assert_eq!(
            outcome.health.last_error.as_deref(),
            Some("public relay returned HTTP 502")
        );
    }

    #[test]
    fn codex_relay_probe_skipped_layer_preserves_last_checked_history() {
        let previous = CodexRelayLayerHealth {
            last_checked_at: Some(77),
            last_success_at: Some(55),
            last_error: Some("old error".into()),
            ..Default::default()
        };

        let next = build_skipped_layer_health(&previous, "public relay probe skipped");

        assert_eq!(next.last_checked_at, Some(77));
        assert_eq!(next.last_success_at, Some(55));
        assert_eq!(
            next.last_error.as_deref(),
            Some("public relay probe skipped")
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

    #[test]
    fn codex_relay_repair_sequence_repairs_local_before_public() {
        let mut snapshot = CodexRelayHealthSnapshot::default();
        snapshot.local = CodexRelayLayerHealth {
            healthy: false,
            last_checked_at: Some(100),
            ..Default::default()
        };
        snapshot.public = CodexRelayLayerHealth {
            healthy: false,
            last_checked_at: Some(100),
            ..Default::default()
        };

        assert_eq!(
            build_repair_sequence(&snapshot),
            vec![
                CodexRelayRepairAction::RepairLocal,
                CodexRelayRepairAction::ProbeLocal,
                CodexRelayRepairAction::RepairPublic,
                CodexRelayRepairAction::ProbePublic,
            ]
        );
    }

    #[test]
    fn codex_relay_repair_auto_respects_cooldown() {
        let layer = CodexRelayLayerHealth {
            cooldown_until: Some(500),
            ..Default::default()
        };

        assert!(!repair_attempt_allowed(&layer, 450, 600, false));
        assert!(repair_attempt_allowed(&layer, 500, 600, false));
    }

    #[test]
    fn codex_relay_repair_manual_bypasses_cooldown_but_not_active_repair() {
        let cooldown_layer = CodexRelayLayerHealth {
            cooldown_until: Some(999),
            ..Default::default()
        };
        assert!(repair_attempt_allowed(&cooldown_layer, 100, 600, true));

        let active_layer = CodexRelayLayerHealth {
            repair_in_progress: true,
            cooldown_until: Some(999),
            ..Default::default()
        };
        assert!(!repair_attempt_allowed(&active_layer, 100, 600, true));
    }

    #[test]
    fn relay_env_file_search_walks_up_parent_directories() {
        let unique = Utc::now()
            .timestamp_nanos_opt()
            .expect("timestamp nanos should be available");
        let root = std::env::temp_dir().join(format!("codex-relay-env-{unique}"));
        let nested = root.join("src-tauri").join("debug");

        std::fs::create_dir_all(&nested).expect("nested directories should be created");
        let env_path = root.join(".env.relay");
        std::fs::write(&env_path, "ATM_RELAY_HOST=ubuntu@example-host\n")
            .expect("env file should be written");

        let found = relay_env_file_path_from(&nested);

        std::fs::remove_dir_all(&root).expect("temp directory should be removed");

        assert_eq!(found, Some(env_path));
    }
}
