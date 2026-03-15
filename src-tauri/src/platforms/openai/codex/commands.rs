//! Codex 模块 Tauri Commands
//!
//! 提供给前端调用的 Codex API 管理命令
use std::fs;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

use super::archive::{
    ArchiveSessionConfidence, export_archive_session_jsonl,
    rebuild_materialized_archive_session_files,
};
use super::archive_storage::ArchiveSessionRow;
use super::logger::RequestLogger;
use super::models::{
    DailyStatsResponse, GatewayDailyStatsResponse, LogPage, LogQuery, ModelTokenStats,
    PeriodTokenStats, PoolStrategy, RequestLog, TokenStats,
};
use super::pool::{CodexServerConfig, CodexServerStatus};
use super::team_profiles::{
    TeamProfilePreset, generate_team_gateway_api_key, import_team_template_into_profiles,
    normalize_member_code, team_profile_presets,
};
use crate::AppState;
use crate::core::gateway_access::{GatewayAccessProfile, GatewayAccessProfiles, GatewayTarget};
use crate::platforms::openai::codex::server::CodexServer;
static QUOTA_REFRESH_TASK: std::sync::LazyLock<TokioMutex<Option<tokio::task::JoinHandle<()>>>> =
    std::sync::LazyLock::new(|| TokioMutex::new(None));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexAccessConfig {
    pub server_url: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexRuntimeSettings {
    pub quota_refresh_enabled: bool,
    pub quota_refresh_interval_seconds: u64,
    pub fast_mode_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexGatewayProfileEntry {
    pub id: String,
    pub name: String,
    pub api_key: String,
    pub enabled: bool,
    pub is_primary: bool,
    pub member_code: Option<String>,
    pub role_title: Option<String>,
    pub persona_summary: Option<String>,
    pub color: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct CodexGatewayProfileMutation {
    name: Option<String>,
    api_key: Option<String>,
    enabled: Option<bool>,
    member_code: Option<String>,
    role_title: Option<String>,
    persona_summary: Option<String>,
    color: Option<String>,
    notes: Option<String>,
}

const CODEX_CONFIG_FILE: &str = "openai_codex_config.json";
const SHARED_API_SERVER_PORT: u16 = 8766;
const MIN_QUOTA_REFRESH_INTERVAL_SECONDS: u64 = 60;
const MAX_QUOTA_REFRESH_INTERVAL_SECONDS: u64 = 24 * 60 * 60;
const CODEX_GATEWAY_PROFILE_ID: &str = "codex-default";
const CODEX_GATEWAY_PROFILE_NAME: &str = "Codex Default";
const CODEX_GATEWAY_PROFILE_NAME_PREFIX: &str = "Codex Key";

fn normalize_access_fields(config: &mut CodexServerConfig) {
    config.api_key = config.api_key.as_ref().and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
}

fn normalize_server_port(config: &mut CodexServerConfig) {
    config.port = SHARED_API_SERVER_PORT;
}

fn normalize_runtime_fields(config: &mut CodexServerConfig) {
    let defaults = CodexServerConfig::default();

    if config.pool_strategy.trim().is_empty() {
        config.pool_strategy = defaults.pool_strategy;
    }

    if config.quota_refresh_interval_seconds < MIN_QUOTA_REFRESH_INTERVAL_SECONDS {
        config.quota_refresh_interval_seconds = defaults
            .quota_refresh_interval_seconds
            .max(MIN_QUOTA_REFRESH_INTERVAL_SECONDS);
    }
    if config.quota_refresh_interval_seconds > MAX_QUOTA_REFRESH_INTERVAL_SECONDS {
        config.quota_refresh_interval_seconds = MAX_QUOTA_REFRESH_INTERVAL_SECONDS;
    }

    config.relay.host = normalize_optional_gateway_field(config.relay.host.take());
    config.relay.control_socket =
        normalize_optional_gateway_field(config.relay.control_socket.take());

    config.relay.public_base_url = config.relay.public_base_url.trim().to_string();
    if config.relay.public_base_url.is_empty() {
        config.relay.public_base_url = defaults.relay.public_base_url;
    }
    if config.relay.remote_port == 0 {
        config.relay.remote_port = defaults.relay.remote_port;
    }
    if config.relay.local_port == 0 {
        config.relay.local_port = defaults.relay.local_port;
    }
    if config.relay.health_check_interval_seconds < defaults.relay.health_check_interval_seconds {
        config.relay.health_check_interval_seconds = defaults.relay.health_check_interval_seconds;
    }
    if config.relay.auto_repair_cooldown_seconds < defaults.relay.auto_repair_cooldown_seconds {
        config.relay.auto_repair_cooldown_seconds = defaults.relay.auto_repair_cooldown_seconds;
    }
}

fn merge_start_codex_server_config(config: &mut CodexServerConfig, existing: &CodexServerConfig) {
    if config.api_key.is_none() {
        config.api_key = existing.api_key.clone();
    }
    if config.pool_strategy.trim().is_empty() {
        config.pool_strategy = existing.pool_strategy.clone();
    }
    if config.selected_account_id.is_none() {
        config.selected_account_id = existing.selected_account_id.clone();
    }

    // Runtime and relay settings are managed outside the start/stop dialog.
    config.quota_refresh_enabled = existing.quota_refresh_enabled;
    config.quota_refresh_interval_seconds = existing.quota_refresh_interval_seconds;
    config.fast_mode_enabled = existing.fast_mode_enabled;
    config.relay = existing.relay.clone();
}

fn runtime_settings_from_config(config: &CodexServerConfig) -> CodexRuntimeSettings {
    CodexRuntimeSettings {
        quota_refresh_enabled: config.quota_refresh_enabled,
        quota_refresh_interval_seconds: config.quota_refresh_interval_seconds,
        fast_mode_enabled: config.fast_mode_enabled,
    }
}

/// 根据快速模式开关同步编辑用户目录下的 ~/.codex/config.toml：
/// 开启时写入 service_tier = "fast" 和 [features] fast_mode = true；
/// 关闭时移除这两处。路径与 Codex 切换账号一致。
fn apply_fast_mode_to_codex_config_toml(
    app: &tauri::AppHandle,
    fast_mode_enabled: bool,
) -> Result<(), String> {
    let home_dir = app
        .path()
        .home_dir()
        .map_err(|e| format!("Failed to get home directory: {}", e))?;
    let codex_dir = home_dir.join(".codex");
    let config_path = codex_dir.join("config.toml");

    if !fast_mode_enabled && !config_path.exists() {
        return Ok(());
    }

    let mut config: toml::Table = if config_path.exists() {
        let content = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config.toml: {}", e))?;
        toml::from_str(&content).map_err(|e| format!("Failed to parse config.toml: {}", e))?
    } else {
        fs::create_dir_all(&codex_dir)
            .map_err(|e| format!("Failed to create .codex directory: {}", e))?;
        toml::Table::new()
    };

    if fast_mode_enabled {
        config.insert(
            "service_tier".to_string(),
            toml::Value::String("fast".to_string()),
        );
        let mut features = toml::Table::new();
        features.insert("fast_mode".to_string(), toml::Value::Boolean(true));
        config.insert("features".to_string(), toml::Value::Table(features));
    } else {
        config.remove("service_tier");
        config.remove("features");
    }

    let content = toml::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config.toml: {}", e))?;
    fs::write(&config_path, content).map_err(|e| format!("Failed to write config.toml: {}", e))?;
    Ok(())
}

fn read_persisted_config(app: &tauri::AppHandle) -> Result<Option<CodexServerConfig>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    let config_path = app_data_dir.join(CODEX_CONFIG_FILE);
    if !config_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read {}: {}", CODEX_CONFIG_FILE, e))?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    let mut config: CodexServerConfig = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", CODEX_CONFIG_FILE, e))?;
    normalize_access_fields(&mut config);
    normalize_server_port(&mut config);
    normalize_runtime_fields(&mut config);
    Ok(Some(config))
}

fn write_persisted_config(
    app: &tauri::AppHandle,
    config: &CodexServerConfig,
) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("Failed to create app data directory: {}", e))?;
    let config_path = app_data_dir.join(CODEX_CONFIG_FILE);
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize codex config: {}", e))?;
    fs::write(&config_path, content)
        .map_err(|e| format!("Failed to write {}: {}", CODEX_CONFIG_FILE, e))?;
    Ok(())
}

