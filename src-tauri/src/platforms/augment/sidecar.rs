//! CLIProxyAPI sidecar 管理器
//!
//! 管理 CLIProxyAPI Go 二进制的生命周期，负责启动、停止、健康检查和 auth 文件同步。

use crate::storage::TokenData;
use serde_json::json;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::{Child, Command};
use url::Url;

const AUGGIE_PROVIDER: &str = "auggie";
const AUGGIE_CLIENT_ID: &str = "auggie-cli";
const AUGGIE_LOGIN_MODE: &str = "localhost";
const AUGGIE_DEFAULT_SCOPE: &str = "email";

/// CLIProxyAPI sidecar 管理器
pub struct AugmentSidecar {
    port: u16,
    child: Option<Child>,
    auth_dir: PathBuf,
    config_path: PathBuf,
    api_key: String,
    binary_path: PathBuf,
}

impl AugmentSidecar {
    /// 创建新的 sidecar 管理器（不启动进程）
    pub fn new(app_data_dir: &std::path::Path, binary_path: PathBuf) -> Self {
        let auth_dir = app_data_dir.join("cliproxy_auths");
        let config_path = app_data_dir.join("cliproxy_config.yaml");
        let api_key = format!("sk-atm-internal-{}", uuid::Uuid::new_v4());

        Self {
            port: 0,
            child: None,
            auth_dir,
            config_path,
            api_key,
            binary_path,
        }
    }

    /// 获取 sidecar 的 base URL
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// 获取内部 API key
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// 是否正在运行
    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }

    /// 启动 sidecar 进程
    pub async fn start(&mut self, tokens: &[TokenData]) -> Result<(), String> {
        if self.child.is_some() {
            return Ok(());
        }

        // 找空闲端口
        self.port = find_available_port().map_err(|e| format!("Failed to find port: {}", e))?;

        // 创建 auth 目录
        std::fs::create_dir_all(&self.auth_dir)
            .map_err(|e| format!("Failed to create auth dir: {}", e))?;

        // 同步账号
        self.sync_accounts(tokens)?;

        // 生成 config.yaml
        self.write_config()?;

        // 启动子进程
        let child = Command::new(&self.binary_path)
            .arg("-config")
            .arg(&self.config_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Failed to start cliproxy-server: {}", e))?;

        self.child = Some(child);

        // 等待健康检查
        self.wait_healthy(10).await?;

        println!(
            "[AugmentSidecar] Started on port {} with {} accounts",
            self.port,
            tokens.len()
        );
        Ok(())
    }

    /// 停止 sidecar 进程
    pub async fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        // 清理临时文件
        let _ = std::fs::remove_dir_all(&self.auth_dir);
        let _ = std::fs::remove_file(&self.config_path);
        println!("[AugmentSidecar] Stopped");
    }

    /// 同步 Augment 账号到 CLIProxyAPI 的 auth 文件
    pub fn sync_accounts(&self, tokens: &[TokenData]) -> Result<(), String> {
        std::fs::create_dir_all(&self.auth_dir)
            .map_err(|e| format!("Failed to create auth dir: {}", e))?;

        let staging_dir = self.auth_dir.with_file_name(format!(
            "{}.staging-{}",
            self.auth_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("cliproxy_auths"),
            uuid::Uuid::new_v4()
        ));

        let _ = std::fs::remove_dir_all(&staging_dir);
        std::fs::create_dir_all(&staging_dir)
            .map_err(|e| format!("Failed to create staging auth dir: {}", e))?;

        let result = (|| -> Result<(), String> {
            let mut desired_files = std::collections::HashSet::new();

            for token in tokens {
                let Ok((filename, auth)) = build_auth_file(token) else {
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

                if target_path.is_dir() {
                    return Err(format!(
                        "Failed to write auth file {}: target path is a directory",
                        filename
                    ));
                }

                if target_path.exists() {
                    std::fs::remove_file(&target_path)
                        .map_err(|e| format!("Failed to write auth file {}: {}", filename, e))?;
                }

                std::fs::rename(&staging_path, &target_path)
                    .map_err(|e| format!("Failed to write auth file {}: {}", filename, e))?;
            }

            if let Ok(entries) = std::fs::read_dir(&self.auth_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.extension().map_or(false, |e| e == "json") {
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

    /// 确保 sidecar 正在运行，如果没有则启动
    pub async fn ensure_running(&mut self, tokens: &[TokenData]) -> Result<(), String> {
        // 检查子进程是否还活着
        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(Some(_)) => {
                    // 进程已退出，清理
                    self.child = None;
                }
                Ok(None) => {
                    // 进程还在运行，同步账号
                    self.sync_accounts(tokens)?;
                    return Ok(());
                }
                Err(_) => {
                    self.child = None;
                }
            }
        }

        // 需要启动
        self.start(tokens).await
    }

    fn write_config(&self) -> Result<(), String> {
        let config = build_config_yaml(self.port, &self.auth_dir, &self.api_key);

        std::fs::write(&self.config_path, config)
            .map_err(|e| format!("Failed to write config: {}", e))
    }

    async fn wait_healthy(&self, max_seconds: u64) -> Result<(), String> {
        let url = format!("{}/v1/models", self.base_url());
        let client = reqwest::Client::new();

        for i in 0..max_seconds * 2 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            match client
                .get(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .timeout(std::time::Duration::from_secs(2))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    println!(
                        "[AugmentSidecar] Health check passed after {}ms",
                        (i + 1) * 500
                    );
                    return Ok(());
                }
                _ => continue,
            }
        }

        Err(format!(
            "Sidecar health check failed after {}s",
            max_seconds
        ))
    }
}

impl Drop for AugmentSidecar {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.start_kill();
        }
        let _ = std::fs::remove_dir_all(&self.auth_dir);
        let _ = std::fs::remove_file(&self.config_path);
    }
}

