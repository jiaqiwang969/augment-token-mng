//! CLIProxyAPI sidecar 管理器
//!
//! 管理 CLIProxyAPI Go 二进制的生命周期，负责启动、停止、健康检查和 auth 文件同步。

use crate::storage::TokenData;
use serde_json::json;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::{Child, Command};

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

        // 清理旧文件
        if let Ok(entries) = std::fs::read_dir(&self.auth_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().map_or(false, |e| e == "json") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }

        // 写入新的 auth 文件
        for token in tokens {
            if token.access_token.is_empty() || token.tenant_url.is_empty() {
                continue;
            }

            let tenant = token.tenant_url.trim_end_matches('/');
            let label = tenant
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .replace('/', "");
            let filename = format!("auggie-{}.json", label.replace('.', "-"));

            let auth = json!({
                "access_token": token.access_token,
                "client_id": "auggie-cli",
                "disabled": false,
                "label": label,
                "last_refresh": token.updated_at.to_rfc3339(),
                "login_mode": "localhost",
                "scopes": ["read", "write"],
                "tenant_url": format!("{}/", tenant),
                "type": "auggie"
            });

            let path = self.auth_dir.join(&filename);
            std::fs::write(&path, serde_json::to_string(&auth).unwrap_or_default())
                .map_err(|e| format!("Failed to write auth file {}: {}", filename, e))?;
        }

        Ok(())
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
        let config = format!(
            r#"host: "127.0.0.1"
port: {}
auth-dir: "{}"
api-keys:
  - "{}"
client-api-keys:
  - key: "{}"
    enabled: true
    scope:
      provider: auggie
routing-strategy: round-robin
debug: false
request-log: false
remote-management:
  allow-remote: false
  disable-control-panel: true
"#,
            self.port,
            self.auth_dir.display(),
            self.api_key,
            self.api_key,
        );

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
