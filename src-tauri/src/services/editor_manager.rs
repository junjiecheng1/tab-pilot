// Editor Manager 服务 — 编辑器实例管理
//
// 对应 Python app/services/editor_manager.py
// 管理外部编辑器进程 (VSCode, Cursor 等)

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::core::error::{ServiceError, ServiceResult};

/// 编辑器类型
#[derive(Debug, Clone)]
pub enum EditorType {
    VSCode,
    Cursor,
    Zed,
    Other(String),
}

impl EditorType {
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "vscode" | "code" => Self::VSCode,
            "cursor" => Self::Cursor,
            "zed" => Self::Zed,
            other => Self::Other(other.to_string()),
        }
    }

    fn command(&self) -> &str {
        match self {
            Self::VSCode => "code",
            Self::Cursor => "cursor",
            Self::Zed => "zed",
            Self::Other(cmd) => cmd,
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::VSCode => "VSCode",
            Self::Cursor => "Cursor",
            Self::Zed => "Zed",
            Self::Other(n) => n,
        }
    }
}

/// 编辑器实例记录
struct EditorInstance {
    editor_type: EditorType,
    project_path: PathBuf,
    opened_at: Instant,
    pid: Option<u32>,
}

/// Editor Manager 服务
pub struct EditorManagerService {
    instances: RwLock<HashMap<String, EditorInstance>>,
}

impl EditorManagerService {
    pub fn new() -> Self {
        Self {
            instances: RwLock::new(HashMap::new()),
        }
    }

    /// 用编辑器打开文件/目录
    pub async fn open(
        &self,
        path: &str,
        editor: Option<&str>,
        line: Option<u32>,
        column: Option<u32>,
    ) -> ServiceResult {
        let editor_type = EditorType::from_str(editor.unwrap_or("code"));
        let cmd_name = editor_type.command();

        let mut args: Vec<String> = vec![];

        // 如果指定了行号，用 file:line:column 格式
        if let Some(l) = line {
            let col = column.unwrap_or(1);
            args.push("--goto".to_string());
            args.push(format!("{path}:{l}:{col}"));
        } else {
            args.push(path.to_string());
        }

        let child = tokio::process::Command::new(cmd_name)
            .args(&args)
            .spawn()
            .map_err(|e| {
                ServiceError::unavailable(format!("{} 启动失败: {e}", editor_type.name()))
            })?;

        let pid = child.id();
        let instance_id = uuid::Uuid::new_v4().to_string();

        self.instances.write().await.insert(
            instance_id.clone(),
            EditorInstance {
                editor_type: editor_type.clone(),
                project_path: PathBuf::from(path),
                opened_at: Instant::now(),
                pid,
            },
        );

        Ok(json!({
            "instance_id": instance_id,
            "editor": editor_type.name(),
            "path": path,
            "pid": pid,
        }))
    }

    /// 列出编辑器实例
    pub async fn list_instances(&self) -> ServiceResult {
        let instances = self.instances.read().await;
        let list: Vec<Value> = instances
            .iter()
            .map(|(id, inst)| {
                json!({
                    "instance_id": id,
                    "editor": inst.editor_type.name(),
                    "project_path": inst.project_path.to_string_lossy(),
                    "age_secs": inst.opened_at.elapsed().as_secs(),
                    "pid": inst.pid,
                })
            })
            .collect();
        Ok(json!({"instances": list}))
    }

    /// WS handler
    pub async fn handle(&self, action: &str, params: Value) -> ServiceResult {
        match action {
            "open" => {
                let path = params["path"]
                    .as_str()
                    .ok_or_else(|| ServiceError::bad_request("缺少 path"))?;
                let editor = params["editor"].as_str();
                let line = params["line"].as_u64().map(|v| v as u32);
                let column = params["column"].as_u64().map(|v| v as u32);
                self.open(path, editor, line, column).await
            }
            "list" => self.list_instances().await,
            _ => Err(ServiceError::bad_request(format!(
                "未知 editor 操作: {action}"
            ))),
        }
    }
}
