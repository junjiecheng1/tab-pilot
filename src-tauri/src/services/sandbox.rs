// Sandbox 服务 — 沙箱环境信息
//
// 对应 Python app/services/sandbox.py
// 提供运行时上下文、包列表、权限等

use serde_json::{json, Value};

use crate::core::error::{ServiceError, ServiceResult};

/// Sandbox 服务
pub struct SandboxService {
    workspace: String,
}

impl SandboxService {
    pub fn new() -> Self {
        Self {
            workspace: std::env::var("WORKSPACE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| "/tmp".to_string()),
        }
    }

    /// 获取沙箱上下文
    pub async fn get_context(&self) -> ServiceResult {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".into());

        Ok(json!({
            "workspace": self.workspace,
            "hostname": hostname,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "platform": "tabpilot",
        }))
    }

    /// 获取 Python 包列表
    pub async fn get_python_packages(&self) -> ServiceResult {
        let output = tokio::process::Command::new("pip")
            .args(["list", "--format=json"])
            .output()
            .await;

        match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let packages: Value = serde_json::from_str(&stdout).unwrap_or(json!([]));
                Ok(json!({"packages": packages}))
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Ok(json!({"packages": [], "error": stderr.to_string()}))
            }
            Err(e) => Ok(json!({"packages": [], "error": format!("pip 不可用: {e}")})),
        }
    }

    /// 获取 Node.js 包列表
    pub async fn get_node_packages(&self) -> ServiceResult {
        let output = tokio::process::Command::new("npm")
            .args(["ls", "--json", "--depth=0"])
            .output()
            .await;

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let packages: Value = serde_json::from_str(&stdout).unwrap_or(json!({}));
                Ok(json!({"packages": packages}))
            }
            Err(e) => Ok(json!({"packages": {}, "error": format!("npm 不可用: {e}")})),
        }
    }

    /// 获取工作区信息
    pub async fn workspace(&self) -> ServiceResult {
        let meta = tokio::fs::metadata(&self.workspace).await;
        Ok(json!({
            "path": self.workspace,
            "exists": meta.is_ok(),
            "is_dir": meta.map(|m| m.is_dir()).unwrap_or(false),
        }))
    }

    /// 获取环境变量 (过滤敏感信息)
    pub async fn env(&self) -> ServiceResult {
        let sensitive = [
            "SECRET", "TOKEN", "API_KEY", "PASSWORD", "PRIVATE",
            "CREDENTIAL", "AUTH",
        ];

        let filtered: Value = std::env::vars()
            .filter(|(k, _)| {
                let upper = k.to_uppercase();
                !sensitive.iter().any(|s| upper.contains(s))
            })
            .map(|(k, v)| (k, json!(v)))
            .collect::<serde_json::Map<String, Value>>()
            .into();

        Ok(json!({"env": filtered}))
    }

    /// 获取权限信息
    pub async fn permissions(&self) -> ServiceResult {
        // TabPilot 本地模式: 完全权限
        Ok(json!({
            "mode": "local",
            "platform": "tabpilot",
            "file_read": true,
            "file_write": true,
            "network": true,
            "shell": true,
            "browser": true,
        }))
    }

    /// WS handler
    pub async fn handle(&self, action: &str, _params: Value) -> ServiceResult {
        match action {
            "context" | "status" => self.get_context().await,
            "packages_python" => self.get_python_packages().await,
            "packages_nodejs" | "packages_node" => self.get_node_packages().await,
            "workspace" => self.workspace().await,
            "env" => self.env().await,
            "permissions" => self.permissions().await,
            _ => Err(ServiceError::bad_request(format!("未知 sandbox 操作: {action}"))),
        }
    }
}
