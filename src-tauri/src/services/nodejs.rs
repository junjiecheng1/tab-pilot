// Node.js 服务 — JavaScript 代码执行
//
// 对应 Python app/services/nodejs.py
// 通过 tokio::process::Command 执行 JS 代码

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;

use serde_json::{json, Value};
use tokio::process::Command;

use crate::core::error::{ServiceError, ServiceResult};

/// Node 版本别名
fn resolve_version(version: Option<&str>) -> String {
    let default = std::env::var("NODE_CODE_EXEC_VERSION").unwrap_or_else(|_| "node22".into());
    let v = version.unwrap_or(&default);
    match v.to_lowercase().as_str() {
        "node" | "node22" | "22" | "" => "node22".into(),
        "node20" | "20" => "node20".into(),
        "node24" | "24" => "node24".into(),
        other => other.to_string(),
    }
}

/// 查找 node 二进制
fn find_node_binary(version: &str) -> String {
    let version_path = format!("/usr/local/bin/{version}");
    if std::path::Path::new(&version_path).exists() {
        return version_path;
    }
    // Fallback
    if let Ok(p) = which::which("node") {
        return p.to_string_lossy().to_string();
    }
    "node".to_string()
}

/// Node.js 服务
pub struct NodeJsService {
    runtime_dir: Option<PathBuf>,
}

impl NodeJsService {
    pub fn new() -> Self {
        // 查找 runtime 目录
        let runtime_dir = ["/opt/runtime/nodejs"]
            .iter()
            .map(PathBuf::from)
            .find(|p| p.exists());

        Self { runtime_dir }
    }

