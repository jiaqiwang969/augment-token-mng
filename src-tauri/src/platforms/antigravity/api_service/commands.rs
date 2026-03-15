use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri::State;
use uuid::Uuid;

use super::logger::AntigravityLogStorage;
use super::models::{
    AntigravityGatewayProfileEntry, AntigravityGatewayProfileMutation, DailyStatsResponse,
    GatewayDailyStatsResponse, LogPage, LogQuery, LogSummary, ModelTokenStats, PeriodTokenStats,
};
use super::team_profiles::{
    generate_antigravity_gateway_api_key, import_antigravity_team_template_into_profiles,
};
use crate::AppState;
use crate::core::gateway_access::{GatewayAccessProfile, GatewayAccessProfiles, GatewayTarget};
use crate::platforms::openai::codex::team_profiles::normalize_member_code;

const DEFAULT_SHARED_API_SERVER_PORT: u16 = 8766;
const DEFAULT_PUBLIC_GATEWAY_BASE_URL: &str = "https://lingkong.xyz/v1";
const ANTIGRAVITY_GATEWAY_PROFILE_NAME_PREFIX: &str = "Antigravity Key";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityAccessConfig {
    pub server_url: String,
    pub public_server_url: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityApiServiceStatus {
    pub api_server_running: bool,
    pub api_server_address: Option<String>,
    pub server_url: String,
    pub public_server_url: String,
    pub sidecar_configured: bool,
    pub sidecar_running: bool,
    pub sidecar_healthy: bool,
    pub total_accounts: usize,
    pub available_accounts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityLogStorageStatus {
    pub total_logs: i64,
    pub db_size_bytes: u64,
    pub db_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntigravityAllTimeStats {
    pub requests: u64,
    pub tokens: u64,
}

fn api_server_address_for_port(port: u16) -> String {
    format!("http://127.0.0.1:{}", port)
}

fn gateway_server_url(port: u16) -> String {
    format!("{}/v1", api_server_address_for_port(port))
}

fn public_gateway_server_url() -> String {
    std::env::var("ATM_RELAY_PUBLIC_BASE_URL")
        .ok()
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .unwrap_or_else(|| DEFAULT_PUBLIC_GATEWAY_BASE_URL.to_string())
}

fn current_api_server_port(state: &AppState) -> Option<u16> {
    let guard = state.api_server.lock().unwrap();
    guard.as_ref().map(|server| server.get_port())
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

fn normalize_gateway_member_code(member_code: Option<String>) -> Option<String> {
    member_code.and_then(|value| normalize_member_code(&value))
}

fn default_antigravity_gateway_profile_name(profiles: &GatewayAccessProfiles) -> String {
    let next_index = profiles
        .list_by_target(GatewayTarget::Antigravity)
        .len()
        .saturating_add(1);
    format!("{} {}", ANTIGRAVITY_GATEWAY_PROFILE_NAME_PREFIX, next_index)
}

fn ensure_unique_antigravity_member_code(
    profiles: &GatewayAccessProfiles,
    member_code: Option<&str>,
    ignored_profile_id: Option<&str>,
) -> Result<Option<String>, String> {
    let normalized = member_code.and_then(normalize_member_code);
    let Some(expected) = normalized.clone() else {
        return Ok(None);
    };

    let has_conflict = profiles
        .list_by_target(GatewayTarget::Antigravity)
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
            "Antigravity gateway member code already exists: {}",
            expected
        ))
    } else {
        Ok(Some(expected))
    }
}

fn generate_gateway_api_key_for_member_code(member_code: Option<&str>) -> String {
    match member_code.and_then(normalize_member_code) {
        Some(member_code) => generate_antigravity_gateway_api_key(&member_code),
        None => generate_antigravity_gateway_api_key("member"),
    }
}

fn build_antigravity_access_config(
    server_url: String,
    public_server_url: String,
    api_key: Option<String>,
) -> AntigravityAccessConfig {
    AntigravityAccessConfig {
        server_url,
        public_server_url,
        api_key: normalize_gateway_api_key(api_key),
    }
}

fn summarize_antigravity_api_service_status(
    api_server_port: Option<u16>,
    public_server_url: String,
    sidecar_configured: bool,
    sidecar_running: bool,
    sidecar_healthy: bool,
    total_accounts: usize,
    available_accounts: usize,
) -> AntigravityApiServiceStatus {
    let resolved_port = api_server_port.unwrap_or(DEFAULT_SHARED_API_SERVER_PORT);

    AntigravityApiServiceStatus {
        api_server_running: api_server_port.is_some(),
        api_server_address: api_server_port.map(api_server_address_for_port),
        server_url: gateway_server_url(resolved_port),
        public_server_url,
        sidecar_configured,
        sidecar_running,
        sidecar_healthy,
        total_accounts,
        available_accounts,
    }
}

fn antigravity_gateway_profile_entry(
    profile: GatewayAccessProfile,
    primary_profile_id: Option<&str>,
) -> AntigravityGatewayProfileEntry {
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

    AntigravityGatewayProfileEntry {
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

fn antigravity_gateway_profiles(
    profiles: &GatewayAccessProfiles,
) -> Vec<AntigravityGatewayProfileEntry> {
    let primary_profile_id = profiles
        .list_by_target(GatewayTarget::Antigravity)
        .into_iter()
        .find_map(|profile| {
            if profile.enabled && !profile.api_key.trim().is_empty() {
                Some(profile.id)
            } else {
                None
            }
        });

    profiles
        .list_by_target(GatewayTarget::Antigravity)
        .into_iter()
        .map(|profile| antigravity_gateway_profile_entry(profile, primary_profile_id.as_deref()))
        .collect()
}

fn antigravity_gateway_profile_entry_by_id(
    profiles: &GatewayAccessProfiles,
    profile_id: &str,
) -> Result<AntigravityGatewayProfileEntry, String> {
    antigravity_gateway_profiles(profiles)
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("Antigravity gateway profile not found: {}", profile_id))
}

fn antigravity_gateway_profile_by_id(
    profiles: &GatewayAccessProfiles,
    profile_id: &str,
) -> Result<GatewayAccessProfile, String> {
    profiles
        .list_by_target(GatewayTarget::Antigravity)
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("Antigravity gateway profile not found: {}", profile_id))
}

fn import_antigravity_team_template_profiles(
    profiles: GatewayAccessProfiles,
) -> GatewayAccessProfiles {
    import_antigravity_team_template_into_profiles(profiles)
}

pub(crate) fn create_antigravity_gateway_profile_in_profiles(
    profiles: &mut GatewayAccessProfiles,
    mutation: AntigravityGatewayProfileMutation,
) -> Result<GatewayAccessProfile, String> {
    let member_code =
        ensure_unique_antigravity_member_code(profiles, mutation.member_code.as_deref(), None)?;
    let entry = GatewayAccessProfile {
        id: format!("antigravity-{}", Uuid::new_v4().simple()),
        name: normalize_gateway_profile_name(mutation.name)
            .unwrap_or_else(|| default_antigravity_gateway_profile_name(profiles)),
        target: GatewayTarget::Antigravity,
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

fn update_antigravity_gateway_profile_in_profiles(
    profiles: &mut GatewayAccessProfiles,
    profile_id: &str,
    mutation: AntigravityGatewayProfileMutation,
) -> Result<GatewayAccessProfile, String> {
    let mut existing = antigravity_gateway_profile_by_id(profiles, profile_id)?;
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
            .ok_or_else(|| "Antigravity gateway API key cannot be empty".to_string())?;
    }

    if let Some(enabled) = mutation.enabled {
        existing.enabled = enabled;
    }

    if let Some(member_code_input) = mutation.member_code {
        let normalized_member_code = normalize_gateway_member_code(Some(member_code_input));
        if normalized_member_code != current_member_code {
            ensure_unique_antigravity_member_code(
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

    antigravity_gateway_profile_by_id(profiles, &updated_id)
}

fn regenerate_antigravity_gateway_profile_api_key_in_profiles(
    profiles: &mut GatewayAccessProfiles,
    profile_id: &str,
) -> Result<GatewayAccessProfile, String> {
    let mut existing = antigravity_gateway_profile_by_id(profiles, profile_id)?;
    existing.api_key = generate_gateway_api_key_for_member_code(existing.member_code.as_deref());
    let updated_id = existing.id.clone();
    profiles.upsert_profile(existing);

    antigravity_gateway_profile_by_id(profiles, &updated_id)
}

fn delete_antigravity_gateway_profile_in_profiles(
    profiles: &mut GatewayAccessProfiles,
    profile_id: &str,
) -> Result<(), String> {
    antigravity_gateway_profile_by_id(profiles, profile_id)?;

    if profiles.remove_profile(profile_id) {
        Ok(())
    } else {
        Err(format!(
            "Antigravity gateway profile not found: {}",
            profile_id
        ))
    }
}

fn build_antigravity_access_bundle_text(
    public_base_url: &str,
    profiles: &[AntigravityGatewayProfileEntry],
) -> String {
    let base_url = public_base_url.trim();

    profiles
        .iter()
        .filter_map(|profile| {
            let api_key = profile.api_key.trim();
            if api_key.is_empty() {
                return None;
            }

            let mut label_parts = Vec::new();
            if !profile.name.trim().is_empty() {
                label_parts.push(profile.name.trim().to_string());
            }
            if let Some(member_code) = profile
                .member_code
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                label_parts.push(member_code.to_string());
            }
            let label = if label_parts.is_empty() {
                profile.id.clone()
            } else {
                label_parts.join(" · ")
            };

            Some(format!(
                "# {label}\nANTIGRAVITY_BASE_URL={base_url}\nANTIGRAVITY_API_KEY={api_key}"
            ))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn gateway_api_key_for_target(
    app: &tauri::AppHandle,
    state: &AppState,
    target: GatewayTarget,
) -> Result<Option<String>, String> {
    let profiles = crate::core::gateway_access::get_or_load_gateway_access_profiles(app, state)?;
    Ok(profiles.first_enabled_api_key_for_target(target))
}

async fn load_antigravity_accounts(
    state: &AppState,
) -> Result<Vec<crate::platforms::antigravity::models::Account>, String> {
    use crate::data::storage::common::traits::AccountStorage;

    let storage = {
        let guard = state.antigravity_storage_manager.lock().unwrap();
        guard.as_ref().cloned()
    };

    let Some(storage) = storage else {
        return Ok(Vec::new());
    };

    storage
        .load_accounts()
        .await
        .map_err(|e| format!("Failed to load Antigravity accounts: {}", e))
}

fn is_antigravity_account_usable(account: &crate::platforms::antigravity::models::Account) -> bool {
    !account.token.access_token.trim().is_empty()
        && !account.disabled
        && !account.deleted
        && !account
            .quota
            .as_ref()
            .is_some_and(|quota| quota.is_forbidden)
}

fn antigravity_log_storage_for_data_dir(data_dir: PathBuf) -> Result<AntigravityLogStorage, String> {
    AntigravityLogStorage::new(data_dir)
}

fn antigravity_log_storage_from_app(
    app: &tauri::AppHandle,
) -> Result<AntigravityLogStorage, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    antigravity_log_storage_for_data_dir(data_dir)
}

fn query_antigravity_logs_from_storage_with(
    storage: Option<&AntigravityLogStorage>,
    query: LogQuery,
) -> Result<LogPage, String> {
    if let Some(storage) = storage {
        storage.query_logs(&query)
    } else {
        Ok(LogPage {
            total: 0,
            items: vec![],
        })
    }
}

fn get_antigravity_model_stats_from_storage_with(
    storage: Option<&AntigravityLogStorage>,
    start_ts: i64,
    end_ts: i64,
) -> Result<Vec<ModelTokenStats>, String> {
    if let Some(storage) = storage {
        storage.get_model_stats(start_ts, end_ts)
    } else {
        Ok(vec![])
    }
}

fn get_antigravity_log_summary_from_storage_with(
    storage: Option<&AntigravityLogStorage>,
    query: LogQuery,
) -> Result<LogSummary, String> {
    if let Some(storage) = storage {
        storage.get_log_summary(&query)
    } else {
        Ok(LogSummary {
            total_requests: 0,
            success_requests: 0,
            error_requests: 0,
            total_tokens: 0,
            success_rate: 0.0,
        })
    }
}

fn get_antigravity_period_stats_from_storage_with(
    storage: Option<&AntigravityLogStorage>,
) -> Result<PeriodTokenStats, String> {
    if let Some(storage) = storage {
        storage.get_period_stats(chrono::Utc::now().timestamp())
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

fn get_antigravity_daily_stats_from_storage_with(
    storage: Option<&AntigravityLogStorage>,
    days: Option<u32>,
) -> Result<DailyStatsResponse, String> {
    if let Some(storage) = storage {
        storage.get_daily_stats(days.unwrap_or(30).min(365))
    } else {
        Ok(DailyStatsResponse { stats: vec![] })
    }
}

fn get_antigravity_daily_stats_by_gateway_profile_from_storage_with(
    storage: Option<&AntigravityLogStorage>,
    days: Option<u32>,
) -> Result<GatewayDailyStatsResponse, String> {
    if let Some(storage) = storage {
        storage.get_daily_stats_by_gateway_profile(days.unwrap_or(30).min(365))
    } else {
        Ok(GatewayDailyStatsResponse { series: vec![] })
    }
}

fn clear_antigravity_logs_in_storage_with(
    storage: Option<&AntigravityLogStorage>,
) -> Result<usize, String> {
    if let Some(storage) = storage {
        storage.clear_all()
    } else {
        Ok(0)
    }
}

fn delete_antigravity_logs_before_with(
    storage: Option<&AntigravityLogStorage>,
    date_key: i64,
) -> Result<usize, String> {
    if let Some(storage) = storage {
        storage.delete_before(date_key)
    } else {
        Ok(0)
    }
}

fn get_antigravity_log_storage_status_with(
    storage: Option<&AntigravityLogStorage>,
) -> Result<AntigravityLogStorageStatus, String> {
    let Some(storage) = storage else {
        return Err("Antigravity log storage not initialized".to_string());
    };

    Ok(AntigravityLogStorageStatus {
        total_logs: storage.total_logs()?,
        db_size_bytes: storage.db_size()?,
        db_path: storage.db_path().display().to_string(),
    })
}

fn get_antigravity_all_time_stats_with(
    storage: Option<&AntigravityLogStorage>,
) -> Result<AntigravityAllTimeStats, String> {
    if let Some(storage) = storage {
        let (requests, tokens) = storage.get_all_time_stats()?;
        Ok(AntigravityAllTimeStats { requests, tokens })
    } else {
        Ok(AntigravityAllTimeStats {
            requests: 0,
            tokens: 0,
        })
    }
}

#[tauri::command]
pub async fn get_antigravity_api_service_status(
    state: State<'_, AppState>,
) -> Result<AntigravityApiServiceStatus, String> {
    let api_server_port = current_api_server_port(state.inner());
    let public_server_url = public_gateway_server_url();
    let accounts = load_antigravity_accounts(state.inner()).await?;
    let available_accounts = accounts
        .iter()
        .filter(|account| is_antigravity_account_usable(account))
        .count();

    let (sidecar_configured, sidecar_running, sidecar_healthy) = {
        let guard = state.antigravity_sidecar.lock().await;
        match guard.as_ref() {
            Some(sidecar) => (true, sidecar.is_running(), sidecar.is_healthy().await),
            None => (false, false, false),
        }
    };

    Ok(summarize_antigravity_api_service_status(
        api_server_port,
        public_server_url,
        sidecar_configured,
        sidecar_running,
        sidecar_healthy,
        accounts.len(),
        available_accounts,
    ))
}

#[tauri::command]
pub async fn get_antigravity_access_config(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<AntigravityAccessConfig, String> {
    Ok(build_antigravity_access_config(
        gateway_server_url(
            current_api_server_port(state.inner()).unwrap_or(DEFAULT_SHARED_API_SERVER_PORT),
        ),
        public_gateway_server_url(),
        gateway_api_key_for_target(&app, state.inner(), GatewayTarget::Antigravity)?,
    ))
}

#[tauri::command]
pub async fn list_antigravity_gateway_profiles(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<AntigravityGatewayProfileEntry>, String> {
    let profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;
    Ok(antigravity_gateway_profiles(&profiles))
}

#[tauri::command]
pub async fn create_antigravity_gateway_profile(
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
) -> Result<AntigravityGatewayProfileEntry, String> {
    let mut profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;

    let entry = create_antigravity_gateway_profile_in_profiles(
        &mut profiles,
        AntigravityGatewayProfileMutation {
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
    antigravity_gateway_profile_entry_by_id(&profiles, &created_id)
        .map_err(|_| "Failed to load created Antigravity gateway profile".to_string())
}

#[tauri::command]
pub async fn update_antigravity_gateway_profile(
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
) -> Result<AntigravityGatewayProfileEntry, String> {
    let mut profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;
    let updated = update_antigravity_gateway_profile_in_profiles(
        &mut profiles,
        &profile_id,
        AntigravityGatewayProfileMutation {
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
    antigravity_gateway_profile_entry_by_id(&profiles, &updated_id)
        .map_err(|_| "Failed to load updated Antigravity gateway profile".to_string())
}

#[tauri::command]
pub async fn import_antigravity_team_template(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<AntigravityGatewayProfileEntry>, String> {
    let profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;
    let profiles = import_antigravity_team_template_profiles(profiles);
    let profiles =
        crate::core::gateway_access::set_gateway_access_profiles(&app, state.inner(), profiles)?;
    Ok(antigravity_gateway_profiles(&profiles))
}

#[tauri::command]
pub async fn regenerate_antigravity_gateway_profile_api_key(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AntigravityGatewayProfileEntry, String> {
    let mut profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;
    let updated =
        regenerate_antigravity_gateway_profile_api_key_in_profiles(&mut profiles, &profile_id)?;
    let updated_id = updated.id.clone();

    let profiles =
        crate::core::gateway_access::set_gateway_access_profiles(&app, state.inner(), profiles)?;
    antigravity_gateway_profile_entry_by_id(&profiles, &updated_id)
        .map_err(|_| "Failed to load regenerated Antigravity gateway profile".to_string())
}

#[tauri::command]
pub async fn delete_antigravity_gateway_profile(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), String> {
    let mut profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;
    delete_antigravity_gateway_profile_in_profiles(&mut profiles, &profile_id)?;

    crate::core::gateway_access::set_gateway_access_profiles(&app, state.inner(), profiles)?;
    Ok(())
}

#[tauri::command]
pub async fn export_antigravity_access_bundle(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let profiles =
        crate::core::gateway_access::get_or_load_gateway_access_profiles(&app, state.inner())?;
    Ok(build_antigravity_access_bundle_text(
        &public_gateway_server_url(),
        &antigravity_gateway_profiles(&profiles),
    ))
}

#[tauri::command]
pub async fn query_antigravity_logs_from_storage(
    app: tauri::AppHandle,
    query: LogQuery,
) -> Result<LogPage, String> {
    let storage = antigravity_log_storage_from_app(&app)?;
    query_antigravity_logs_from_storage_with(Some(&storage), query)
}

#[tauri::command]
pub async fn get_antigravity_model_stats_from_storage(
    app: tauri::AppHandle,
    start_ts: i64,
    end_ts: i64,
) -> Result<Vec<ModelTokenStats>, String> {
    let storage = antigravity_log_storage_from_app(&app)?;
    get_antigravity_model_stats_from_storage_with(Some(&storage), start_ts, end_ts)
}

#[tauri::command]
pub async fn get_antigravity_log_summary_from_storage(
    app: tauri::AppHandle,
    query: LogQuery,
) -> Result<LogSummary, String> {
    let storage = antigravity_log_storage_from_app(&app)?;
    get_antigravity_log_summary_from_storage_with(Some(&storage), query)
}

#[tauri::command]
pub async fn get_antigravity_period_stats_from_storage(
    app: tauri::AppHandle,
) -> Result<PeriodTokenStats, String> {
    let storage = antigravity_log_storage_from_app(&app)?;
    get_antigravity_period_stats_from_storage_with(Some(&storage))
}

#[tauri::command]
pub async fn get_antigravity_daily_stats_from_storage(
    app: tauri::AppHandle,
    days: Option<u32>,
) -> Result<DailyStatsResponse, String> {
    let storage = antigravity_log_storage_from_app(&app)?;
    get_antigravity_daily_stats_from_storage_with(Some(&storage), days)
}

#[tauri::command]
pub async fn get_antigravity_daily_stats_by_gateway_profile_from_storage(
    app: tauri::AppHandle,
    days: Option<u32>,
) -> Result<GatewayDailyStatsResponse, String> {
    let storage = antigravity_log_storage_from_app(&app)?;
    get_antigravity_daily_stats_by_gateway_profile_from_storage_with(Some(&storage), days)
}

#[tauri::command]
pub async fn clear_antigravity_logs_in_storage(app: tauri::AppHandle) -> Result<usize, String> {
    let storage = antigravity_log_storage_from_app(&app)?;
    clear_antigravity_logs_in_storage_with(Some(&storage))
}

#[tauri::command]
pub async fn delete_antigravity_logs_before(
    app: tauri::AppHandle,
    date_key: i64,
) -> Result<usize, String> {
    let storage = antigravity_log_storage_from_app(&app)?;
    delete_antigravity_logs_before_with(Some(&storage), date_key)
}

#[tauri::command]
pub async fn get_antigravity_log_storage_status(
    app: tauri::AppHandle,
) -> Result<AntigravityLogStorageStatus, String> {
    let storage = antigravity_log_storage_from_app(&app)?;
    get_antigravity_log_storage_status_with(Some(&storage))
}

#[tauri::command]
pub async fn get_antigravity_all_time_stats(
    app: tauri::AppHandle,
) -> Result<AntigravityAllTimeStats, String> {
    let storage = antigravity_log_storage_from_app(&app)?;
    get_antigravity_all_time_stats_with(Some(&storage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::antigravity::api_service::logger::AntigravityLogStorage;
    use crate::platforms::antigravity::api_service::models::LogQuery;
    use crate::platforms::antigravity::api_service::team_profiles::import_antigravity_team_template_into_profiles;
    use tempfile::tempdir;

    fn build_request_log(
        id: &str,
        timestamp: i64,
        total_tokens: u64,
        gateway_profile_id: Option<&str>,
        gateway_profile_name: Option<&str>,
        member_code: Option<&str>,
    ) -> crate::platforms::antigravity::api_service::models::RequestLog {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "timestamp": timestamp,
            "account_id": "antigravity-sidecar",
            "account_email": "member@example.com",
            "model": "claude-sonnet-4-5",
            "format": "openai-responses",
            "input_tokens": total_tokens / 2,
            "output_tokens": total_tokens / 2,
            "total_tokens": total_tokens,
            "status": "success",
            "error_message": null,
            "request_duration_ms": 120,
            "gateway_profile_id": gateway_profile_id,
            "gateway_profile_name": gateway_profile_name,
            "member_code": member_code,
            "role_title": null,
            "display_label": null,
            "api_key_suffix": "12345678",
            "color": "#4c6ef5"
        }))
        .unwrap()
    }

    #[test]
    fn create_antigravity_gateway_profile_preserves_member_metadata_and_uses_ant_key() {
        let mut profiles = GatewayAccessProfiles::default();

        let created = create_antigravity_gateway_profile_in_profiles(
            &mut profiles,
            AntigravityGatewayProfileMutation {
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

        assert_eq!(created.target, GatewayTarget::Antigravity);
        assert_eq!(created.member_code.as_deref(), Some("jdd"));
        assert_eq!(created.role_title.as_deref(), Some("产品与方法论"));
        assert_eq!(created.persona_summary.as_deref(), Some("高频输出"));
        assert_eq!(created.color.as_deref(), Some("#4c6ef5"));
        assert_eq!(created.notes.as_deref(), Some("高频使用成员"));
        assert!(created.api_key.starts_with("sk-ant-jdd-"));
    }

    #[test]
    fn create_antigravity_gateway_profile_rejects_duplicate_member_code_inside_antigravity_target()
    {
        let mut profiles =
            import_antigravity_team_template_into_profiles(GatewayAccessProfiles::default());

        let error = create_antigravity_gateway_profile_in_profiles(
            &mut profiles,
            AntigravityGatewayProfileMutation {
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
    fn create_antigravity_gateway_profile_allows_member_code_used_by_codex_target() {
        let mut profiles = GatewayAccessProfiles {
            profiles: vec![GatewayAccessProfile {
                id: "codex-jdd".into(),
                name: "姜大大".into(),
                target: GatewayTarget::Codex,
                api_key: "sk-team-jdd-existing".into(),
                enabled: true,
                member_code: Some("jdd".into()),
                role_title: None,
                persona_summary: None,
                color: None,
                notes: None,
            }],
        };

        let created = create_antigravity_gateway_profile_in_profiles(
            &mut profiles,
            AntigravityGatewayProfileMutation {
                name: Some("姜大大".into()),
                api_key: None,
                enabled: Some(true),
                member_code: Some("jdd".into()),
                role_title: None,
                persona_summary: None,
                color: None,
                notes: None,
            },
        )
        .unwrap();

        assert_eq!(created.member_code.as_deref(), Some("jdd"));
        assert_eq!(profiles.list_by_target(GatewayTarget::Codex).len(), 1);
        assert_eq!(profiles.list_by_target(GatewayTarget::Antigravity).len(), 1);
    }

    #[test]
    fn antigravity_access_config_uses_shared_v1_base_url() {
        let cfg = build_antigravity_access_config(
            "http://127.0.0.1:8766/v1".into(),
            "https://lingkong.xyz/v1".into(),
            Some("sk-ant-jdd-12345678".into()),
        );

        assert_eq!(cfg.server_url, "http://127.0.0.1:8766/v1");
        assert_eq!(cfg.public_server_url, "https://lingkong.xyz/v1");
        assert_eq!(cfg.api_key.as_deref(), Some("sk-ant-jdd-12345678"));
    }

    #[test]
    fn antigravity_service_status_uses_shared_gateway_urls() {
        let status = summarize_antigravity_api_service_status(
            Some(8766),
            "https://lingkong.xyz/v1".into(),
            true,
            true,
            true,
            10,
            8,
        );

        assert!(status.api_server_running);
        assert_eq!(
            status.api_server_address.as_deref(),
            Some("http://127.0.0.1:8766")
        );
        assert_eq!(status.server_url, "http://127.0.0.1:8766/v1");
        assert_eq!(status.public_server_url, "https://lingkong.xyz/v1");
        assert_eq!(status.total_accounts, 10);
        assert_eq!(status.available_accounts, 8);
    }

    #[test]
    fn antigravity_gateway_commands_support_update_regenerate_delete_and_export() {
        let mut profiles =
            import_antigravity_team_template_into_profiles(GatewayAccessProfiles::default());
        let created = create_antigravity_gateway_profile_in_profiles(
            &mut profiles,
            AntigravityGatewayProfileMutation {
                name: Some("临时成员".into()),
                api_key: None,
                enabled: Some(true),
                member_code: Some("tmp".into()),
                role_title: Some("实验验证".into()),
                persona_summary: None,
                color: Some("#ff922b".into()),
                notes: None,
            },
        )
        .unwrap();

        let updated = update_antigravity_gateway_profile_in_profiles(
            &mut profiles,
            &created.id,
            AntigravityGatewayProfileMutation {
                name: Some("临时成员-更新".into()),
                api_key: None,
                enabled: Some(false),
                member_code: Some("tmp".into()),
                role_title: Some("实战落地".into()),
                persona_summary: Some("偏执行".into()),
                color: Some("#f03e3e".into()),
                notes: Some("已停用".into()),
            },
        )
        .unwrap();
        let previous_api_key = updated.api_key.clone();

        let regenerated =
            regenerate_antigravity_gateway_profile_api_key_in_profiles(&mut profiles, &created.id)
                .unwrap();
        let export = build_antigravity_access_bundle_text(
            "https://lingkong.xyz/v1",
            &antigravity_gateway_profiles(&profiles),
        );

        assert_eq!(updated.name, "临时成员-更新");
        assert!(!updated.enabled);
        assert_eq!(updated.role_title.as_deref(), Some("实战落地"));
        assert_ne!(regenerated.api_key, previous_api_key);
        assert!(export.contains("ANTIGRAVITY_BASE_URL=https://lingkong.xyz/v1"));
        assert!(export.contains("ANTIGRAVITY_API_KEY="));
        assert!(export.contains(&regenerated.api_key));

        delete_antigravity_gateway_profile_in_profiles(&mut profiles, &created.id).unwrap();
        assert!(antigravity_gateway_profile_entry_by_id(&profiles, &created.id).is_err());
    }

    #[test]
    fn antigravity_storage_helpers_return_empty_defaults_without_storage() {
        let page = query_antigravity_logs_from_storage_with(
            None,
            LogQuery {
                limit: None,
                offset: None,
                start_ts: None,
                end_ts: None,
                model: None,
                format: None,
                status: None,
                account_id: None,
                member_code: None,
            },
        )
        .unwrap();
        let daily =
            get_antigravity_daily_stats_from_storage_with(None, Some(30)).unwrap();
        let by_profile =
            get_antigravity_daily_stats_by_gateway_profile_from_storage_with(None, Some(30))
                .unwrap();
        let all_time = get_antigravity_all_time_stats_with(None).unwrap();

        assert_eq!(page.total, 0);
        assert!(page.items.is_empty());
        assert!(daily.stats.is_empty());
        assert!(by_profile.series.is_empty());
        assert_eq!(all_time.requests, 0);
        assert_eq!(all_time.tokens, 0);
    }

    #[tokio::test]
    async fn antigravity_storage_helpers_return_grouped_gateway_daily_stats() {
        let temp_dir = tempdir().unwrap();
        let storage = AntigravityLogStorage::new(temp_dir.path().to_path_buf()).unwrap();
        let now = chrono::Utc::now().timestamp();
        let yesterday = now - 24 * 60 * 60;

        storage
            .add_log(build_request_log(
                "yesterday-jdd",
                yesterday,
                120,
                Some("ant-jdd"),
                Some("姜大大"),
                Some("jdd"),
            ))
            .await;
        storage
            .add_log(build_request_log(
                "today-jdd",
                now,
                220,
                Some("ant-jdd"),
                Some("姜大大"),
                Some("jdd"),
            ))
            .await;
        storage
            .add_log(build_request_log(
                "today-cr",
                now,
                300,
                Some("ant-cr"),
                Some("CR"),
                Some("cr"),
            ))
            .await;

        let response = get_antigravity_daily_stats_by_gateway_profile_from_storage_with(
            Some(&storage),
            Some(2),
        )
        .unwrap();

        assert_eq!(response.series.len(), 2);
        assert_eq!(response.series[0].profile_id, "ant-cr");
        assert_eq!(response.series[1].profile_id, "ant-jdd");
        assert_eq!(response.series[1].stats[0].tokens, 120);
        assert_eq!(response.series[1].stats[1].tokens, 220);
    }

    #[tokio::test]
    async fn antigravity_storage_helpers_return_all_time_totals() {
        let temp_dir = tempdir().unwrap();
        let storage = AntigravityLogStorage::new(temp_dir.path().to_path_buf()).unwrap();
        let now = chrono::Utc::now().timestamp();

        storage
            .add_log(build_request_log(
                "log-1",
                now,
                111,
                Some("ant-jdd"),
                Some("姜大大"),
                Some("jdd"),
            ))
            .await;
        storage
            .add_log(build_request_log(
                "log-2",
                now,
                222,
                Some("ant-cr"),
                Some("CR"),
                Some("cr"),
            ))
            .await;

        let stats = get_antigravity_all_time_stats_with(Some(&storage)).unwrap();

        assert_eq!(stats.requests, 2);
        assert_eq!(stats.tokens, 333);
    }

    #[tokio::test]
    async fn antigravity_log_summary_counts_success_and_error_rows() {
        let temp_dir = tempdir().unwrap();
        let storage = AntigravityLogStorage::new(temp_dir.path().to_path_buf()).unwrap();
        let now = chrono::Utc::now().timestamp();

        storage
            .add_log(build_request_log(
                "success-jdd",
                now,
                120,
                Some("ant-jdd"),
                Some("姜大大"),
                Some("jdd"),
            ))
            .await;

        let mut failed = build_request_log(
            "error-jdd",
            now,
            80,
            Some("ant-jdd"),
            Some("姜大大"),
            Some("jdd"),
        );
        failed.status = "error".into();
        failed.error_message = Some("upstream failed".into());
        storage.add_log(failed).await;

        let summary = get_antigravity_log_summary_from_storage_with(
            Some(&storage),
            LogQuery {
                limit: Some(1),
                offset: Some(1),
                start_ts: Some(now - 60),
                end_ts: Some(now + 60),
                model: None,
                format: None,
                status: None,
                account_id: None,
                member_code: Some("jdd".into()),
            },
        )
        .unwrap();

        assert_eq!(summary.total_requests, 2);
        assert_eq!(summary.success_requests, 1);
        assert_eq!(summary.error_requests, 1);
        assert_eq!(summary.total_tokens, 200);
        assert_eq!(summary.success_rate, 50.0);
    }
}
