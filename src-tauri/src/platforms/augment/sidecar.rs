//! CLIProxyAPI sidecar 管理器
//!
//! 管理 CLIProxyAPI Go 二进制的生命周期，负责启动、停止、健康检查和 auth 文件同步。

use crate::storage::TokenData;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::process::Stdio;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::process::{Child, Command};
use url::Url;

const AUGGIE_PROVIDER: &str = "auggie";
const AUGGIE_CLIENT_ID: &str = "auggie-cli";
const AUGGIE_LOGIN_MODE: &str = "localhost";
const AUGGIE_DEFAULT_SCOPE: &str = "email";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SidecarRuntimeMetadata {
    pid: u32,
    config_path: PathBuf,
    binary_path: PathBuf,
}

/// CLIProxyAPI sidecar 管理器
pub struct AugmentSidecar {
    port: u16,
    child: Option<Child>,
    auth_dir: PathBuf,
    home_dir: PathBuf,
    config_path: PathBuf,
    runtime_path: PathBuf,
    api_key: String,
    binary_path: PathBuf,
}

impl AugmentSidecar {
    /// 创建新的 sidecar 管理器（不启动进程）
    pub fn new(app_data_dir: &std::path::Path, binary_path: PathBuf) -> Self {
        let auth_dir = app_data_dir.join("cliproxy_auths");
        let home_dir = app_data_dir.join("cliproxy_home");
        let config_path = app_data_dir.join("cliproxy_config.yaml");
        let runtime_path = app_data_dir.join("cliproxy_runtime.json");
        let api_key = format!("sk-atm-internal-{}", uuid::Uuid::new_v4());

        Self {
            port: 0,
            child: None,
            auth_dir,
            home_dir,
            config_path,
            runtime_path,
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

    /// sidecar 是否健康可用
    pub async fn is_healthy(&self) -> bool {
        if self.port == 0 {
            return false;
        }

        probe_sidecar_health(&self.base_url(), &self.api_key).await
    }

    /// 同步强制停止 sidecar，用于退出路径
    pub fn force_stop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
        self.child = None;
        self.port = 0;
        self.cleanup_runtime_files();
    }

    /// 启动 sidecar 进程
    pub async fn start(&mut self, tokens: &[TokenData]) -> Result<(), String> {
        if self.child.is_some() {
            return Ok(());
        }

        self.reap_stale_managed_sidecar()?;

        // 找空闲端口
        self.port = find_available_port().map_err(|e| format!("Failed to find port: {}", e))?;

        // 创建 auth 目录
        std::fs::create_dir_all(&self.auth_dir)
            .map_err(|e| format!("Failed to create auth dir: {}", e))?;
        std::fs::create_dir_all(self.home_dir.join(".augment"))
            .map_err(|e| format!("Failed to create sidecar home dir: {}", e))?;

        // 同步账号
        self.sync_accounts(tokens)?;

        // 生成 config.yaml
        self.write_config()?;

        // 启动子进程
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
        self.port = 0;
        self.cleanup_runtime_files();
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
            self.sync_accounts(tokens)?;
            if self.is_healthy().await {
                return Ok(());
            }

            println!("[AugmentSidecar] Running sidecar is unhealthy, restarting");
            self.stop().await;
        }

        self.start(tokens).await
    }

    fn write_config(&self) -> Result<(), String> {
        let config = build_config_yaml(self.port, &self.auth_dir, &self.api_key);

        std::fs::write(&self.config_path, config)
            .map_err(|e| format!("Failed to write config: {}", e))
    }

    async fn wait_healthy(&self, max_seconds: u64) -> Result<(), String> {
        for i in 0..max_seconds * 2 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            if self.is_healthy().await {
                println!(
                    "[AugmentSidecar] Health check passed after {}ms",
                    (i + 1) * 500
                );
                return Ok(());
            }
        }

