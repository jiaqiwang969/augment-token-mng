use crate::platforms::antigravity::models::Account;
use serde_json::json;
use std::path::{Path, PathBuf};
use tokio::process::Child;

pub struct AntigravitySidecar {
    port: u16,
    child: Option<Child>,
    auth_dir: PathBuf,
    home_dir: PathBuf,
    config_path: PathBuf,
    runtime_path: PathBuf,
    api_key: String,
    #[allow(dead_code)]
    binary_path: PathBuf,
}

impl AntigravitySidecar {
    pub fn new(app_data_dir: &Path, binary_path: PathBuf) -> Self {
        Self {
            port: 0,
            child: None,
            auth_dir: app_data_dir.join("cliproxy_antigravity_auths"),
            home_dir: app_data_dir.join("cliproxy_antigravity_home"),
            config_path: app_data_dir.join("cliproxy_antigravity_config.yaml"),
            runtime_path: app_data_dir.join("cliproxy_antigravity_runtime.json"),
            api_key: format!("sk-atm-antigravity-internal-{}", uuid::Uuid::new_v4()),
            binary_path,
        }
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    pub fn auth_dir(&self) -> &Path {
        &self.auth_dir
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn runtime_path(&self) -> &Path {
        &self.runtime_path
    }

    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }

    #[cfg(test)]
    pub(crate) fn set_port_for_test(&mut self, port: u16) {
        self.port = port;
    }

    pub fn write_config(&self) -> Result<(), String> {
        let config = build_config_yaml(self.port, &self.auth_dir, &self.api_key);
        std::fs::write(&self.config_path, config)
            .map_err(|e| format!("Failed to write Antigravity sidecar config: {}", e))
    }

    pub fn sync_accounts(&self, accounts: &[Account]) -> Result<(), String> {
        std::fs::create_dir_all(&self.auth_dir)
            .map_err(|e| format!("Failed to create auth dir: {}", e))?;

        let staging_dir = self.auth_dir.with_file_name(format!(
            "{}.staging-{}",
            self.auth_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("cliproxy_antigravity_auths"),
            uuid::Uuid::new_v4()
        ));

        let _ = std::fs::remove_dir_all(&staging_dir);
        std::fs::create_dir_all(&staging_dir)
            .map_err(|e| format!("Failed to create staging auth dir: {}", e))?;

        let result = (|| -> Result<(), String> {
            let mut desired_files = std::collections::HashSet::new();

            for account in accounts {
                let Ok((filename, auth)) = build_auth_file(account) else {
                    continue;
                };

                let staging_path = staging_dir.join(&filename);
                std::fs::write(&staging_path, auth)
                    .map_err(|e| format!("Failed to write auth file {}: {}", filename, e))?;
                desired_files.insert(filename);
            }

            for filename in &desired_files {
                let staging_path = staging_dir.join(filename);
                let target_path = self.auth_dir.join(filename);

                if target_path.exists() {
                    std::fs::remove_file(&target_path).map_err(|e| {
                        format!("Failed to replace auth file {}: {}", filename, e)
                    })?;
                }

                std::fs::rename(&staging_path, &target_path)
                    .map_err(|e| format!("Failed to publish auth file {}: {}", filename, e))?;
            }

            if let Ok(entries) = std::fs::read_dir(&self.auth_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.extension().is_some_and(|ext| ext == "json") {
                        continue;
                    }

                    let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
                        continue;
                    };
                    if desired_files.contains(filename) {
                        continue;
                    }

                    std::fs::remove_file(&path).map_err(|e| {
                        format!("Failed to remove stale auth file {}: {}", filename, e)
                    })?;
                }
            }

            Ok(())
        })();

        let _ = std::fs::remove_dir_all(&staging_dir);
        result
    }

    pub async fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        self.port = 0;
        self.cleanup_runtime_files();
    }

    pub fn force_stop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
        self.child = None;
        self.port = 0;
        self.cleanup_runtime_files();
    }

    fn cleanup_runtime_files(&self) {
        let _ = std::fs::remove_dir_all(&self.auth_dir);
        let _ = std::fs::remove_dir_all(&self.home_dir);
        let _ = std::fs::remove_file(&self.config_path);
        let _ = std::fs::remove_file(&self.runtime_path);
    }
}

impl Drop for AntigravitySidecar {
    fn drop(&mut self) {
        self.force_stop();
    }
}

fn build_config_yaml(port: u16, auth_dir: &Path, api_key: &str) -> String {
    let auth_dir = yaml_double_quoted(&auth_dir.display().to_string());
    let api_key = yaml_double_quoted(api_key);

    format!(
        r#"host: "127.0.0.1"
port: {}
auth-dir: {}
api-keys:
  - {}
client-api-keys:
  - key: {}
    enabled: true
    scope:
      provider: antigravity
routing:
  strategy: round-robin
debug: false
request-log: false
remote-management:
  allow-remote: false
  disable-control-panel: true
"#,
        port, auth_dir, api_key, api_key,
    )
}

