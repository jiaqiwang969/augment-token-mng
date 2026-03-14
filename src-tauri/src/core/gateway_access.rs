use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tauri::Manager;

use crate::platforms::openai::codex::pool::CodexServerConfig;
use crate::AppState;

const GATEWAY_ACCESS_PROFILES_FILE: &str = "gateway_access_profiles.json";
const LEGACY_CODEX_CONFIG_FILE: &str = "openai_codex_config.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GatewayTarget {
    Codex,
    Augment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayAccessProfile {
    pub id: String,
    pub name: String,
    pub target: GatewayTarget,
    pub api_key: String,
    #[serde(default = "default_profile_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayAccessProfiles {
    #[serde(default)]
    pub profiles: Vec<GatewayAccessProfile>,
}

fn default_profile_enabled() -> bool {
    true
}

impl GatewayAccessProfiles {
    pub fn migrate_from_legacy_codex_key(legacy_api_key: Option<String>) -> Self {
        let api_key = legacy_api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        match api_key {
            Some(api_key) => Self {
                profiles: vec![GatewayAccessProfile {
                    id: "codex-default".to_string(),
                    name: "Codex Default".to_string(),
                    target: GatewayTarget::Codex,
                    api_key,
                    enabled: true,
                }],
            },
            None => Self::default(),
        }
    }

    pub fn find_by_key(&self, api_key: &str) -> Option<&GatewayAccessProfile> {
        let expected = api_key.trim();
        if expected.is_empty() {
            return None;
        }

        self.profiles
            .iter()
            .find(|profile| profile.enabled && profile.api_key.trim() == expected)
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}

fn gateway_access_profiles_path(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join(GATEWAY_ACCESS_PROFILES_FILE)
}

fn normalize_legacy_codex_api_key(config: &mut CodexServerConfig) {
    config.api_key = config.api_key.as_ref().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
}

fn load_legacy_codex_api_key(app_data_dir: &Path) -> Result<Option<String>, String> {
    let config_path = app_data_dir.join(LEGACY_CODEX_CONFIG_FILE);
    if !config_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read {}: {}", LEGACY_CODEX_CONFIG_FILE, e))?;
    if content.trim().is_empty() {
        return Ok(None);
    }

    let mut config: CodexServerConfig = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", LEGACY_CODEX_CONFIG_FILE, e))?;
    normalize_legacy_codex_api_key(&mut config);
    Ok(config.api_key)
}

pub fn load_gateway_access_profiles(app_data_dir: &Path) -> Result<GatewayAccessProfiles, String> {
    let config_path = gateway_access_profiles_path(app_data_dir);
    if !config_path.exists() {
        return Ok(GatewayAccessProfiles::default());
    }

    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read {}: {}", GATEWAY_ACCESS_PROFILES_FILE, e))?;
    if content.trim().is_empty() {
        return Ok(GatewayAccessProfiles::default());
    }

    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", GATEWAY_ACCESS_PROFILES_FILE, e))
}

pub fn write_gateway_access_profiles(
    app_data_dir: &Path,
    profiles: &GatewayAccessProfiles,
) -> Result<(), String> {
    fs::create_dir_all(app_data_dir)
        .map_err(|e| format!("Failed to create app data directory: {}", e))?;
    let content = serde_json::to_string_pretty(profiles)
        .map_err(|e| format!("Failed to serialize gateway access profiles: {}", e))?;
    fs::write(gateway_access_profiles_path(app_data_dir), content)
        .map_err(|e| format!("Failed to write {}: {}", GATEWAY_ACCESS_PROFILES_FILE, e))
}

pub fn get_or_load_gateway_access_profiles(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<GatewayAccessProfiles, String> {
    if let Some(profiles) = state.gateway_access_profiles.lock().unwrap().clone() {
        return Ok(profiles);
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    let mut profiles = load_gateway_access_profiles(&app_data_dir)?;
    if profiles.is_empty() {
        let legacy_key = load_legacy_codex_api_key(&app_data_dir)?;
        let migrated = GatewayAccessProfiles::migrate_from_legacy_codex_key(legacy_key);
        if !migrated.is_empty() {
            write_gateway_access_profiles(&app_data_dir, &migrated)?;
        }
        profiles = migrated;
    }

    *state.gateway_access_profiles.lock().unwrap() = Some(profiles.clone());
    Ok(profiles)
}

pub fn set_gateway_access_profiles(
    app: &tauri::AppHandle,
    state: &AppState,
    profiles: GatewayAccessProfiles,
) -> Result<GatewayAccessProfiles, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    write_gateway_access_profiles(&app_data_dir, &profiles)?;
    *state.gateway_access_profiles.lock().unwrap() = Some(profiles.clone());
    Ok(profiles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn gateway_access_loads_empty_profile_set_when_file_missing() {
        let temp_dir = tempdir().unwrap();
        let profiles = load_gateway_access_profiles(temp_dir.path()).unwrap();

        assert!(profiles.profiles.is_empty());
    }

    #[test]
    fn gateway_access_migrates_legacy_codex_key_into_shared_profile() {
        let profiles =
            GatewayAccessProfiles::migrate_from_legacy_codex_key(Some("sk-legacy".into()));
        let profile = profiles.find_by_key("sk-legacy").unwrap();

        assert_eq!(profile.id, "codex-default");
        assert_eq!(profile.target, GatewayTarget::Codex);
        assert!(profile.enabled);
    }

    #[test]
    fn gateway_access_resolves_profile_by_key() {
        let profiles = GatewayAccessProfiles {
            profiles: vec![GatewayAccessProfile {
                id: "augment-default".into(),
                name: "Augment Default".into(),
                target: GatewayTarget::Augment,
                api_key: "sk-auggie".into(),
                enabled: true,
            }],
        };

        let profile = profiles.find_by_key("  sk-auggie ").unwrap();

        assert_eq!(profile.id, "augment-default");
        assert_eq!(profile.target, GatewayTarget::Augment);
    }

    #[test]
    fn gateway_access_refuses_disabled_profiles() {
        let profiles = GatewayAccessProfiles {
            profiles: vec![GatewayAccessProfile {
                id: "codex-disabled".into(),
                name: "Codex Disabled".into(),
                target: GatewayTarget::Codex,
                api_key: "sk-disabled".into(),
                enabled: false,
            }],
        };

        assert!(profiles.find_by_key("sk-disabled").is_none());
    }

    #[test]
    fn gateway_access_writes_and_reads_profiles() {
        let temp_dir = tempdir().unwrap();
        let profiles = GatewayAccessProfiles {
            profiles: vec![GatewayAccessProfile {
                id: "augment-default".into(),
                name: "Augment Default".into(),
                target: GatewayTarget::Augment,
                api_key: "sk-write-read".into(),
                enabled: true,
            }],
        };

        write_gateway_access_profiles(temp_dir.path(), &profiles).unwrap();
        let loaded = load_gateway_access_profiles(temp_dir.path()).unwrap();

        assert_eq!(loaded, profiles);
    }
}
