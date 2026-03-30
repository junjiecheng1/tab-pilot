// MCP 传输会话 — HTTP + stdio

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::infra::runtime::RuntimeManager;

// ═══════════════════════════════════════════════════════════
// HTTP 会话 (远程 MCP)
// ═══════════════════════════════════════════════════════════

pub(super) struct HttpSession {
    url: String,
    pub(super) session_id: Option<String>,
    client: reqwest::Client,
}

impl HttpSession {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            session_id: None,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    /// JSON-RPC 请求
    async fn rpc(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let mut req = self.client.post(&self.url)
            .header("Content-Type", "application/json");

        if let Some(ref sid) = self.session_id {
            req = req.header("Mcp-Session-Id", sid);
        }

        let resp = req.json(&body).send().await.map_err(|e| e.to_string())?;

        if let Some(sid) = resp.headers().get("mcp-session-id") {
            self.session_id = sid.to_str().ok().map(|s| s.to_string());
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

        if let Some(error) = json.get("error") {
            return Err(format!("MCP error: {}", error));
        }

        Ok(json.get("result").cloned().unwrap_or(serde_json::Value::Null))
    }

    pub async fn initialize(&mut self) -> Result<(), String> {
        self.rpc("initialize", serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "TabPilot", "version": "0.2.0" }
        })).await?;
        Ok(())
    }

    pub async fn list_tools(&mut self) -> Result<Vec<serde_json::Value>, String> {
        let result = self.rpc("tools/list", serde_json::json!({})).await?;
        Ok(result.get("tools").and_then(|t| t.as_array()).cloned().unwrap_or_default())
    }

    pub async fn call_tool(
        &mut self, name: &str, args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.rpc("tools/call", serde_json::json!({ "name": name, "arguments": args })).await
    }
}

// ═══════════════════════════════════════════════════════════
// stdio 会话 (本地 MCP)
// ═══════════════════════════════════════════════════════════

pub(super) struct StdioSession {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: AtomicU64,
}

impl StdioSession {
    /// 启动 MCP server 子进程
    pub async fn spawn(
        runtime: &RuntimeManager,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self, String> {
        let cmd = match command {
            "npx" => runtime.npx_bin(),
            "node" => runtime.node_bin(),
            other => PathBuf::from(other),
        };

        let path_dir = runtime.node_bin();
        let path_dir = path_dir.parent().unwrap_or(Path::new(""));

        let mut child = Command::new(&cmd)
            .args(args)
            .envs(env)
            .env("PATH", path_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("启动 MCP 服务器 {:?} 失败: {e}", cmd))?;

        let stdin = child.stdin.take().ok_or("获取 MCP stdin 失败".to_string())?;
        let stdout = child.stdout.take().ok_or("获取 MCP stdout 失败".to_string())?;

        Ok(Self {
            _child: child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: AtomicU64::new(1),
        })
    }

    /// JSON-RPC over stdio
    async fn rpc(
        &mut self, method: &str, params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params,
        });

        let line = format!("{}\n", msg);
        self.stdin.write_all(line.as_bytes()).await
            .map_err(|e| format!("写入 MCP 失败: {e}"))?;
        self.stdin.flush().await
            .map_err(|e| format!("flush MCP 失败: {e}"))?;

        let mut resp_line = String::new();
        self.stdout.read_line(&mut resp_line).await
            .map_err(|e| format!("读取 MCP 失败: {e}"))?;

        if resp_line.is_empty() {
            return Err("MCP 进程已退出".to_string());
        }

        let resp: serde_json::Value = serde_json::from_str(&resp_line)
            .map_err(|e| format!("解析 MCP 响应失败: {e}"))?;

        if let Some(error) = resp.get("error") {
            return Err(format!("MCP: {}", error));
        }

        Ok(resp.get("result").cloned().unwrap_or(serde_json::Value::Null))
    }

    pub async fn initialize(&mut self) -> Result<(), String> {
        self.rpc("initialize", serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "TabPilot", "version": "0.2.0" }
        })).await?;
        Ok(())
    }

    pub async fn list_tools(&mut self) -> Result<Vec<serde_json::Value>, String> {
        let result = self.rpc("tools/list", serde_json::json!({})).await?;
        Ok(result.get("tools").and_then(|t| t.as_array()).cloned().unwrap_or_default())
    }

    pub async fn call_tool(
        &mut self, name: &str, args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.rpc("tools/call", serde_json::json!({ "name": name, "arguments": args })).await
    }
}
