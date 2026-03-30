// Shell 服务 — PTY 终端管理
//
// 职责分层:
//   mod.rs       → ShellService + WS 路由分派
//   session.rs   → PtySession 创建 / 写入 / 终止
//   exec.rs      → 命令执行 + 轮询等待 + 输出清洗
//   collector.rs → 线程安全输出收集 + prompt 检测

mod collector;
mod exec;
mod session;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::{Mutex, RwLock};

use crate::core::error::{ServiceError, ServiceResult};

use session::ShellSession;

/// Shell 服务
pub struct ShellService {
    sessions: RwLock<HashMap<String, Arc<Mutex<ShellSession>>>>,
    max_sessions: usize,
}

impl ShellService {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            max_sessions: std::env::var("MAX_SHELL_SESSIONS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(20),
        }
    }

    // ── 会话管理 ──────────────────────────────

    /// 创建终端会话
    pub async fn create_session(
        &self,
        session_id: Option<String>,
        shell: Option<String>,
        working_dir: Option<String>,
        environment: Option<HashMap<String, String>>,
    ) -> ServiceResult {
        let sid = session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let shell_cmd = shell.unwrap_or_else(|| {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
        });
        let cwd = PathBuf::from(
            working_dir.unwrap_or_else(|| {
                std::env::var("WORKSPACE")
                    .or_else(|_| std::env::var("HOME"))
                    .unwrap_or_else(|_| "/tmp".to_string())
            }),
        );

        // 检查会话数限制
        {
            let sessions = self.sessions.read().await;
            if sessions.len() >= self.max_sessions {
                return Err(ServiceError::bad_request(format!(
                    "会话数已达上限: {}", self.max_sessions
                )));
            }
        }

        // 在线程池中创建 PTY (阻塞操作)
        let sid_clone = sid.clone();
        let shell_clone = shell_cmd.clone();
        let cwd_clone = cwd.clone();
        let env_clone = environment.clone();

        let session = tokio::task::spawn_blocking(move || {
            ShellSession::create(
                &sid_clone,
                &shell_clone,
                &cwd_clone,
                env_clone.as_ref(),
            )
        })
        .await
        .map_err(|e| ServiceError::internal(format!("PTY 创建 spawn 失败: {e}")))?
        .map_err(|e| ServiceError::internal(e))?;

        let result = json!({
            "session_id": sid,
            "shell": shell_cmd,
            "working_dir": cwd.to_string_lossy(),
        });

        self.sessions
            .write()
            .await
            .insert(sid.clone(), Arc::new(Mutex::new(session)));

        Ok(result)
    }

    /// 列出所有会话
    pub async fn list_sessions(&self) -> ServiceResult {
        let sessions = self.sessions.read().await;
        let mut list = Vec::new();
        for (id, session) in sessions.iter() {
            let s = session.lock().await;
            list.push(json!({
                "session_id": id,
                "shell": s.shell,
                "working_dir": s.working_dir.to_string_lossy(),
                "active": s.active,
                "age_secs": s.created_at.elapsed().as_secs(),
            }));
        }
        Ok(json!({"sessions": list}))
    }

    /// 终止单个会话
    pub async fn kill_session(&self, session_id: &str) -> ServiceResult {
        let session = self.get_session(session_id).await?;
        let mut s = session.lock().await;
        s.kill();
        Ok(json!({"killed": true, "session_id": session_id}))
    }

    /// 查看会话输出
    pub async fn view_session(&self, session_id: &str) -> ServiceResult {
        let session = self.get_session(session_id).await?;
        exec::view_output(session).await
    }

    /// 清理单个会话
    pub async fn cleanup_session(&self, session_id: &str) -> ServiceResult {
        let removed = self.sessions.write().await.remove(session_id);
        if let Some(session) = removed {
            let mut s = session.lock().await;
            s.kill();
            Ok(json!({"cleaned": true, "session_id": session_id}))
        } else {
            Err(ServiceError::not_found(format!("会话不存在: {session_id}")))
        }
    }

    /// 清理所有会话
    pub async fn cleanup_all(&self) -> ServiceResult {
        let mut sessions = self.sessions.write().await;
        let count = sessions.len();
        for (_, session) in sessions.drain() {
            let mut s = session.lock().await;
            s.kill();
        }
        Ok(json!({"cleaned": count}))
    }

    // ── WS 路由 ──────────────────────────────

    pub async fn handle(&self, action: &str, params: Value) -> ServiceResult {
        match action {
            "create_session" => {
                self.create_session(
                    params["session_id"].as_str().map(String::from),
                    params["shell"].as_str().map(String::from),
                    params["working_dir"].as_str().map(String::from),
                    params.get("environment").and_then(|v| serde_json::from_value(v.clone()).ok()),
                ).await
            }
            "exec" => {
                let cmd = req_str(&params, "command")?;
                let timeout = params["timeout"].as_u64();

                // Toolkit 内置命令拦截 (tab-xlsx, tab-pdf, tab-base 等)
                if let Some(result) = super::toolkit_dispatch::try_dispatch(&cmd, None).await {
                    return result;
                }

                // 获取或创建会话
                let session = self.get_or_create_session(&params).await?;
                exec::exec_in_session(session, &cmd, timeout).await
            }
            "view" => {
                let sid = req_str(&params, "session_id")?;
                let session = self.get_session(&sid).await?;
                exec::view_output(session).await
            }
            "write" => {
                let sid = req_str(&params, "session_id")?;
                let text = req_str(&params, "text")?;
                let session = self.get_session(&sid).await?;
                exec::write_text(session, &text).await
            }
            "kill" => {
                let sid = req_str(&params, "session_id")?;
                self.kill_session(&sid).await
            }
            "list_sessions" => self.list_sessions().await,
            "cleanup_session" => {
                let sid = req_str(&params, "session_id")?;
                self.cleanup_session(&sid).await
            }
            "cleanup_all" => self.cleanup_all().await,
            "wait" => {
                let sid = req_str(&params, "session_id")?;
                let seconds = params["seconds"].as_u64().unwrap_or(30);
                let session = self.get_session(&sid).await?;
                exec::wait_session(session, seconds).await
            }
            _ => Err(ServiceError::bad_request(format!("未知 shell 操作: {action}"))),
        }
    }

    // ── 内部 ──────────────────────────────

    async fn get_session(&self, session_id: &str) -> Result<Arc<Mutex<ShellSession>>, ServiceError> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| ServiceError::not_found(format!("会话不存在: {session_id}")))
    }

    /// exec 兼容: 如果没有 session_id, 自动创建临时会话
    async fn get_or_create_session(&self, params: &Value) -> Result<Arc<Mutex<ShellSession>>, ServiceError> {
        if let Some(sid) = params["session_id"].as_str() {
            self.get_session(sid).await
        } else {
            let cwd = params["exec_dir"].as_str()
                .or_else(|| params["cwd"].as_str())
                .map(String::from);
            let result = self.create_session(None, None, cwd, None).await?;
            let sid = result["session_id"]
                .as_str()
                .ok_or_else(|| ServiceError::internal("auto-create session failed"))?;
            self.get_session(sid).await
        }
    }
}

fn req_str(params: &Value, key: &str) -> Result<String, ServiceError> {
    params[key]
        .as_str()
        .map(String::from)
        .ok_or_else(|| ServiceError::bad_request(format!("缺少 {key}")))
}
