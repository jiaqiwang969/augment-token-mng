use crate::platforms::antigravity::models::Account;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, Command};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SidecarRuntimeMetadata {
    pid: u32,
    config_path: PathBuf,
    binary_path: PathBuf,
}

pub struct AntigravitySidecar {
    port: u16,
    child: Option<Child>,
    auth_dir: PathBuf,
    home_dir: PathBuf,
    config_path: PathBuf,
    runtime_path: PathBuf,
    api_key: String,
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

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
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

    pub async fn is_healthy(&self) -> bool {
        if self.port == 0 {
            return false;
        }

        probe_sidecar_health(&self.base_url(), &self.api_key).await
    }

    #[cfg(test)]
    pub(crate) fn set_port_for_test(&mut self, port: u16) {
        self.port = port;
    }

    pub async fn start(&mut self, accounts: &[Account]) -> Result<(), String> {
        if self.child.is_some() {
            return Ok(());
        }

        self.port = find_available_port().map_err(|e| format!("Failed to find port: {}", e))?;

        std::fs::create_dir_all(&self.auth_dir)
            .map_err(|e| format!("Failed to create auth dir: {}", e))?;
        std::fs::create_dir_all(&self.home_dir)
            .map_err(|e| format!("Failed to create sidecar home dir: {}", e))?;

        self.sync_accounts(accounts)?;
        self.write_config()?;

        let mut command = Command::new(&self.binary_path);
        command
            .arg("-config")
            .arg(&self.config_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        for (key, value) in sidecar_process_env(&self.home_dir) {
            command.env(key, value);
        }

        let child = command
            .spawn()
            .map_err(|e| format!("Failed to start cliproxy-server: {}", e))?;

        self.child = Some(child);
        self.persist_runtime_metadata()?;
        self.wait_healthy(10).await?;
        Ok(())
    }

    pub async fn ensure_running(&mut self, accounts: &[Account]) -> Result<(), String> {
        let child_alive = if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) => {
                    self.child = None;
                    false
                }
                Ok(None) => true,
                Err(_) => {
                    self.child = None;
                    false
                }
            }
        } else {
            false
        };

        if child_alive {
            self.sync_accounts(accounts)?;
            if self.is_healthy().await {
                return Ok(());
            }

            self.stop().await;
        }

        self.start(accounts).await
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

    async fn wait_healthy(&self, max_seconds: u64) -> Result<(), String> {
        for _ in 0..max_seconds * 2 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if self.is_healthy().await {
                return Ok(());
            }
        }

        Err(format!(
            "Antigravity sidecar health check failed after {}s",
            max_seconds
        ))
    }

    fn cleanup_runtime_files(&self) {
        let _ = std::fs::remove_dir_all(&self.auth_dir);
        let _ = std::fs::remove_dir_all(&self.home_dir);
        let _ = std::fs::remove_file(&self.config_path);
        let _ = std::fs::remove_file(&self.runtime_path);
    }

    fn persist_runtime_metadata(&self) -> Result<(), String> {
        let Some(pid) = self.child.as_ref().and_then(|child| child.id()) else {
            return Ok(());
        };

        let metadata = SidecarRuntimeMetadata {
            pid,
            config_path: self.config_path.clone(),
            binary_path: self.binary_path.clone(),
        };
        let raw = serde_json::to_vec(&metadata)
            .map_err(|e| format!("Failed to serialize Antigravity sidecar metadata: {}", e))?;

        std::fs::write(&self.runtime_path, raw)
            .map_err(|e| format!("Failed to write Antigravity sidecar metadata: {}", e))
    }
}

impl Drop for AntigravitySidecar {
    fn drop(&mut self) {
        self.force_stop();
    }
}

fn find_available_port() -> Result<u16, std::io::Error> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
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

fn sidecar_process_env(home_dir: &Path) -> Vec<(&'static str, PathBuf)> {
    let home_dir = home_dir.to_path_buf();
    let mut env = vec![
        ("HOME", home_dir.clone()),
        ("USERPROFILE", home_dir.clone()),
    ];

    if let Some((drive, path)) = split_windows_home_components(&home_dir) {
        env.push(("HOMEDRIVE", PathBuf::from(drive)));
        env.push(("HOMEPATH", PathBuf::from(path)));
    }

    env
}

pub(crate) async fn probe_sidecar_health(base_url: &str, api_key: &str) -> bool {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();

    match client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => match resp.bytes().await {
            Ok(body) => models_payload_is_ready(body.as_ref()),
            Err(_) => false,
        },
        _ => false,
    }
}

fn split_windows_home_components(home_dir: &Path) -> Option<(String, String)> {
    let normalized = home_dir.to_string_lossy().replace('/', "\\");
    let bytes = normalized.as_bytes();

    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return None;
    }

    Some((normalized[..2].to_string(), normalized[2..].to_string()))
}

fn models_payload_is_ready(body: &[u8]) -> bool {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) else {
        return false;
    };

    json.get("data")
        .and_then(|data| data.as_array())
        .is_some_and(|models| !models.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::antigravity::models::{QuotaData, TokenData};
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