/// 找一个可用的 TCP 端口
fn find_available_port() -> Result<u16, std::io::Error> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

fn build_config_yaml(port: u16, auth_dir: &std::path::Path, api_key: &str) -> String {
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
      provider: auggie
routing:
  strategy: round-robin
debug: false
request-log: false
remote-management:
  allow-remote: false
  disable-control-panel: true
"#,
        port,
        auth_dir,
        api_key,
        api_key,
    )
}

fn build_auth_file(token: &TokenData) -> Result<(String, String), String> {
    if token.access_token.trim().is_empty() {
        return Err("Missing access_token".to_string());
    }

    let tenant_url = normalized_tenant_url(&token.tenant_url)
        .ok_or_else(|| "Missing tenant_url".to_string())?;
    let label =
        auth_label_for_tenant(&tenant_url).ok_or_else(|| "Invalid tenant_url".to_string())?;
    let filename = auth_filename_for_tenant(&tenant_url)
        .ok_or_else(|| "Invalid tenant_url".to_string())?;
    let auth = json!({
        "type": AUGGIE_PROVIDER,
        "label": label,
        "access_token": token.access_token,
        "tenant_url": tenant_url,
        "scopes": [AUGGIE_DEFAULT_SCOPE],
        "client_id": AUGGIE_CLIENT_ID,
        "login_mode": AUGGIE_LOGIN_MODE,
        "last_refresh": token.updated_at.to_rfc3339()
    });
    let raw = serde_json::to_string(&auth)
        .map_err(|e| format!("Failed to serialize auth file: {}", e))?;

    Ok((filename, raw))
}

fn auth_filename_for_tenant(tenant_url: &str) -> Option<String> {
    let label = auth_label_for_tenant(tenant_url)?;
    Some(format!("auggie-{}.json", label.replace('.', "-")))
}

fn auth_label_for_tenant(tenant_url: &str) -> Option<String> {
    let parsed = parse_tenant_url(tenant_url)?;
    parsed.host_str().map(|host| host.to_ascii_lowercase())
}

fn normalized_tenant_url(tenant_url: &str) -> Option<String> {
    let parsed = parse_tenant_url(tenant_url)?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let authority = match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    };

    Some(format!("{}://{authority}/", parsed.scheme()))
}