        Err(format!(
            "Sidecar health check failed after {}s",
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
            .map_err(|e| format!("Failed to serialize sidecar metadata: {}", e))?;

        std::fs::write(&self.runtime_path, raw)
            .map_err(|e| format!("Failed to write sidecar metadata: {}", e))
    }

    fn load_runtime_metadata(&self) -> Result<Option<SidecarRuntimeMetadata>, String> {
        match std::fs::read(&self.runtime_path) {
            Ok(raw) => serde_json::from_slice(&raw)
                .map(Some)
                .map_err(|e| format!("Failed to parse sidecar metadata: {}", e)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(format!("Failed to read sidecar metadata: {}", err)),
        }
    }

    fn reap_stale_managed_sidecar(&self) -> Result<(), String> {
        let metadata = match self.load_runtime_metadata() {
            Ok(Some(metadata)) => metadata,
            Ok(None) => return Ok(()),
            Err(err) => {
                eprintln!(
                    "[AugmentSidecar] Ignoring unreadable sidecar metadata at {}: {}",
                    self.runtime_path.display(),
                    err
                );
                let _ = std::fs::remove_file(&self.runtime_path);
                return Ok(());
            }
        };

        let mut system = System::new();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::new());

        let pid = sysinfo::Pid::from_u32(metadata.pid);
        let matched_process = system.process(pid).is_some_and(|process| {
            let process_name = process.name().to_string_lossy().into_owned();
            let exe_path = process.exe();
            let cmd = process
                .cmd()
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();

            process_matches_runtime_metadata(&process_name, exe_path, &cmd, &metadata)
        });

        if matched_process {
            println!(
                "[AugmentSidecar] Reaping stale managed sidecar PID {}",
                metadata.pid
            );

            let process = system
                .process(pid)
                .ok_or_else(|| "Stale sidecar process disappeared during cleanup".to_string())?;
            if !process.kill() {
                return Err(format!(
                    "Failed to kill stale managed sidecar PID {}",
                    metadata.pid
                ));
            }

            for _ in 0..10 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                system.refresh_processes_specifics(
                    ProcessesToUpdate::All,
                    true,
                    ProcessRefreshKind::new(),
                );
                if system.process(pid).is_none() {
                    break;
                }
            }

            if system.process(pid).is_some() {
                return Err(format!(
                    "Stale managed sidecar PID {} is still alive after kill",
                    metadata.pid
                ));
            }
        }

        let _ = std::fs::remove_file(&self.runtime_path);
        Ok(())
    }
}

impl Drop for AugmentSidecar {
    fn drop(&mut self) {
        self.force_stop();
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
        port, auth_dir, api_key, api_key,
    )
}

