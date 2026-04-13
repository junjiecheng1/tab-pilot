// Jupyter 服务 — Jupyter Kernel 代码执行
//
// 对应 Python app/services/jupyter.py
// 通过 tokio::process::Command 管理 jupyter kernel

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;

use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::RwLock;

use crate::core::error::{ServiceError, ServiceResult};

/// Kernel 别名映射
fn kernel_alias(name: &str) -> &str {
    match name {
        "python" | "python3" | "" => "python3",
        "python3.10" => "python3.10",
        "python3.11" => "python3.11",
        "python3.12" => "python3.12",
        other => other,
    }
}

/// Jupyter 会话
struct JupyterSession {
    kernel_name: String,
    last_used: Instant,
    execution_count: u32,
}

/// Jupyter 服务
pub struct JupyterService {
    sessions: RwLock<HashMap<String, JupyterSession>>,
    max_sessions: usize,
    session_timeout_secs: u64,
    default_kernel: String,
}

impl JupyterService {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            max_sessions: 20,
            session_timeout_secs: 1800,
            default_kernel: std::env::var("PYTHON_CODE_EXEC_VERSION")
                .unwrap_or_else(|_| "python3".to_string()),
        }
    }

    /// 执行代码
    pub async fn execute_code(
        &self,
        code: &str,
        timeout_secs: u64,
        kernel_name: Option<&str>,
        session_id: Option<&str>,
        cwd: Option<&str>,
    ) -> ServiceResult {
        let kernel = kernel_alias(kernel_name.unwrap_or(&self.default_kernel));
        let sid = session_id
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // 查找 Python 解释器
        let python = Self::find_python(kernel);

        // 在临时文件中写入代码并用 Python 执行
        let tmp_dir = tempfile::tempdir()
            .map_err(|e| ServiceError::internal(format!("创建临时目录失败: {e}")))?;

        let code_file = tmp_dir.path().join("__code__.py");
        tokio::fs::write(&code_file, code)
            .await
            .map_err(|e| ServiceError::internal(format!("写入代码失败: {e}")))?;

        let work_dir = cwd.unwrap_or(".");
        let mut cmd = Command::new(&python);
        cmd.arg(&code_file)
            .current_dir(work_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd
            .spawn()
            .map_err(|e| ServiceError::internal(format!("Python 启动失败: {e}")))?;

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

        let status = if exit_code == 0 { "ok" } else { "error" };

        // 更新会话
        {
            let mut sessions = self.sessions.write().await;
            let session = sessions
                .entry(sid.clone())
                .or_insert_with(|| JupyterSession {
                    kernel_name: kernel.to_string(),
                    last_used: Instant::now(),
                    execution_count: 0,
                });
            session.last_used = Instant::now();
            session.execution_count += 1;
        }

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
            "kernel_name": kernel,
            "session_id": sid,
            "status": status,
            "outputs": outputs,
            "code": code,
            "exit_code": exit_code,
            "stdout": stdout,
            "stderr": stderr,
        }))
    }

    /// 获取活跃会话
    pub async fn get_sessions(&self) -> ServiceResult {
        let sessions = self.sessions.read().await;
        let list: Vec<Value> = sessions
            .iter()
            .map(|(id, s)| {
                json!({
                    "session_id": id,
                    "kernel_name": s.kernel_name,
                    "execution_count": s.execution_count,
                    "age_secs": s.last_used.elapsed().as_secs(),
                })
            })
            .collect();
        Ok(json!({"sessions": list}))
    }

    /// 清理会话
    pub async fn cleanup_session(&self, session_id: &str) -> ServiceResult {
        let removed = self.sessions.write().await.remove(session_id);
        Ok(json!({
            "cleaned": removed.is_some(),
            "session_id": session_id,
        }))
    }

    /// 清理所有会话
    pub async fn cleanup_all(&self) -> ServiceResult {
        let mut sessions = self.sessions.write().await;
        let count = sessions.len();
        sessions.clear();
        Ok(json!({"cleaned": count}))
    }

    /// WS handler
    pub async fn handle(&self, action: &str, params: Value) -> ServiceResult {
        match action {
            "execute" => {
                let code = params["code"]
                    .as_str()
                    .ok_or_else(|| ServiceError::bad_request("缺少 code"))?;
                let timeout = params["timeout"].as_u64().unwrap_or(30);
                let kernel = params["kernel_name"].as_str();
                let sid = params["session_id"].as_str();
                let cwd = params["cwd"].as_str();
                self.execute_code(code, timeout, kernel, sid, cwd).await
            }
            "sessions" | "list_sessions" => self.get_sessions().await,
            "cleanup_session" => {
                let sid = params["session_id"]
                    .as_str()
                    .ok_or_else(|| ServiceError::bad_request("缺少 session_id"))?;
                self.cleanup_session(sid).await
            }
            "cleanup_all" => self.cleanup_all().await,
            _ => Err(ServiceError::bad_request(format!(
                "未知 jupyter 操作: {action}"
            ))),
        }
    }

    // ── 内部 ──────────────────────────────────

    /// 查找 Python 解释器
    fn find_python(kernel: &str) -> String {
        // 优先检查版本特定二进制
        let candidates: Vec<String> = match kernel {
            "python3.10" => vec!["python3.10".into(), "python3".into(), "python".into()],
            "python3.11" => vec!["python3.11".into(), "python3".into(), "python".into()],
            "python3.12" => vec!["python3.12".into(), "python3".into(), "python".into()],
            _ => vec!["python3".into(), "python".into()],
        };

        for c in candidates {
            if which::which(&c).is_ok() {
                return c;
            }
        }

        "python3".to_string()
    }
}