fn parse_tenant_url(tenant_url: &str) -> Option<Url> {
    let trimmed = tenant_url.trim();
    if trimmed.is_empty() {
        return None;
    }

    Url::parse(trimmed)
        .ok()
        .or_else(|| Url::parse(&format!("https://{trimmed}")).ok())
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
    use chrono::{TimeZone, Utc};
    use serde_json::{Value, json};
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn sample_token(tenant_url: &str, access_token: &str) -> TokenData {
        let updated_at = Utc.with_ymd_and_hms(2026, 3, 13, 1, 2, 3).unwrap();

        TokenData {
            id: "token-1".to_string(),
            tenant_url: tenant_url.to_string(),
            access_token: access_token.to_string(),
            created_at: updated_at,
            updated_at,
            portal_url: None,
            email_note: None,
            tag_name: None,
            tag_color: None,
            ban_status: None,
            portal_info: None,
            auth_session: None,
            suspensions: None,
            balance_color_mode: None,
            skip_check: None,
            session_updated_at: None,
            version: 0,
        }
    }

    #[test]
    fn sidecar_config_yaml_matches_cliproxy_contract() {
        let temp_dir = tempdir().unwrap();
        let mut sidecar = AugmentSidecar::new(temp_dir.path(), PathBuf::from("/tmp/cliproxy-server"));
        sidecar.port = 43123;

        sidecar.write_config().unwrap();

        let yaml = fs::read_to_string(&sidecar.config_path).unwrap();
        assert!(yaml.contains("host: \"127.0.0.1\""));
        assert!(yaml.contains("port: 43123"));
        assert!(yaml.contains(&format!("auth-dir: \"{}\"", sidecar.auth_dir.display())));
        assert!(yaml.contains("api-keys:"));
        assert!(yaml.contains(&format!("  - \"{}\"", sidecar.api_key())));
        assert!(yaml.contains("debug: false"));
        assert!(yaml.contains("request-log: false"));
        assert!(yaml.contains("routing:"));
        assert!(yaml.contains("strategy: round-robin"));
        assert!(!yaml.contains("routing-strategy:"));
    }

    #[test]
    fn sidecar_auth_json_matches_auggie_metadata_contract() {
        let temp_dir = tempdir().unwrap();
        let sidecar = AugmentSidecar::new(temp_dir.path(), PathBuf::from("/tmp/cliproxy-server"));
        let token = sample_token("https://tenant.augmentcode.com/", "token-1");

        sidecar.sync_accounts(&[token]).unwrap();

        let auth_path = sidecar.auth_dir.join("auggie-tenant-augmentcode-com.json");
        let raw = fs::read_to_string(auth_path).unwrap();
        let json: Value = serde_json::from_str(&raw).unwrap();

        assert_eq!(json["type"], "auggie");
        assert_eq!(json["label"], "tenant.augmentcode.com");
        assert_eq!(json["access_token"], "token-1");
        assert_eq!(json["tenant_url"], "https://tenant.augmentcode.com/");
        assert_eq!(json["client_id"], "auggie-cli");
        assert_eq!(json["login_mode"], "localhost");
        assert_eq!(json["last_refresh"], "2026-03-13T01:02:03+00:00");
        assert_eq!(json["scopes"], json!(["email"]));
    }

    #[test]
    fn sidecar_config_yaml_escapes_auth_dir_for_yaml() {
        let yaml = build_config_yaml(
            43123,
            Path::new("C:\\Users\\me\\\"quoted\"\\ATM Data"),
            "sk-test",
        );

        assert!(yaml.contains("auth-dir: \"C:\\\\Users\\\\me\\\\\\\"quoted\\\"\\\\ATM Data\""));
    }

    #[test]
    fn sync_accounts_keeps_existing_auths_when_new_write_fails() {
        let temp_dir = tempdir().unwrap();
        let sidecar = AugmentSidecar::new(temp_dir.path(), PathBuf::from("/tmp/cliproxy-server"));

        fs::create_dir_all(&sidecar.auth_dir).unwrap();

        let preserved_path = sidecar.auth_dir.join("auggie-existing.json");
        fs::write(&preserved_path, r#"{"type":"auggie","label":"existing"}"#).unwrap();
        fs::create_dir(sidecar.auth_dir.join("auggie-tenant-augmentcode-com.json")).unwrap();

        let err = sidecar
            .sync_accounts(&[sample_token("https://tenant.augmentcode.com/", "token-1")])
            .unwrap_err();

        assert!(err.contains("Failed to write auth file"));
        assert!(preserved_path.exists());
    }
}