    /// 一次性执行 JS 代码
    pub async fn execute_code(
        &self,
        code: &str,
        timeout_secs: u64,
        cwd: Option<&str>,
        version: Option<&str>,
        files: Option<&HashMap<String, String>>,
    ) -> ServiceResult {
        let resolved = resolve_version(version);
        let binary = find_node_binary(&resolved);

        // 创建临时目录
        let tmp_dir = tempfile::tempdir()
            .map_err(|e| ServiceError::internal(format!("创建临时目录失败: {e}")))?;
        let work_dir = cwd.unwrap_or(tmp_dir.path().to_str().unwrap_or("/tmp"));

        // 链接 node_modules
        if let Some(ref rt) = self.runtime_dir {
            let nm_src = rt.join("node_modules");
            let nm_dst = tmp_dir.path().join("node_modules");
            if nm_src.exists() && !nm_dst.exists() {
                #[cfg(unix)]
                let _ = std::os::unix::fs::symlink(&nm_src, &nm_dst);
                
                #[cfg(windows)]
                let _ = std::os::windows::fs::symlink_dir(&nm_src, &nm_dst);
            }
        }

        // 写入额外文件
        if let Some(files) = files {
            for (name, content) in files {
                let file_path = tmp_dir.path().join(name);
                if let Some(parent) = file_path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                let _ = tokio::fs::write(&file_path, content).await;
            }
        }

        // 写入代码文件
        let code_file = tmp_dir.path().join("__code__.js");
        tokio::fs::write(&code_file, code)
            .await
            .map_err(|e| ServiceError::internal(format!("写入代码失败: {e}")))?;

        // 准备环境变量
        let mut env_vars: HashMap<String, String> = std::env::vars().collect();
        if let Some(ref rt) = self.runtime_dir {
            let nm = rt.join("node_modules").to_string_lossy().to_string();
            let existing = env_vars.get("NODE_PATH").cloned().unwrap_or_default();
            env_vars.insert(
                "NODE_PATH".into(),
                if existing.is_empty() { nm } else { format!("{nm}:{existing}") },
            );
        }

        let mut cmd = Command::new(&binary);
        cmd.arg(&code_file)
            .current_dir(work_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .envs(&env_vars);

        let child = cmd
            .spawn()
            .map_err(|e| ServiceError::internal(format!("Node.js 启动失败: {e}")))?;

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            child.wait_with_output(),
        )
        .await
        .map_err(|_| ServiceError::timeout(format!("执行超时: {timeout_secs}s")))?
        .map_err(|e| ServiceError::internal(format!("执行失败: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        let status = match exit_code {
            0 => "ok",
            -1 => "timeout",
            _ => "error",
        };

        let mut outputs = vec![];
        if !stdout.is_empty() {
            outputs.push(json!({"output_type": "stream", "name": "stdout", "text": stdout}));
        }
        if !stderr.is_empty() {
            outputs.push(json!({"output_type": "stream", "name": "stderr", "text": stderr}));
        }
        if exit_code != 0 {
            outputs.push(json!({
                "output_type": "error",
                "ename": "RuntimeError",
                "evalue": format!("exit code {exit_code}"),
                "traceback": stderr.lines().collect::<Vec<_>>(),
            }));
        }

        Ok(json!({
            "language": "javascript",
            "status": status,
            "outputs": outputs,
            "code": code,
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": exit_code,
            "version": resolved,
        }))
    }

    /// REPL 方式执行 (通过 HTTP 转发到 REPL server)
    pub async fn execute_stateful(
        &self,
        code: &str,
        timeout_secs: u64,
        session_id: Option<&str>,
        cwd: Option<&str>,
        version: Option<&str>,
    ) -> ServiceResult {
        let resolved = resolve_version(version);
        let port = match resolved.as_str() {
            "node20" => 8192,
            "node22" => 8292,
            "node24" => 8392,
            _ => 8292,
        };
        let url = format!("http://localhost:{port}/execute");

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .json(&json!({
                "code": code,
                "session_id": session_id,
                "timeout": timeout_secs * 1000 + 100,
                "cwd": cwd,
            }))
            .timeout(std::time::Duration::from_secs(timeout_secs + 2))
            .send()
            .await
            .map_err(|e| ServiceError::unavailable(format!("REPL server 不可用: {e}")))?;

        let result: Value = response
            .json()
            .await
            .map_err(|e| ServiceError::internal(format!("REPL 响应解析失败: {e}")))?;

        let success = result["success"].as_bool().unwrap_or(false);
        let stdout = result["stdout"].as_str().unwrap_or("");
        let stderr = result["stderr"].as_str().unwrap_or("");

        let mut outputs = vec![];
        if !stdout.is_empty() {
            outputs.push(json!({"output_type": "stream", "name": "stdout", "text": stdout}));
        }
        if !stderr.is_empty() {
            outputs.push(json!({"output_type": "stream", "name": "stderr", "text": stderr}));
        }

        let status = if success { "ok" } else { "error" };
        if !success {
            if let Some(error) = result.get("error") {
                outputs.push(json!({
                    "output_type": "error",
                    "ename": error["name"].as_str().unwrap_or("Error"),
                    "evalue": error["message"].as_str().unwrap_or(""),
                }));
            }
        }

        Ok(json!({
            "language": "javascript",
            "status": status,
            "outputs": outputs,
            "code": code,
            "session_id": session_id,
            "version": resolved,
        }))
    }

    /// 获取运行时信息
    pub fn get_runtime_info(&self, version: Option<&str>) -> ServiceResult {
        let resolved = resolve_version(version);
        let binary = find_node_binary(&resolved);

        let node_version = std::process::Command::new(&binary)
            .arg("--version")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "unknown".into());

        Ok(json!({
            "node_version": node_version,
            "resolved_version": resolved,
            "binary": binary,
            "runtime_dir": self.runtime_dir.as_ref().map(|p| p.to_string_lossy().to_string()),
            "languages": ["javascript"],
        }))
    }

    /// WS handler
    pub async fn handle(&self, action: &str, params: Value) -> ServiceResult {
        match action {
            "execute" => {
                let code = params["code"]
                    .as_str()
                    .ok_or_else(|| ServiceError::bad_request("缺少 code"))?;
                let timeout = params["timeout"].as_u64().unwrap_or(30);
                let cwd = params["cwd"].as_str();
                let version = params["version"].as_str();
                let files: Option<HashMap<String, String>> = params
                    .get("files")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                self.execute_code(code, timeout, cwd, version, files.as_ref())
                    .await
            }
            "execute_stateful" | "repl" => {
                let code = params["code"]
                    .as_str()
                    .ok_or_else(|| ServiceError::bad_request("缺少 code"))?;
                let timeout = params["timeout"].as_u64().unwrap_or(30);
                let sid = params["session_id"].as_str();
                let cwd = params["cwd"].as_str();
                let version = params["version"].as_str();
                self.execute_stateful(code, timeout, sid, cwd, version).await
            }
            "info" | "runtime_info" => {
                let version = params["version"].as_str();
                self.get_runtime_info(version)
            }
            _ => Err(ServiceError::bad_request(format!("未知 nodejs 操作: {action}"))),
        }
    }
}