fn build_auth_file(token: &TokenData) -> Result<(String, String), String> {
    if token.access_token.trim().is_empty() {
        return Err("Missing access_token".to_string());
    }

    let tenant_url =
        normalized_tenant_url(&token.tenant_url).ok_or_else(|| "Missing tenant_url".to_string())?;
    let label =
        auth_label_for_tenant(&tenant_url).ok_or_else(|| "Invalid tenant_url".to_string())?;
    let filename =
        auth_filename_for_tenant(&tenant_url).ok_or_else(|| "Invalid tenant_url".to_string())?;
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

fn sidecar_process_env(home_dir: &std::path::Path) -> Vec<(&'static str, PathBuf)> {
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

fn split_windows_home_components(home_dir: &std::path::Path) -> Option<(String, String)> {
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

fn process_matches_runtime_metadata(
    process_name: &str,
    exe_path: Option<&std::path::Path>,
    cmd: &[String],
    metadata: &SidecarRuntimeMetadata,
) -> bool {
    binary_identity_matches(process_name, exe_path, &metadata.binary_path)
        && command_uses_config_path(cmd, &metadata.config_path)
}

fn binary_identity_matches(
    process_name: &str,
    exe_path: Option<&std::path::Path>,
    binary_path: &std::path::Path,
) -> bool {
    let Some(binary_name) = binary_path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    let process_name = process_name.to_ascii_lowercase();
    let binary_name = binary_name.to_ascii_lowercase();
    let name_matches = process_name == binary_name || process_name.contains(&binary_name);

    let exe_matches = exe_path.is_some_and(|exe_path| exe_path == binary_path);

    name_matches || exe_matches
}

fn command_uses_config_path(cmd: &[String], config_path: &std::path::Path) -> bool {
    let expected = config_path.to_string_lossy();

    cmd.windows(2).any(|window| {
        window.first().is_some_and(|arg| arg == "-config")
            && window
                .get(1)
                .is_some_and(|value| value == expected.as_ref())
    }) || cmd.iter().any(|arg| {
        arg.strip_prefix("-config=")
            .is_some_and(|value| value == expected.as_ref())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use serde_json::{Value, json};
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    #[cfg(unix)]
    use std::process::Stdio;
    use tempfile::tempdir;
    #[cfg(unix)]
    use tokio::process::Command;

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
        let mut sidecar =
            AugmentSidecar::new(temp_dir.path(), PathBuf::from("/tmp/cliproxy-server"));
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
    fn sidecar_uses_isolated_home_dir_under_app_data() {
        let temp_dir = tempdir().unwrap();
        let sidecar = AugmentSidecar::new(temp_dir.path(), PathBuf::from("/tmp/cliproxy-server"));

        assert_eq!(sidecar.home_dir, temp_dir.path().join("cliproxy_home"));
    }

    #[test]
    fn sidecar_process_env_overrides_home_with_isolated_dir() {
        let temp_dir = tempdir().unwrap();
        let sidecar = AugmentSidecar::new(temp_dir.path(), PathBuf::from("/tmp/cliproxy-server"));

        let env = sidecar_process_env(&sidecar.home_dir);

        assert_eq!(
            env.iter()
                .find(|(key, _)| *key == "HOME")
                .map(|(_, value)| value.as_path()),
            Some(sidecar.home_dir.as_path())
        );
    }

    #[test]
    fn sidecar_process_env_sets_userprofile_to_isolated_dir() {
        let temp_dir = tempdir().unwrap();
        let sidecar = AugmentSidecar::new(temp_dir.path(), PathBuf::from("/tmp/cliproxy-server"));

        let env = sidecar_process_env(&sidecar.home_dir);

        assert_eq!(
            env.iter()
                .find(|(key, _)| *key == "USERPROFILE")
                .map(|(_, value)| value.as_path()),
            Some(sidecar.home_dir.as_path())
        );
    }

    #[test]
    fn sidecar_process_env_sets_windows_home_components_when_possible() {
        let env = sidecar_process_env(Path::new(r"C:\Users\me\AppData\Roaming\ATM\cliproxy_home"));

        assert_eq!(
            env.iter()
                .find(|(key, _)| *key == "HOMEDRIVE")
                .map(|(_, value)| value.to_string_lossy().into_owned()),
            Some("C:".to_string())
        );
        assert_eq!(
            env.iter()
                .find(|(key, _)| *key == "HOMEPATH")
                .map(|(_, value)| value.to_string_lossy().into_owned()),
            Some(r"\Users\me\AppData\Roaming\ATM\cliproxy_home".to_string())
        );
    }

    #[test]
    fn models_payload_is_not_ready_when_model_list_is_empty() {
        assert!(!models_payload_is_ready(br#"{"data":[],"object":"list"}"#));
    }

    #[test]
    fn models_payload_is_ready_when_model_list_is_present() {
        assert!(models_payload_is_ready(
            br#"{"data":[{"id":"gpt-5","object":"model"}],"object":"list"}"#
        ));
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

    #[test]
    fn runtime_metadata_round_trips_through_json() {
        let metadata = SidecarRuntimeMetadata {
            pid: 43123,
            config_path: PathBuf::from("/tmp/cliproxy_config.yaml"),
            binary_path: PathBuf::from("/tmp/cliproxy-server"),
        };

        let raw = serde_json::to_string(&metadata).unwrap();
        let restored: SidecarRuntimeMetadata = serde_json::from_str(&raw).unwrap();

        assert_eq!(restored.pid, 43123);
        assert_eq!(
            restored.config_path,
            PathBuf::from("/tmp/cliproxy_config.yaml")
        );
        assert_eq!(restored.binary_path, PathBuf::from("/tmp/cliproxy-server"));
    }

    #[test]
    fn stale_sidecar_process_requires_matching_config_path() {
        let metadata = SidecarRuntimeMetadata {
            pid: 7,
            config_path: PathBuf::from("/tmp/cliproxy_config.yaml"),
            binary_path: PathBuf::from("/tmp/cliproxy-server"),
        };

        assert!(!process_matches_runtime_metadata(
            "cliproxy-server",
            Some(Path::new("/tmp/cliproxy-server")),
            &[
                "/tmp/cliproxy-server".to_string(),
                "-config".to_string(),
                "/tmp/other.yaml".to_string(),
            ],
            &metadata,
        ));
    }

    #[test]
    fn stale_sidecar_process_requires_matching_binary_identity() {
        let metadata = SidecarRuntimeMetadata {
            pid: 7,
            config_path: PathBuf::from("/tmp/cliproxy_config.yaml"),
            binary_path: PathBuf::from("/tmp/cliproxy-server"),
        };

        assert!(!process_matches_runtime_metadata(
            "python3",
            Some(Path::new("/usr/bin/python3")),
            &[
                "python3".to_string(),
                "-m".to_string(),
                "http.server".to_string(),
                "-config".to_string(),
                "/tmp/cliproxy_config.yaml".to_string(),
            ],
            &metadata,
        ));
    }

    #[test]
    fn stale_sidecar_process_matches_expected_binary_and_config() {
        let metadata = SidecarRuntimeMetadata {
            pid: 7,
            config_path: PathBuf::from("/tmp/cliproxy_config.yaml"),
            binary_path: PathBuf::from("/tmp/cliproxy-server"),
        };

        assert!(process_matches_runtime_metadata(
            "cliproxy-server",
            Some(Path::new("/tmp/cliproxy-server")),
            &[
                "/tmp/cliproxy-server".to_string(),
                "-config".to_string(),
                "/tmp/cliproxy_config.yaml".to_string(),
            ],
            &metadata,
        ));
    }

    #[test]
    fn reap_stale_sidecar_ignores_invalid_runtime_metadata() {
        let temp_dir = tempdir().unwrap();
        let sidecar = AugmentSidecar::new(temp_dir.path(), PathBuf::from("/tmp/cliproxy-server"));

        fs::write(&sidecar.runtime_path, b"{not-json").unwrap();

        sidecar.reap_stale_managed_sidecar().unwrap();

        assert!(!sidecar.runtime_path.exists());
    }

    #[cfg(unix)]
    fn write_fake_sidecar_binary(dir: &Path) -> PathBuf {
        let binary_path = dir.join("fake-cliproxy-server.sh");
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
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

port = int(sys.argv[1])

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/v1/models"):
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"data":[{"id":"auggie-test"}]}')
        else:
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
    #[tokio::test]
    async fn sidecar_stop_removes_runtime_artifacts_and_child() {
        let temp_dir = tempdir().unwrap();
        let mut sidecar =
            AugmentSidecar::new(temp_dir.path(), PathBuf::from("/tmp/cliproxy-server"));

        fs::create_dir_all(&sidecar.auth_dir).unwrap();
        fs::create_dir_all(&sidecar.home_dir).unwrap();
        fs::write(sidecar.auth_dir.join("auggie-test.json"), "{}").unwrap();
        fs::write(&sidecar.config_path, "port: 12345").unwrap();

        sidecar.child = Some(
            Command::new("/bin/sh")
                .arg("-c")
                .arg("sleep 30")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );

        assert!(sidecar.is_running());

        sidecar.stop().await;

        assert!(!sidecar.is_running());
        assert!(!sidecar.auth_dir.exists());
        assert!(!sidecar.home_dir.exists());
        assert!(!sidecar.config_path.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sidecar_force_stop_removes_runtime_artifacts_and_child() {
        let temp_dir = tempdir().unwrap();
        let mut sidecar =
            AugmentSidecar::new(temp_dir.path(), PathBuf::from("/tmp/cliproxy-server"));

        fs::create_dir_all(&sidecar.auth_dir).unwrap();
        fs::create_dir_all(&sidecar.home_dir).unwrap();
        fs::write(sidecar.auth_dir.join("auggie-test.json"), "{}").unwrap();
        fs::write(&sidecar.config_path, "port: 12345").unwrap();

        sidecar.child = Some(
            Command::new("/bin/sh")
                .arg("-c")
                .arg("sleep 30")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );

        assert!(sidecar.is_running());

        sidecar.force_stop();

        assert!(!sidecar.is_running());
        assert!(!sidecar.auth_dir.exists());
        assert!(!sidecar.home_dir.exists());
        assert!(!sidecar.config_path.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sidecar_force_stop_reaps_child_process_before_returning() {
        let temp_dir = tempdir().unwrap();
        let mut sidecar =
            AugmentSidecar::new(temp_dir.path(), PathBuf::from("/tmp/cliproxy-server"));

        sidecar.child = Some(
            Command::new("/bin/sh")
                .arg("-c")
                .arg("sleep 30")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );

        let pid = sidecar.child.as_ref().and_then(|child| child.id()).unwrap();

        sidecar.force_stop();

        let mut system = System::new();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::new());

        assert!(
            system.process(sysinfo::Pid::from_u32(pid)).is_none(),
            "sidecar child PID {} should be gone after force_stop",
            pid
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ensure_running_restarts_alive_but_unhealthy_child() {
        let temp_dir = tempdir().unwrap();
        let fake_binary = write_fake_sidecar_binary(temp_dir.path());
        let mut sidecar = AugmentSidecar::new(temp_dir.path(), fake_binary);
        let tokens = vec![sample_token("https://tenant.augmentcode.com/", "token-1")];

        sidecar.port = find_available_port().unwrap();
        let stale_port = sidecar.port;
        sidecar.child = Some(
            Command::new("/bin/sh")
                .arg("-c")
                .arg("sleep 30")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );

        sidecar.ensure_running(&tokens).await.unwrap();

        assert_ne!(sidecar.port, stale_port);
        assert!(sidecar.is_healthy().await);

        sidecar.stop().await;
    }
}