fn build_auth_file(account: &Account) -> Result<(String, String), String> {
    let email = if account.email.trim().is_empty() {
        account
            .token
            .email
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string()
    } else {
        account.email.trim().to_string()
    };
    let access_token = account.token.access_token.trim().to_string();
    if access_token.is_empty() {
        return Err("Missing access_token".to_string());
    }

    let refresh_token = account.token.refresh_token.trim().to_string();
    let timestamp_seconds = if account.updated_at > 0 {
        account.updated_at
    } else {
        chrono::Utc::now().timestamp()
    };
    let expired = chrono::DateTime::<chrono::Utc>::from_timestamp(account.token.expiry_timestamp, 0)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();
    let metadata = json!({
        "type": "antigravity",
        "label": if email.is_empty() { "antigravity" } else { email.as_str() },
        "email": if email.is_empty() { serde_json::Value::Null } else { json!(email) },
        "access_token": access_token,
        "refresh_token": refresh_token,
        "expires_in": account.token.expires_in,
        "timestamp": timestamp_seconds * 1000,
        "expired": expired,
        "project_id": account.token.project_id,
        "session_id": account.token.session_id,
    });
    let raw = serde_json::to_string(&metadata)
        .map_err(|e| format!("Failed to serialize auth file: {}", e))?;

    Ok((auth_filename_for_email(&email), raw))
}

fn auth_filename_for_email(email: &str) -> String {
    let trimmed = email.trim();
    if trimmed.is_empty() {
        "antigravity.json".to_string()
    } else {
        format!("antigravity-{}.json", trimmed)
    }
}

fn yaml_double_quoted(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::antigravity::models::{Account, QuotaData, TokenData};
    use chrono::{TimeZone, Utc};
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

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
        account.updated_at = Utc.with_ymd_and_hms(2026, 3, 15, 8, 0, 0).unwrap().timestamp();
        account.quota = Some(QuotaData::new());
        account
    }

    #[test]
    fn antigravity_sidecar_uses_dedicated_runtime_paths() {
        let dir = tempdir().unwrap();
        let sidecar = AntigravitySidecar::new(dir.path(), PathBuf::from("/tmp/cliproxy-server"));

        assert!(sidecar.config_path().ends_with("cliproxy_antigravity_config.yaml"));
        assert!(sidecar.auth_dir().ends_with("cliproxy_antigravity_auths"));
        assert!(sidecar.runtime_path().ends_with("cliproxy_antigravity_runtime.json"));
    }

    #[test]
    fn antigravity_sidecar_config_yaml_matches_cliproxy_contract() {
        let dir = tempdir().unwrap();
        let mut sidecar =
            AntigravitySidecar::new(dir.path(), PathBuf::from("/tmp/cliproxy-server"));
        sidecar.set_port_for_test(43124);

        sidecar.write_config().unwrap();

        let yaml = fs::read_to_string(sidecar.config_path()).unwrap();
        assert!(yaml.contains("host: \"127.0.0.1\""));
        assert!(yaml.contains("port: 43124"));
        assert!(yaml.contains(&format!("auth-dir: \"{}\"", sidecar.auth_dir().display())));
        assert!(yaml.contains("api-keys:"));
        assert!(yaml.contains(&format!("  - \"{}\"", sidecar.api_key())));
        assert!(yaml.contains("provider: antigravity"));
        assert!(yaml.contains("debug: false"));
        assert!(yaml.contains("request-log: false"));
    }

    #[test]
    fn antigravity_sidecar_sync_accounts_writes_antigravity_auth_files() {
        let dir = tempdir().unwrap();
        let sidecar = AntigravitySidecar::new(dir.path(), PathBuf::from("/tmp/cliproxy-server"));

        sidecar
            .sync_accounts(&[sample_account("jdd@lingkong.xyz")])
            .unwrap();

        let auth_path = sidecar.auth_dir().join("antigravity-jdd@lingkong.xyz.json");
        let raw = fs::read_to_string(auth_path).unwrap();
        let json: Value = serde_json::from_str(&raw).unwrap();

        assert_eq!(json["type"], "antigravity");
        assert_eq!(json["email"], "jdd@lingkong.xyz");
        assert_eq!(json["access_token"], "access-token-1");
        assert_eq!(json["refresh_token"], "refresh-token-1");
        assert_eq!(json["project_id"], "project-123");
        assert_eq!(json["session_id"], "session-1");
        assert_eq!(json["expires_in"], 3600);
        assert!(json["expired"].as_str().is_some());
    }
}