pub(crate) fn get_or_load_codex_config(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<CodexServerConfig, String> {
    if let Some(config) = state.codex_server_config.lock().unwrap().clone() {
        return Ok(config);
    }
    if let Some(config) = read_persisted_config(app)? {
        let _ = write_persisted_config(app, &config);
        *state.codex_server_config.lock().unwrap() = Some(config.clone());
        return Ok(config);
    }
    let mut config = CodexServerConfig::default();
    normalize_server_port(&mut config);
    normalize_runtime_fields(&mut config);
    let _ = write_persisted_config(app, &config);
    *state.codex_server_config.lock().unwrap() = Some(config.clone());
    Ok(config)
}

pub(crate) fn current_api_server_port(state: &AppState) -> u16 {
    state
        .api_server
        .lock()
        .unwrap()
        .as_ref()
        .map(|server| server.get_port())
        .unwrap_or(SHARED_API_SERVER_PORT)
}

pub(crate) fn gateway_server_url(port: u16) -> String {
    format!("http://127.0.0.1:{}/v1", port)
}

fn normalize_gateway_api_key(api_key: Option<String>) -> Option<String> {
    api_key.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn generate_gateway_api_key() -> String {
    format!("sk-{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn normalize_optional_gateway_field(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_gateway_member_code(member_code: Option<String>) -> Option<String> {
    member_code.and_then(|value| normalize_member_code(&value))
}

fn team_profile_preset_for_member_code(member_code: Option<&str>) -> Option<TeamProfilePreset> {
    let normalized = member_code.and_then(normalize_member_code)?;

    team_profile_presets().iter().copied().find(|preset| {
        normalize_member_code(preset.member_code).as_deref() == Some(normalized.as_str())
    })
}

fn generate_gateway_api_key_for_member_code(member_code: Option<&str>) -> String {
    match member_code.and_then(normalize_member_code) {
        Some(member_code) => generate_team_gateway_api_key(&member_code),
        None => generate_gateway_api_key(),
    }
}

fn ensure_unique_codex_member_code(
    profiles: &GatewayAccessProfiles,
    member_code: Option<&str>,
    ignored_profile_id: Option<&str>,
) -> Result<Option<String>, String> {
    let normalized = member_code.and_then(normalize_member_code);
    let Some(expected) = normalized.clone() else {
        return Ok(None);
    };

    let has_conflict = profiles
        .list_by_target(GatewayTarget::Codex)
        .into_iter()
        .any(|profile| {
            ignored_profile_id != Some(profile.id.as_str())
                && profile
                    .member_code
                    .as_deref()
                    .and_then(normalize_member_code)
                    .as_deref()
                    == Some(expected.as_str())
        });

    if has_conflict {
        Err(format!(
            "Codex gateway member code already exists: {}",
            expected
        ))
    } else {
        Ok(Some(expected))
    }
}

fn default_codex_gateway_profile_name(profiles: &GatewayAccessProfiles) -> String {
    let next_index = profiles
        .list_by_target(GatewayTarget::Codex)
        .len()
        .saturating_add(1);
    format!("{} {}", CODEX_GATEWAY_PROFILE_NAME_PREFIX, next_index)
}

fn codex_gateway_profile_entry(
    profile: GatewayAccessProfile,
    primary_profile_id: Option<&str>,
) -> CodexGatewayProfileEntry {
    let GatewayAccessProfile {
        id,
        name,
        api_key,
        enabled,
        member_code,
        role_title,
        persona_summary,
        color,
        notes,
        ..
    } = profile;

    CodexGatewayProfileEntry {
        is_primary: primary_profile_id
            .map(|expected| expected == id)
            .unwrap_or(false),
        id,
        name,
        api_key: api_key.trim().to_string(),
        enabled,
        member_code,
        role_title,
        persona_summary,
        color,
        notes,
    }
}

fn codex_gateway_profiles(profiles: &GatewayAccessProfiles) -> Vec<CodexGatewayProfileEntry> {
    let primary_profile_id = profiles
        .list_by_target(GatewayTarget::Codex)
        .into_iter()
        .find_map(|profile| {
            if profile.enabled && !profile.api_key.trim().is_empty() {
                Some(profile.id)
            } else {
                None
            }
        });
    profiles
        .list_by_target(GatewayTarget::Codex)
        .into_iter()
        .map(|profile| codex_gateway_profile_entry(profile, primary_profile_id.as_deref()))
        .collect()
}

fn codex_gateway_profile_entry_by_id(
    profiles: &GatewayAccessProfiles,
    profile_id: &str,
) -> Result<CodexGatewayProfileEntry, String> {
    codex_gateway_profiles(profiles)
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("Codex gateway profile not found: {}", profile_id))
}

fn codex_gateway_profile_by_id(
    profiles: &GatewayAccessProfiles,
    profile_id: &str,
) -> Result<GatewayAccessProfile, String> {
    profiles
        .list_by_target(GatewayTarget::Codex)
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("Codex gateway profile not found: {}", profile_id))
}

fn import_codex_team_template_profiles(profiles: GatewayAccessProfiles) -> GatewayAccessProfiles {
    import_team_template_into_profiles(profiles)
}

fn create_codex_gateway_profile_in_profiles(
    profiles: &mut GatewayAccessProfiles,
    mutation: CodexGatewayProfileMutation,
) -> Result<GatewayAccessProfile, String> {
    let member_code =
        ensure_unique_codex_member_code(profiles, mutation.member_code.as_deref(), None)?;
    let entry = GatewayAccessProfile {
        id: format!("codex-{}", Uuid::new_v4().simple()),
        name: normalize_gateway_profile_name(mutation.name)
            .unwrap_or_else(|| default_codex_gateway_profile_name(profiles)),
        target: GatewayTarget::Codex,
        api_key: normalize_gateway_api_key(mutation.api_key)
            .unwrap_or_else(|| generate_gateway_api_key_for_member_code(member_code.as_deref())),
        enabled: mutation.enabled.unwrap_or(true),
        member_code,
        role_title: normalize_optional_gateway_field(mutation.role_title),
        persona_summary: normalize_optional_gateway_field(mutation.persona_summary),
        color: normalize_optional_gateway_field(mutation.color),
        notes: normalize_optional_gateway_field(mutation.notes),
    };

    profiles.upsert_profile(entry.clone());
    Ok(entry)
}

fn update_codex_gateway_profile_in_profiles(
    profiles: &mut GatewayAccessProfiles,
    profile_id: &str,
    mutation: CodexGatewayProfileMutation,
) -> Result<GatewayAccessProfile, String> {
    let mut existing = codex_gateway_profile_by_id(profiles, profile_id)?;
    let current_member_code = existing
        .member_code
        .as_deref()
        .and_then(normalize_member_code);

    if let Some(name_input) = mutation.name {
        if let Some(name) = normalize_gateway_profile_name(Some(name_input)) {
            existing.name = name;
        }
    }

    if let Some(api_key_input) = mutation.api_key {
        existing.api_key = normalize_gateway_api_key(Some(api_key_input))
            .ok_or_else(|| "Codex gateway API key cannot be empty".to_string())?;
    }

    if let Some(enabled) = mutation.enabled {
        existing.enabled = enabled;
    }

    if let Some(member_code_input) = mutation.member_code {
        let normalized_member_code = normalize_gateway_member_code(Some(member_code_input));
        if normalized_member_code != current_member_code {
            ensure_unique_codex_member_code(
                profiles,
                normalized_member_code.as_deref(),
                Some(existing.id.as_str()),
            )?;
        }
        existing.member_code = normalized_member_code;
    }

    if mutation.role_title.is_some() {
        existing.role_title = normalize_optional_gateway_field(mutation.role_title);
    }
    if mutation.persona_summary.is_some() {
        existing.persona_summary = normalize_optional_gateway_field(mutation.persona_summary);
    }
    if mutation.color.is_some() {
        existing.color = normalize_optional_gateway_field(mutation.color);
    }
    if mutation.notes.is_some() {
        existing.notes = normalize_optional_gateway_field(mutation.notes);
    }

    let updated_id = existing.id.clone();
    profiles.upsert_profile(existing.clone());

    codex_gateway_profile_by_id(profiles, &updated_id)
}

fn regenerate_codex_gateway_profile_api_key_in_profiles(
    profiles: &mut GatewayAccessProfiles,
    profile_id: &str,
) -> Result<GatewayAccessProfile, String> {
    let mut existing = codex_gateway_profile_by_id(profiles, profile_id)?;
    existing.api_key = generate_gateway_api_key_for_member_code(existing.member_code.as_deref());
    let updated_id = existing.id.clone();
    profiles.upsert_profile(existing);

    codex_gateway_profile_by_id(profiles, &updated_id)
}

fn reset_codex_gateway_profile_to_team_defaults_in_profiles(
    profiles: &mut GatewayAccessProfiles,
    profile_id: &str,
) -> Result<GatewayAccessProfile, String> {
    let mut existing = codex_gateway_profile_by_id(profiles, profile_id)?;
    let preset =
        team_profile_preset_for_member_code(existing.member_code.as_deref()).ok_or_else(|| {
            "Only built-in team members can be reset to template defaults".to_string()
        })?;

    existing.name = preset.name.to_string();
    existing.enabled = true;
    existing.member_code = Some(preset.member_code.to_string());
    existing.role_title = Some(preset.role_title.to_string());
    existing.persona_summary = Some(preset.persona_summary.to_string());
    existing.color = Some(preset.color.to_string());
    existing.notes = None;

    if existing.api_key.trim().is_empty() {
        existing.api_key = generate_team_gateway_api_key(preset.member_code);
    }

    let updated_id = existing.id.clone();
    profiles.upsert_profile(existing);

    codex_gateway_profile_by_id(profiles, &updated_id)
}

fn delete_codex_gateway_profile_in_profiles(
    profiles: &mut GatewayAccessProfiles,
    profile_id: &str,
) -> Result<(), String> {
    codex_gateway_profile_by_id(profiles, profile_id)?;

    if profiles.remove_profile(profile_id) {
        Ok(())
    } else {
        Err(format!("Codex gateway profile not found: {}", profile_id))
    }
}

pub(crate) fn gateway_api_key_for_target(
    app: &tauri::AppHandle,
    state: &AppState,
    target: GatewayTarget,
) -> Result<Option<String>, String> {
    let profiles = crate::core::gateway_access::get_or_load_gateway_access_profiles(app, state)?;
    Ok(profiles.first_enabled_api_key_for_target(target))
}

fn sync_legacy_codex_access_profile(
    mut profiles: GatewayAccessProfiles,
    api_key: Option<String>,
) -> GatewayAccessProfiles {
    match normalize_gateway_api_key(api_key) {
        Some(api_key) => {
            profiles.upsert_profile(GatewayAccessProfile {
                id: CODEX_GATEWAY_PROFILE_ID.to_string(),
                name: CODEX_GATEWAY_PROFILE_NAME.to_string(),
                target: GatewayTarget::Codex,
                api_key,
                enabled: true,
                member_code: None,
                role_title: None,
                persona_summary: None,
                color: None,
                notes: None,
            });
        }
        None => {
            profiles.remove_profile(CODEX_GATEWAY_PROFILE_ID);
        }
    }

    profiles
}

fn sync_codex_gateway_profile(
    app: &tauri::AppHandle,
    state: &AppState,
    api_key: Option<String>,
) -> Result<(), String> {
    let profiles = crate::core::gateway_access::get_or_load_gateway_access_profiles(app, state)?;
    let profiles = sync_legacy_codex_access_profile(profiles, api_key);
    let profiles = crate::core::gateway_access::set_gateway_access_profiles(app, state, profiles)?;
    sync_codex_config_api_key_from_profiles(app, state, &profiles)?;
    Ok(())
}

fn sync_codex_config_api_key_from_profiles(
    app: &tauri::AppHandle,
    state: &AppState,
    profiles: &GatewayAccessProfiles,
) -> Result<(), String> {
    let mut config = get_or_load_codex_config(app, state)?;
    config.api_key = profiles.first_enabled_api_key_for_target(GatewayTarget::Codex);
    normalize_access_fields(&mut config);
    normalize_server_port(&mut config);
    normalize_runtime_fields(&mut config);
    *state.codex_server_config.lock().unwrap() = Some(config.clone());
    write_persisted_config(app, &config)?;
    Ok(())
}

fn normalize_gateway_profile_name(name: Option<String>) -> Option<String> {
    name.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

async fn apply_periodic_tasks(app: tauri::AppHandle, state: &AppState, config: &CodexServerConfig) {
    if config.enabled && config.quota_refresh_enabled {
        start_periodic_quota_refresh(app, state, config.quota_refresh_interval_seconds).await;
    } else {
        stop_periodic_quota_refresh().await;
    }
}

async fn refresh_relay_health_snapshot(app: &tauri::AppHandle, state: &AppState) {
    let _ = super::relay::refresh_codex_relay_health_snapshot(app, state).await;
}

async fn init_codex_runtime(
    app: &tauri::AppHandle,
    state: &AppState,
    config: &CodexServerConfig,
) -> Result<(), String> {
    let pool = if let Some(existing) = state.codex_pool.lock().unwrap().clone() {
        existing
    } else {
        let created = Arc::new(crate::platforms::openai::codex::pool::CodexPool::new());
        *state.codex_pool.lock().unwrap() = Some(created.clone());
        created
    };

    let accounts = crate::platforms::openai::modules::storage::list_accounts(app).await?;
    pool.refresh_from_accounts(&accounts).await;
    let strategy = match config.pool_strategy.as_str() {
        "single" => PoolStrategy::Single,
        "smart" => PoolStrategy::Smart,
        _ => PoolStrategy::RoundRobin,
    };
    pool.set_strategy(strategy).await;
    if let Some(ref account_id) = config.selected_account_id {
        pool.set_selected_account_id(account_id.clone()).await;
    }

    let executor =
        Arc::new(crate::platforms::openai::codex::executor::CodexExecutor::new(pool.clone())?);
    *state.codex_executor.lock().unwrap() = Some(executor);

    // 初始化 logger
    if state.codex_logger.lock().unwrap().is_none() {
        let logger = Arc::new(tokio::sync::RwLock::new(RequestLogger::new(3000)));
        *state.codex_logger.lock().unwrap() = Some(logger);
    }

    Ok(())
}

pub async fn init_codex_enabled_state_on_startup(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<(), String> {
    if let Err(err) = crate::core::gateway_access::get_or_load_gateway_access_profiles(app, state) {
        eprintln!("Failed to initialize gateway access profiles: {}", err);
    }

    let mut config = get_or_load_codex_config(app, state)?;
    normalize_access_fields(&mut config);
    normalize_server_port(&mut config);
    normalize_runtime_fields(&mut config);

    if config.enabled {
        init_codex_runtime(app, state, &config).await?;
        *state.codex_server.lock().unwrap() = Some(CodexServer::new(config.port));
    } else {
        *state.codex_server.lock().unwrap() = None;
    }

    apply_periodic_tasks(app.clone(), state, &config).await;

    *state.codex_server_config.lock().unwrap() = Some(config.clone());
    write_persisted_config(app, &config)?;
    if config.enabled {
        refresh_relay_health_snapshot(app, state).await;
    }
    Ok(())
}

/// 获取 Codex 服务器状态
#[tauri::command]
pub async fn get_codex_server_status(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<CodexServerStatus, String> {
    let running = state.codex_server.lock().unwrap().is_some();
    let pool = state.codex_pool.lock().unwrap().clone();
    let _cfg = get_or_load_codex_config(&app, state.inner())?;

    let pool_status = if let Some(p) = pool {
        Some(p.status().await)
    } else {
        None
    };

    let status = CodexServerStatus {
        running,
        port: current_api_server_port(state.inner()),
        address: format!(
            "http://127.0.0.1:{}",
            current_api_server_port(state.inner())
        ),
        pool_status,
    };

    Ok(status)
}

/// 启动 Codex 服务器
#[tauri::command]
pub async fn start_codex_server(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    mut config: CodexServerConfig,
) -> Result<(), String> {
    {
        let server = state.codex_server.lock().unwrap();
        if server.is_some() {
            return Err("Codex server is already running".to_string());
        }
    }

    let need_start_api_server = state.api_server.lock().unwrap().is_none();
    if need_start_api_server {
        // 先更新持久化配置，确保 enabled = true
        let mut persisted_config = get_or_load_codex_config(&app, state.inner())?;
        persisted_config.enabled = true;
        write_persisted_config(&app, &persisted_config)?;
        *state.codex_server_config.lock().unwrap() = Some(persisted_config);

        crate::core::api_server::start_api_server_cmd(app.clone(), state.clone())
            .await
            .map_err(|e| format!("Failed to start API server: {}", e))?;
        return Ok(());
    }

    // 合并已有配置，避免前端只传部分字段时覆盖现有配置
    if let Ok(existing) = get_or_load_codex_config(&app, state.inner()) {
        merge_start_codex_server_config(&mut config, &existing);
    }
    normalize_access_fields(&mut config);
    normalize_server_port(&mut config);
    normalize_runtime_fields(&mut config);
    config.enabled = true;

    init_codex_runtime(&app, state.inner(), &config).await?;
    *state.codex_server.lock().unwrap() = Some(CodexServer::new(config.port));
    *state.codex_server_config.lock().unwrap() = Some(config.clone());
    write_persisted_config(&app, &config)?;
    apply_periodic_tasks(app.clone(), state.inner(), &config).await;
    refresh_relay_health_snapshot(&app, state.inner()).await;
    Ok(())
}

/// 停止 Codex 服务器
#[tauri::command]
pub async fn stop_codex_server(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // 停用 Codex 路由并更新持久化状态
    let was_running = {
        let mut server = state.codex_server.lock().unwrap();
        server.take().is_some()
    };
    if was_running {
        let mut config = get_or_load_codex_config(&app, state.inner())?;
        config.enabled = false;
        *state.codex_server_config.lock().unwrap() = Some(config.clone());
        write_persisted_config(&app, &config)?;
        apply_periodic_tasks(app.clone(), state.inner(), &config).await;
        refresh_relay_health_snapshot(&app, state.inner()).await;
        println!("Codex routes disabled");
        Ok(())
    } else {
        Err("Codex server is not running".to_string())
    }
}

/// 获取 Codex 号池状态
#[tauri::command]
pub async fn get_codex_pool_status(
    state: State<'_, AppState>,
) -> Result<crate::platforms::openai::codex::models::PoolStatus, String> {
    let pool = state.codex_pool.lock().unwrap().clone();
    if let Some(pool_ref) = pool {
        Ok(pool_ref.status().await)
    } else {
        Err("Codex pool not initialized".to_string())
    }
}

/// 刷新 Codex 号池
#[tauri::command]
pub async fn refresh_codex_pool(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::platforms::openai::codex::models::PoolStatus, String> {
    let pool_ref = if let Some(existing) = state.codex_pool.lock().unwrap().clone() {
        existing
    } else {
        let created = Arc::new(crate::platforms::openai::codex::pool::CodexPool::new());
        *state.codex_pool.lock().unwrap() = Some(created.clone());
        created
    };

    let accounts = crate::platforms::openai::modules::storage::list_accounts(&app).await?;
    pool_ref.refresh_from_accounts(&accounts).await;
    Ok(pool_ref.status().await)
}

/// 获取内存中的请求日志
#[tauri::command]
pub async fn get_codex_logs(
    state: State<'_, AppState>,
    limit: usize,
) -> Result<Vec<RequestLog>, String> {
    let logger = state.codex_logger.lock().unwrap().clone();
    if let Some(log) = logger {
        let guard = log.read().await;
        Ok(guard.get_recent_logs(limit))
    } else {
        Ok(vec![])
    }
}

#[tauri::command]
pub async fn query_codex_logs(
    state: State<'_, AppState>,
    query: LogQuery,
) -> Result<LogPage, String> {
    let logger = state.codex_logger.lock().unwrap().clone();
    if let Some(log) = logger {
        let guard = log.read().await;
        Ok(guard.query_logs(&query))
    } else {
        Ok(LogPage {
            total: 0,
            items: vec![],
        })
    }
}

/// 获取 Token 统计
#[tauri::command]
pub async fn get_codex_stats(
    state: State<'_, AppState>,
    start_date: String,
    end_date: String,
) -> Result<TokenStats, String> {
    let logger = state.codex_logger.lock().unwrap().clone();
    if let Some(log) = logger {
        let guard = log.read().await;
        guard.get_stats(&start_date, &end_date)
    } else {
        Ok(TokenStats {
            start_date,
            end_date,
            total_requests: 0,
            total_tokens: 0,
            per_account: vec![],
        })
    }
}

#[tauri::command]
pub async fn get_codex_period_stats(
    state: State<'_, AppState>,
) -> Result<PeriodTokenStats, String> {
    let logger = state.codex_logger.lock().unwrap().clone();
    if let Some(log) = logger {
        let guard = log.read().await;
        Ok(guard.get_period_stats(chrono::Utc::now().timestamp()))
    } else {
        Ok(PeriodTokenStats {
            today_requests: 0,
            today_tokens: 0,
            week_requests: 0,
            week_tokens: 0,
            month_requests: 0,
            month_tokens: 0,
        })
    }
}

#[tauri::command]
pub async fn get_codex_model_stats(
    state: State<'_, AppState>,
    start_ts: i64,
    end_ts: i64,
) -> Result<Vec<ModelTokenStats>, String> {
    let logger = state.codex_logger.lock().unwrap().clone();
    if let Some(log) = logger {
        let guard = log.read().await;
        Ok(guard.get_model_stats(start_ts, end_ts))
    } else {
        Ok(vec![])
    }
}

#[tauri::command]
pub async fn clear_codex_logs(state: State<'_, AppState>) -> Result<(), String> {
    let logger = state.codex_logger.lock().unwrap().clone();
    if let Some(log) = logger {
        let mut guard = log.write().await;
        guard.clear();
        Ok(())
    } else {
        Ok(())
    }
}

/// 设置号池策略并持久化
#[tauri::command]
pub async fn set_codex_pool_strategy(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    strategy: String,
) -> Result<(), String> {
    let pool = state.codex_pool.lock().unwrap().clone();
    if let Some(pool_ref) = pool {
        let strategy_enum = match strategy.as_str() {
            "round-robin" => PoolStrategy::RoundRobin,
            "single" => PoolStrategy::Single,
            "smart" => PoolStrategy::Smart,
            _ => return Err(format!("Invalid strategy: {}", strategy)),
        };
        pool_ref.set_strategy(strategy_enum).await;
        let mut config = get_or_load_codex_config(&app, state.inner())?;
        config.pool_strategy = strategy;
        *state.codex_server_config.lock().unwrap() = Some(config.clone());
        write_persisted_config(&app, &config)?;

        Ok(())
    } else {
        Err("Codex pool not initialized".to_string())
    }
}

#[tauri::command]

pub async fn set_codex_selected_account(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> Result<(), String> {
    let pool = state.codex_pool.lock().unwrap().clone();
    if let Some(pool_ref) = pool {
        pool_ref.set_selected_account_id(account_id.clone()).await;
        let mut config = get_or_load_codex_config(&app, state.inner())?;
        config.selected_account_id = Some(account_id);
        *state.codex_server_config.lock().unwrap() = Some(config.clone());
        write_persisted_config(&app, &config)?;

        Ok(())
    } else {
        Err("Codex pool not initialized".to_string())
    }
}

#[tauri::command]
pub async fn get_codex_access_config(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<CodexAccessConfig, String> {
    Ok(CodexAccessConfig {
        server_url: gateway_server_url(current_api_server_port(state.inner())),
        api_key: gateway_api_key_for_target(
            &app,
            state.inner(),
            crate::core::gateway_access::GatewayTarget::Codex,
        )?,
    })
}

#[tauri::command]
pub async fn set_codex_access_config(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    api_key: Option<String>,
) -> Result<CodexAccessConfig, String> {
    let mut config = get_or_load_codex_config(&app, state.inner())?;
    config.api_key = api_key;
    normalize_access_fields(&mut config);
    normalize_server_port(&mut config);
    normalize_runtime_fields(&mut config);
    *state.codex_server_config.lock().unwrap() = Some(config.clone());
    write_persisted_config(&app, &config)?;
    sync_codex_gateway_profile(&app, state.inner(), config.api_key.clone())?;
    refresh_relay_health_snapshot(&app, state.inner()).await;

    Ok(CodexAccessConfig {
        server_url: gateway_server_url(current_api_server_port(state.inner())),
        api_key: gateway_api_key_for_target(&app, state.inner(), GatewayTarget::Codex)?,
    })
}

#[tauri::command]
pub async fn list_codex_gateway_profiles(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<CodexGatewayProfileEntry>, String> {
    let profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;
    Ok(codex_gateway_profiles(&profiles))
}

#[tauri::command]
pub async fn create_codex_gateway_profile(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    name: Option<String>,
    api_key: Option<String>,
    enabled: Option<bool>,
    member_code: Option<String>,
    role_title: Option<String>,
    persona_summary: Option<String>,
    color: Option<String>,
    notes: Option<String>,
) -> Result<CodexGatewayProfileEntry, String> {
    let mut profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;

    let entry = create_codex_gateway_profile_in_profiles(
        &mut profiles,
        CodexGatewayProfileMutation {
            name,
            api_key,
            enabled,
            member_code,
            role_title,
            persona_summary,
            color,
            notes,
        },
    )?;
    let created_id = entry.id.clone();

    let profiles =
        crate::core::gateway_access::set_gateway_access_profiles(&app, state.inner(), profiles)?;
    sync_codex_config_api_key_from_profiles(&app, state.inner(), &profiles)?;
    refresh_relay_health_snapshot(&app, state.inner()).await;
    codex_gateway_profile_entry_by_id(&profiles, &created_id)
        .map_err(|_| "Failed to load created Codex gateway profile".to_string())
}

#[tauri::command]
pub async fn update_codex_gateway_profile(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    name: Option<String>,
    api_key: Option<String>,
    enabled: Option<bool>,
    member_code: Option<String>,
    role_title: Option<String>,
    persona_summary: Option<String>,
    color: Option<String>,
    notes: Option<String>,
) -> Result<CodexGatewayProfileEntry, String> {
    let mut profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;
    let updated = update_codex_gateway_profile_in_profiles(
        &mut profiles,
        &profile_id,
        CodexGatewayProfileMutation {
            name,
            api_key,
            enabled,
            member_code,
            role_title,
            persona_summary,
            color,
            notes,
        },
    )?;
    let updated_id = updated.id.clone();

    let profiles =
        crate::core::gateway_access::set_gateway_access_profiles(&app, state.inner(), profiles)?;
    sync_codex_config_api_key_from_profiles(&app, state.inner(), &profiles)?;
    refresh_relay_health_snapshot(&app, state.inner()).await;
    codex_gateway_profile_entry_by_id(&profiles, &updated_id)
        .map_err(|_| "Failed to load updated Codex gateway profile".to_string())
}

#[tauri::command]
pub async fn import_codex_team_template(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<CodexGatewayProfileEntry>, String> {
    let profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;
    let profiles = import_codex_team_template_profiles(profiles);
    let profiles =
        crate::core::gateway_access::set_gateway_access_profiles(&app, state.inner(), profiles)?;
    sync_codex_config_api_key_from_profiles(&app, state.inner(), &profiles)?;
    refresh_relay_health_snapshot(&app, state.inner()).await;
    Ok(codex_gateway_profiles(&profiles))
}

#[tauri::command]
pub async fn regenerate_codex_gateway_profile_api_key(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<CodexGatewayProfileEntry, String> {
    let mut profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;
    let updated = regenerate_codex_gateway_profile_api_key_in_profiles(&mut profiles, &profile_id)?;
    let updated_id = updated.id.clone();
    let profiles =
        crate::core::gateway_access::set_gateway_access_profiles(&app, state.inner(), profiles)?;
    sync_codex_config_api_key_from_profiles(&app, state.inner(), &profiles)?;
    refresh_relay_health_snapshot(&app, state.inner()).await;
    codex_gateway_profile_entry_by_id(&profiles, &updated_id)
        .map_err(|_| "Failed to load regenerated Codex gateway profile".to_string())
}

#[tauri::command]
pub async fn reset_codex_gateway_profile_to_team_defaults(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<CodexGatewayProfileEntry, String> {
    let mut profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;
    let updated =
        reset_codex_gateway_profile_to_team_defaults_in_profiles(&mut profiles, &profile_id)?;
    let updated_id = updated.id.clone();
    let profiles =
        crate::core::gateway_access::set_gateway_access_profiles(&app, state.inner(), profiles)?;
    sync_codex_config_api_key_from_profiles(&app, state.inner(), &profiles)?;
    refresh_relay_health_snapshot(&app, state.inner()).await;
    codex_gateway_profile_entry_by_id(&profiles, &updated_id)
        .map_err(|_| "Failed to load reset Codex gateway profile".to_string())
}

#[tauri::command]
pub async fn delete_codex_gateway_profile(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), String> {
    let mut profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;
    delete_codex_gateway_profile_in_profiles(&mut profiles, &profile_id)?;

    let profiles =
        crate::core::gateway_access::set_gateway_access_profiles(&app, state.inner(), profiles)?;
    sync_codex_config_api_key_from_profiles(&app, state.inner(), &profiles)?;
    refresh_relay_health_snapshot(&app, state.inner()).await;
    Ok(())
}

/// 获取 Codex 运行时设置
#[tauri::command]
pub async fn get_codex_runtime_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<CodexRuntimeSettings, String> {
    let mut config = get_or_load_codex_config(&app, state.inner())?;
    normalize_runtime_fields(&mut config);
    Ok(runtime_settings_from_config(&config))
}

#[tauri::command]
pub async fn set_codex_runtime_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings: CodexRuntimeSettings,
) -> Result<CodexRuntimeSettings, String> {
    let mut config = get_or_load_codex_config(&app, state.inner())?;
    config.quota_refresh_enabled = settings.quota_refresh_enabled;
    config.quota_refresh_interval_seconds = settings.quota_refresh_interval_seconds;
    config.fast_mode_enabled = settings.fast_mode_enabled;
    normalize_access_fields(&mut config);
    normalize_server_port(&mut config);
    normalize_runtime_fields(&mut config);
    *state.codex_server_config.lock().unwrap() = Some(config.clone());
    write_persisted_config(&app, &config)?;
    apply_periodic_tasks(app.clone(), state.inner(), &config).await;
    apply_fast_mode_to_codex_config_toml(&app, settings.fast_mode_enabled)?;
    Ok(runtime_settings_from_config(&config))
}

#[tauri::command]
pub async fn query_codex_logs_from_storage(
    state: State<'_, AppState>,
    query: LogQuery,
) -> Result<LogPage, String> {
    let storage = state.codex_log_storage.lock().unwrap().clone();
    if let Some(s) = storage {
        s.query_logs(&query).map_err(|e| e.to_string())
    } else {
        Ok(LogPage {
            total: 0,
            items: vec![],
        })
    }
}
#[tauri::command]
pub async fn get_codex_model_stats_from_storage(
    state: State<'_, AppState>,
    start_ts: i64,
    end_ts: i64,
) -> Result<Vec<ModelTokenStats>, String> {
    let storage = state.codex_log_storage.lock().unwrap().clone();
    if let Some(s) = storage {
        s.get_model_stats(start_ts, end_ts)
            .map_err(|e| e.to_string())
    } else {
        Ok(vec![])
    }
}
#[tauri::command]
pub async fn get_codex_period_stats_from_storage(
    state: State<'_, AppState>,
) -> Result<PeriodTokenStats, String> {
    let storage = state.codex_log_storage.lock().unwrap().clone();
    if let Some(s) = storage {
        let now_ts = chrono::Utc::now().timestamp();
        s.get_period_stats(now_ts).map_err(|e| e.to_string())
    } else {
        Ok(PeriodTokenStats {
            today_requests: 0,
            today_tokens: 0,
            week_requests: 0,
            week_tokens: 0,
            month_requests: 0,
            month_tokens: 0,
        })
    }
}

/// 从 SQLite 存储获取每日统计
#[tauri::command]
pub async fn get_codex_daily_stats_from_storage(
    state: State<'_, AppState>,
    days: Option<u32>,
) -> Result<DailyStatsResponse, String> {
    let storage = state.codex_log_storage.lock().unwrap().clone();
    if let Some(s) = storage {
        let days = days.unwrap_or(30).min(365);
        s.get_daily_stats(days).map_err(|e| e.to_string())
    } else {
        Ok(DailyStatsResponse { stats: vec![] })
    }
}

#[tauri::command]
pub async fn get_codex_daily_stats_by_gateway_profile_from_storage(
    state: State<'_, AppState>,
    days: Option<u32>,
) -> Result<GatewayDailyStatsResponse, String> {
    let storage = state.codex_log_storage.lock().unwrap().clone();
    if let Some(s) = storage {
        let days = days.unwrap_or(30).min(365);
        s.get_daily_stats_by_gateway_profile(days)
            .map_err(|e| e.to_string())
    } else {
        Ok(GatewayDailyStatsResponse { series: vec![] })
    }
}

/// 清空 SQLite 存储中的日志
#[tauri::command]
pub async fn clear_codex_logs_in_storage(state: State<'_, AppState>) -> Result<usize, String> {
    let storage = state.codex_log_storage.lock().unwrap().clone();
    if let Some(s) = storage {
        s.clear_all().map_err(|e| e.to_string())
    } else {
        Ok(0)
    }
}

/// 删除指定日期之前的 SQLite 日志
#[tauri::command]
pub async fn delete_codex_logs_before(
    state: State<'_, AppState>,
    date_key: i64,
) -> Result<usize, String> {
    let storage = state.codex_log_storage.lock().unwrap().clone();
    if let Some(s) = storage {
        s.delete_before(date_key).map_err(|e| e.to_string())
    } else {
        Ok(0)
    }
}

/// 存储状态信息

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexLogStorageStatus {
    pub total_logs: i64,
    pub db_size_bytes: u64,
    pub db_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexArchiveStatus {
    pub total_sessions: i64,
    pub total_turns: i64,
    pub db_size_bytes: u64,
    pub db_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexArchiveSessionEntry {
    pub archive_session_id: String,
    pub gateway_profile_id: String,
    pub gateway_profile_name: Option<String>,
    pub member_code: Option<String>,
    pub display_label: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub explicit_session_id: Option<String>,
    pub confidence: String,
    pub source: String,
    pub originator: Option<String>,
    pub client_user_agent: Option<String>,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub turn_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexArchiveExportPayload {
    pub archive_session_id: String,
    pub jsonl: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexArchiveMaterializationResult {
    pub exported_count: usize,
    pub output_paths: Vec<String>,
}

/// 获取日志存储状态
#[tauri::command]
pub async fn get_codex_log_storage_status(
    state: State<'_, AppState>,
) -> Result<CodexLogStorageStatus, String> {
    let storage = state.codex_log_storage.lock().unwrap().clone();
    if let Some(s) = storage {
        let total_logs = s.total_logs().map_err(|e| e.to_string())?;
        let db_size = s.db_size().map_err(|e| e.to_string())?;
        Ok(CodexLogStorageStatus {
            total_logs,
            db_size_bytes: db_size,
            db_path: format!("{:?}", s),
        })
    } else {
        Err("Codex log storage not initialized".to_string())
    }
}

#[tauri::command]
pub async fn get_codex_archive_status(
    state: State<'_, AppState>,
) -> Result<CodexArchiveStatus, String> {
    let storage = get_codex_archive_storage(state.inner())?;
    let total_sessions = storage.total_sessions()?;
    let total_turns = storage.total_turns()?;
    let db_size_bytes = storage.db_size()?;

    Ok(CodexArchiveStatus {
        total_sessions,
        total_turns,
        db_size_bytes,
        db_path: storage.db_path().display().to_string(),
    })
}

#[tauri::command]
pub async fn list_codex_archive_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<CodexArchiveSessionEntry>, String> {
    let storage = get_codex_archive_storage(state.inner())?;

    storage.list_sessions().map(|sessions| {
        sessions
            .into_iter()
            .map(codex_archive_session_entry)
            .collect()
    })
}

#[tauri::command]
pub async fn export_codex_archive_session(
    state: State<'_, AppState>,
    archive_session_id: String,
) -> Result<CodexArchiveExportPayload, String> {
    let storage = get_codex_archive_storage(state.inner())?;
    let jsonl = export_archive_session_jsonl(storage.as_ref(), &archive_session_id)?;

    Ok(CodexArchiveExportPayload {
        archive_session_id,
        jsonl,
    })
}

#[tauri::command]
pub async fn materialize_codex_archive_session_files(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    archive_session_id: Option<String>,
) -> Result<CodexArchiveMaterializationResult, String> {
    let storage = get_codex_archive_storage(state.inner())?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    let output_paths = rebuild_materialized_archive_session_files(
        storage.as_ref(),
        app_data_dir.as_path(),
        archive_session_id.as_deref(),
    )?
    .into_iter()
    .map(|path| path.display().to_string())
    .collect::<Vec<_>>();

    Ok(CodexArchiveMaterializationResult {
        exported_count: output_paths.len(),
        output_paths,
    })
}

/// 手动刷新日志缓冲区到数据库
#[tauri::command]
pub async fn flush_codex_logs(state: State<'_, AppState>) -> Result<(), String> {
    let storage = state.codex_log_storage.lock().unwrap().clone();
    if let Some(s) = storage {
        s.flush().await;
        Ok(())
    } else {
        Err("Codex log storage not initialized".to_string())
    }
}

/// 全时间统计

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexAllTimeStats {
    pub requests: u64,
    pub tokens: u64,
}

/// 获取全时间累计统计
#[tauri::command]
pub async fn get_codex_all_time_stats(
    state: State<'_, AppState>,
) -> Result<CodexAllTimeStats, String> {
    let storage = state.codex_log_storage.lock().unwrap().clone();
    if let Some(s) = storage {
        let (requests, tokens) = s.get_all_time_stats()?;
        Ok(CodexAllTimeStats { requests, tokens })
    } else {
        Ok(CodexAllTimeStats {
            requests: 0,
            tokens: 0,
        })
    }
}

fn get_codex_archive_storage(
    state: &AppState,
) -> Result<Arc<crate::platforms::openai::codex::archive_storage::CodexArchiveStorage>, String> {
    state
        .codex_archive_storage
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Codex archive storage not initialized".to_string())
}

fn codex_archive_session_entry(session: ArchiveSessionRow) -> CodexArchiveSessionEntry {
    CodexArchiveSessionEntry {
        archive_session_id: session.archive_session_id,
        gateway_profile_id: session.gateway_profile_id,
        gateway_profile_name: session.gateway_profile_name,
        member_code: session.member_code,
        display_label: session.display_label,
        prompt_cache_key: session.prompt_cache_key,
        explicit_session_id: session.explicit_session_id,
        confidence: archive_confidence_label(session.confidence).to_string(),
        source: session.source,
        originator: session.originator,
        client_user_agent: session.client_user_agent,
        first_seen_at: session.first_seen_at,
        last_seen_at: session.last_seen_at,
        turn_count: session.turn_count,
    }
}

fn archive_confidence_label(confidence: ArchiveSessionConfidence) -> &'static str {
    match confidence {
        ArchiveSessionConfidence::High => "high",
        ArchiveSessionConfidence::Medium => "medium",
        ArchiveSessionConfidence::Low => "low",
    }
}

// ==================== 定时任务 ====================

async fn start_periodic_quota_refresh(
    app: tauri::AppHandle,
    state: &AppState,
    interval_seconds: u64,
) {
    stop_periodic_quota_refresh().await;

    let pool = state.codex_pool.lock().unwrap().clone();
    let Some(pool_ref) = pool else {
        return;
    };

    let tick_seconds = interval_seconds.max(1);
    let handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(tick_seconds));
        interval.tick().await;
        loop {
            interval.tick().await;
            println!("[Codex] Starting periodic quota refresh...");

            let accounts =
                match crate::platforms::openai::modules::storage::list_accounts(&app).await {
                    Ok(accs) => accs,
                    Err(e) => {
                        eprintln!("[Codex] Failed to list accounts for quota refresh: {}", e);
                        continue;
                    }
                };

            let mut refreshed = 0;
            for mut account in accounts {
                if account.account_type
                    == crate::platforms::openai::models::account::AccountType::API
                {
                    continue;
                }
                if account
                    .quota
                    .as_ref()
                    .map(|q| q.is_forbidden)
                    .unwrap_or(false)
                {
                    continue;
                }

                match crate::platforms::openai::modules::account::refresh_quota_and_backfill(
                    &mut account,
                )
                .await
                {
                    Ok(_) => {
                        if let Err(e) =
                            crate::platforms::openai::modules::storage::save_account(&app, &account)
                                .await
                        {
                            eprintln!("[Codex] Failed to save account {}: {}", account.email, e);
                        } else {
                            refreshed += 1;
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "[Codex] Failed to refresh quota for {}: {}",
                            account.email, e
                        );
                    }
                }
            }

            if let Ok(accounts) =
                crate::platforms::openai::modules::storage::list_accounts(&app).await
            {
                pool_ref.refresh_from_accounts(&accounts).await;
            }

            println!(
                "[Codex] Periodic quota refresh completed: {} accounts refreshed",
                refreshed
            );
        }
    });

    *QUOTA_REFRESH_TASK.lock().await = Some(handle);
}

async fn stop_periodic_quota_refresh() {
    let mut task = QUOTA_REFRESH_TASK.lock().await;
    if let Some(handle) = task.take() {
        handle.abort();
        println!("[Codex] Periodic quota refresh task stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gateway_access::{GatewayAccessProfile, GatewayAccessProfiles, GatewayTarget};
    use crate::platforms::openai::codex::pool::{CodexRelayHealthSnapshot, CodexRelayLayerHealth};
    use crate::platforms::openai::codex::team_profiles::{
        generate_team_gateway_api_key, import_team_template_into_profiles,
    };

    #[test]
    fn codex_access_server_url_uses_unified_v1_base_url() {
        assert_eq!(gateway_server_url(9123), "http://127.0.0.1:9123/v1");
    }

    #[test]
    fn codex_relay_health_config_defaults_normalize_to_ten_minutes() {
        let mut config = CodexServerConfig::default();
        config.relay.health_check_interval_seconds = 30;
        config.relay.auto_repair_cooldown_seconds = 0;

        normalize_runtime_fields(&mut config);

        assert_eq!(config.relay.health_check_interval_seconds, 600);
        assert_eq!(config.relay.auto_repair_cooldown_seconds, 600);
    }

    #[test]
    fn codex_relay_health_config_deserializes_when_legacy_relay_fields_are_missing() {
        let legacy = r#"{
          "port": 8766,
          "enabled": true,
          "pool_strategy": "round-robin",
          "selected_account_id": null,
          "api_key": "sk-legacy",
          "quota_refresh_enabled": true,
          "quota_refresh_interval_seconds": 1800,
          "fast_mode_enabled": false
        }"#;

        let mut config: CodexServerConfig = serde_json::from_str(legacy).unwrap();
        normalize_runtime_fields(&mut config);

        assert_eq!(config.relay.public_base_url, "https://lingkong.xyz/v1");
        assert_eq!(config.relay.health_check_interval_seconds, 600);
        assert_eq!(config.relay.auto_repair_cooldown_seconds, 600);
    }

    #[test]
    fn codex_relay_health_config_start_merge_preserves_existing_relay_settings() {
        let mut incoming = CodexServerConfig::default();
        let mut existing = CodexServerConfig::default();

        existing.relay.public_base_url = "https://relay.example.com/v1".into();
        existing.relay.host = Some("ubuntu@example.com".into());
        existing.relay.remote_port = 29090;
        existing.relay.local_port = 9766;
        existing.relay.control_socket = Some("/tmp/atm-relay.sock".into());
        existing.relay.health_check_interval_seconds = 1800;
        existing.relay.auto_repair_enabled = false;
        existing.relay.auto_repair_cooldown_seconds = 2400;

        merge_start_codex_server_config(&mut incoming, &existing);

        assert_eq!(incoming.relay, existing.relay);
    }

    #[test]
    fn codex_relay_health_config_trims_public_base_url_before_validation() {
        let mut config = CodexServerConfig::default();
        config.relay.public_base_url = "  https://relay.example.com/v1  ".into();

        normalize_runtime_fields(&mut config);

        assert_eq!(config.relay.public_base_url, "https://relay.example.com/v1");
    }

    #[test]
    fn codex_relay_health_config_aggregate_state_maps_expected_statuses() {
        let mut status = CodexRelayHealthSnapshot::default();

        status.local = CodexRelayLayerHealth {
            healthy: false,
            last_checked_at: Some(1),
            ..Default::default()
        };
        status.public = CodexRelayLayerHealth {
            healthy: false,
            last_checked_at: Some(1),
            ..Default::default()
        };
        status.refresh_overall();
        assert_eq!(status.overall.state, "local_down");

        status.local = CodexRelayLayerHealth {
            healthy: true,
            last_checked_at: Some(2),
            ..Default::default()
        };
        status.public = CodexRelayLayerHealth {
            healthy: false,
            last_checked_at: Some(2),
            ..Default::default()
        };
        status.refresh_overall();
        assert_eq!(status.overall.state, "public_down");

        status.public = CodexRelayLayerHealth {
            healthy: true,
            last_checked_at: Some(3),
            ..Default::default()
        };
        status.refresh_overall();
        assert_eq!(status.overall.state, "healthy");

        status.public = CodexRelayLayerHealth {
            healthy: false,
            last_checked_at: Some(4),
            repair_in_progress: true,
            ..Default::default()
        };
        status.refresh_overall();
        assert_eq!(status.overall.state, "in_progress");
    }

    #[test]
    fn codex_access_compat_sync_updates_legacy_profile_without_dropping_other_keys() {
        let profiles = GatewayAccessProfiles {
            profiles: vec![
                GatewayAccessProfile {
                    id: "codex-default".into(),
                    name: "Codex Default".into(),
                    target: GatewayTarget::Codex,
                    api_key: "sk-old-default".into(),
                    enabled: true,
                    member_code: None,
                    role_title: None,
                    persona_summary: None,
                    color: None,
                    notes: None,
                },
                GatewayAccessProfile {
                    id: "codex-user-a".into(),
                    name: "Alice".into(),
                    target: GatewayTarget::Codex,
                    api_key: "sk-alice".into(),
                    enabled: true,
                    member_code: None,
                    role_title: None,
                    persona_summary: None,
                    color: None,
                    notes: None,
                },
                GatewayAccessProfile {
                    id: "augment-default".into(),
                    name: "Augment Default".into(),
                    target: GatewayTarget::Augment,
                    api_key: "sk-augment".into(),
                    enabled: true,
                    member_code: None,
                    role_title: None,
                    persona_summary: None,
                    color: None,
                    notes: None,
                },
            ],
        };

        let updated = sync_legacy_codex_access_profile(profiles, Some("sk-new-default".into()));

        let codex_profiles: Vec<_> = updated
            .profiles
            .iter()
            .filter(|profile| profile.target == GatewayTarget::Codex)
            .cloned()
            .collect();

        assert_eq!(codex_profiles.len(), 2);
        assert_eq!(codex_profiles[0].id, "codex-default");
        assert_eq!(codex_profiles[0].api_key, "sk-new-default");
        assert_eq!(codex_profiles[1].id, "codex-user-a");
        assert_eq!(
            updated.first_enabled_api_key_for_target(GatewayTarget::Codex),
            Some("sk-new-default".to_string())
        );
    }

    #[test]
    fn generate_team_gateway_api_key_uses_member_code_prefix() {
        let key = generate_team_gateway_api_key("jdd");

        assert!(key.starts_with("sk-team-jdd-"));
        assert_eq!(key.len(), "sk-team-jdd-".len() + 8);
    }

    #[test]
    fn generate_team_gateway_api_key_normalizes_member_code() {
        let key = generate_team_gateway_api_key(" J/D D ");

        assert!(key.starts_with("sk-team-jdd-"));
        assert_eq!(key.len(), "sk-team-jdd-".len() + 8);
    }

    #[test]
    fn import_team_template_into_profiles_is_idempotent_by_member_code() {
        let imported = import_team_template_into_profiles(GatewayAccessProfiles::default());
        let imported = import_team_template_into_profiles(imported);
        let codex_profiles = imported.list_by_target(GatewayTarget::Codex);

        assert_eq!(codex_profiles.len(), 10);
        assert_eq!(
            codex_profiles
                .iter()
                .filter(|profile| profile.member_code.as_deref() == Some("jdd"))
                .count(),
            1
        );

        let jdd = codex_profiles
            .iter()
            .find(|profile| profile.member_code.as_deref() == Some("jdd"))
            .unwrap();
        assert_eq!(jdd.name, "姜大大");
        assert_eq!(jdd.role_title.as_deref(), Some("产品与方法论"));
        assert!(jdd.api_key.starts_with("sk-team-jdd-"));
    }

    #[test]
    fn codex_team_import_creates_ten_profiles_without_duplicates() {
        let imported = import_codex_team_template_profiles(GatewayAccessProfiles::default());
        let imported = import_codex_team_template_profiles(imported);
        let codex_profiles = imported.list_by_target(GatewayTarget::Codex);

        assert_eq!(codex_profiles.len(), 10);
        assert_eq!(
            codex_profiles
                .iter()
                .filter(|profile| profile.member_code.as_deref() == Some("jdd"))
                .count(),
            1
        );
        assert_eq!(
            codex_profiles
                .iter()
                .filter(|profile| profile.member_code.as_deref() == Some("jqw"))
                .count(),
            1
        );
    }

    #[test]
    fn create_codex_gateway_profile_preserves_member_metadata_and_uses_team_key() {
        let mut profiles = GatewayAccessProfiles::default();

        let created = create_codex_gateway_profile_in_profiles(
            &mut profiles,
            CodexGatewayProfileMutation {
                name: Some("姜大大".into()),
                api_key: None,
                enabled: Some(true),
                member_code: Some(" J/D D ".into()),
                role_title: Some("产品与方法论".into()),
                persona_summary: Some("高频输出".into()),
                color: Some("#4c6ef5".into()),
                notes: Some("高频使用成员".into()),
            },
        )
        .unwrap();

        assert_eq!(created.name, "姜大大");
        assert_eq!(created.member_code.as_deref(), Some("jdd"));
        assert_eq!(created.role_title.as_deref(), Some("产品与方法论"));
        assert_eq!(created.persona_summary.as_deref(), Some("高频输出"));
        assert_eq!(created.color.as_deref(), Some("#4c6ef5"));
        assert_eq!(created.notes.as_deref(), Some("高频使用成员"));
        assert!(created.api_key.starts_with("sk-team-jdd-"));

        let listed = codex_gateway_profiles(&profiles);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].member_code.as_deref(), Some("jdd"));
        assert_eq!(listed[0].role_title.as_deref(), Some("产品与方法论"));
        assert_eq!(listed[0].persona_summary.as_deref(), Some("高频输出"));
        assert_eq!(listed[0].color.as_deref(), Some("#4c6ef5"));
        assert_eq!(listed[0].notes.as_deref(), Some("高频使用成员"));
    }

    #[test]
    fn create_codex_gateway_profile_rejects_duplicate_member_code() {
        let mut profiles = import_codex_team_template_profiles(GatewayAccessProfiles::default());

        let error = create_codex_gateway_profile_in_profiles(
            &mut profiles,
            CodexGatewayProfileMutation {
                name: Some("重复姜大大".into()),
                api_key: None,
                enabled: Some(true),
                member_code: Some("JDD".into()),
                role_title: None,
                persona_summary: None,
                color: None,
                notes: None,
            },
        )
        .unwrap_err();

        assert!(error.contains("member code"));
    }

    #[test]
    fn regenerate_codex_gateway_profile_api_key_preserves_member_identity() {
        let mut profiles = GatewayAccessProfiles {
            profiles: vec![GatewayAccessProfile {
                id: "codex-jdd".into(),
                name: "姜大大".into(),
                target: GatewayTarget::Codex,
                api_key: "sk-team-jdd-oldkey01".into(),
                enabled: true,
                member_code: Some("jdd".into()),
                role_title: Some("产品与方法论".into()),
                persona_summary: Some("高频输出".into()),
                color: Some("#4c6ef5".into()),
                notes: Some("keep".into()),
            }],
        };

        let regenerated =
            regenerate_codex_gateway_profile_api_key_in_profiles(&mut profiles, "codex-jdd")
                .unwrap();

        assert_eq!(regenerated.name, "姜大大");
        assert_eq!(regenerated.member_code.as_deref(), Some("jdd"));
        assert_eq!(regenerated.notes.as_deref(), Some("keep"));
        assert!(regenerated.api_key.starts_with("sk-team-jdd-"));
        assert_ne!(regenerated.api_key, "sk-team-jdd-oldkey01");
    }

    #[test]
    fn reset_codex_gateway_profile_to_team_defaults_restores_preset_metadata() {
        let mut profiles = GatewayAccessProfiles {
            profiles: vec![GatewayAccessProfile {
                id: "codex-jdd".into(),
                name: "临时名称".into(),
                target: GatewayTarget::Codex,
                api_key: "sk-team-jdd-existing".into(),
                enabled: false,
                member_code: Some("jdd".into()),
                role_title: Some("临时角色".into()),
                persona_summary: Some("临时简介".into()),
                color: Some("#000000".into()),
                notes: Some("临时备注".into()),
            }],
        };

        let reset =
            reset_codex_gateway_profile_to_team_defaults_in_profiles(&mut profiles, "codex-jdd")
                .unwrap();

        assert_eq!(reset.name, "姜大大");
        assert_eq!(reset.member_code.as_deref(), Some("jdd"));
        assert_eq!(reset.role_title.as_deref(), Some("产品与方法论"));
        assert_eq!(
            reset.persona_summary.as_deref(),
            Some("高频输出，偏产品与方法论视角，擅长比较工具优劣并推动落地。")
        );
        assert_eq!(reset.color.as_deref(), Some("#4c6ef5"));
        assert_eq!(reset.notes, None);
        assert!(reset.enabled);
        assert_eq!(reset.api_key, "sk-team-jdd-existing");
    }

    #[test]
    fn update_codex_gateway_profile_rejects_duplicate_member_code() {
        let mut profiles = import_codex_team_template_profiles(GatewayAccessProfiles::default());
        profiles.upsert_profile(GatewayAccessProfile {
            id: "codex-custom".into(),
            name: "Custom".into(),
            target: GatewayTarget::Codex,
            api_key: "sk-custom".into(),
            enabled: true,
            member_code: Some("custom".into()),
            role_title: None,
            persona_summary: None,
            color: None,
            notes: None,
        });

        let error = update_codex_gateway_profile_in_profiles(
            &mut profiles,
            "codex-custom",
            CodexGatewayProfileMutation {
                name: None,
                api_key: None,
                enabled: None,
                member_code: Some(" j/d d ".into()),
                role_title: None,
                persona_summary: None,
                color: None,
                notes: None,
            },
        )
        .unwrap_err();

        assert!(error.contains("member code"));
    }

    #[test]
    fn update_codex_gateway_profile_allows_editing_imported_team_member() {
        let mut profiles = import_codex_team_template_profiles(GatewayAccessProfiles::default());
        let jdd = profiles
            .list_by_target(GatewayTarget::Codex)
            .into_iter()
            .find(|profile| profile.member_code.as_deref() == Some("jdd"))
            .unwrap();

        let updated = update_codex_gateway_profile_in_profiles(
            &mut profiles,
            &jdd.id,
            CodexGatewayProfileMutation {
                name: Some("姜大大 Pro".into()),
                api_key: None,
                enabled: Some(false),
                member_code: Some("mentor".into()),
                role_title: Some("顾问".into()),
                persona_summary: Some("转为顾问角色".into()),
                color: Some("#112233".into()),
                notes: Some("允许自由编辑".into()),
            },
        )
        .unwrap();

        assert_eq!(updated.name, "姜大大 Pro");
        assert!(!updated.enabled);
        assert_eq!(updated.member_code.as_deref(), Some("mentor"));
        assert_eq!(updated.role_title.as_deref(), Some("顾问"));
        assert_eq!(updated.persona_summary.as_deref(), Some("转为顾问角色"));
        assert_eq!(updated.color.as_deref(), Some("#112233"));
        assert_eq!(updated.notes.as_deref(), Some("允许自由编辑"));
    }

    #[test]
    fn delete_codex_gateway_profile_allows_removing_imported_team_member() {
        let mut profiles = import_codex_team_template_profiles(GatewayAccessProfiles::default());
        let jdd = profiles
            .list_by_target(GatewayTarget::Codex)
            .into_iter()
            .find(|profile| profile.member_code.as_deref() == Some("jdd"))
            .unwrap();

        delete_codex_gateway_profile_in_profiles(&mut profiles, &jdd.id).unwrap();

        let codex_profiles = profiles.list_by_target(GatewayTarget::Codex);
        assert_eq!(codex_profiles.len(), 9);
        assert!(!codex_profiles.iter().any(|profile| profile.id == jdd.id));
    }

    #[test]
    fn delete_codex_gateway_profile_allows_removing_noncanonical_duplicate_team_member_code() {
        let mut profiles = GatewayAccessProfiles {
            profiles: vec![
                GatewayAccessProfile {
                    id: "codex-jdd".into(),
                    name: "姜大大".into(),
                    target: GatewayTarget::Codex,
                    api_key: "sk-team-jdd-a".into(),
                    enabled: true,
                    member_code: Some("jdd".into()),
                    role_title: Some("产品与方法论".into()),
                    persona_summary: None,
                    color: None,
                    notes: None,
                },
                GatewayAccessProfile {
                    id: "codex-dup-jdd".into(),
                    name: "姜大大副本".into(),
                    target: GatewayTarget::Codex,
                    api_key: "sk-team-jdd-b".into(),
                    enabled: true,
                    member_code: Some("jdd".into()),
                    role_title: None,
                    persona_summary: None,
                    color: None,
                    notes: None,
                },
            ],
        };

        delete_codex_gateway_profile_in_profiles(&mut profiles, "codex-dup-jdd").unwrap();

        let codex_profiles = profiles.list_by_target(GatewayTarget::Codex);
        assert_eq!(codex_profiles.len(), 1);
        assert_eq!(codex_profiles[0].id, "codex-jdd");
    }
}
