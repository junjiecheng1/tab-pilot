// 消息路由 — 统一分发到 AppServices
//
// 所有请求通过 app/* 命名空间路由:
//   app/shell.*       → ShellService (guard + audit)
//   app/file.*        → FileService  (guard + audit)
//   app/browser.*     → BrowserService (audit)
//   app/sandbox.*     → SandboxService
//   app/mcp_client.*  → McpClient
//   app/skills.*      → SkillService
//   app/editor.*      → EditorManagerService
//   ...
//
// runtime/* 兼容转发到 app/* (过渡期)
//
// 本地日志: 所有 WS 请求和响应均记录到 log (info/warn)

use std::sync::Arc;
use std::time::Instant;

use crate::services::AppServices;
use super::protocol::JsonRpcResponse; 

pub struct MessageRouter {
    app: Arc<AppServices>,   
}

impl MessageRouter {
    pub fn new(app: Arc<AppServices>) -> Self {
        Self { app }
    }

    pub async fn handle_request(
        &self,
        request_id: &str,
        method: &str,
        params: &serde_json::Value,
    ) -> JsonRpcResponse {
        // 统一路由: runtime/* 转发到 app/*
        let app_method = if method.starts_with("runtime/") {
            method.replacen("runtime/", "app/", 1)
        } else if method.starts_with("app/") {
            method.to_string()
        } else {
            log::warn!("[WS] ← 未知方法: {} (id={})", method, request_id);
            return JsonRpcResponse::error(
                request_id,
                -32601,
                &format!("未知方法: {method}"),
            );
        };

        // 请求日志: method + 参数摘要
        let params_preview = Self::preview_params(params);
        log::info!("[WS] → {} {} (id={})", app_method, params_preview, request_id);
        let t0 = Instant::now();

        match self.app.handle_request(&app_method, params.clone()).await {
            Ok(value) => {
                let duration = t0.elapsed();
                let result_preview = Self::preview_result(&value);
                log::info!("[WS] ← {} OK ({}ms) {} (id={})",
                    app_method, duration.as_millis(), result_preview, request_id);
                JsonRpcResponse::success(request_id, value)
            }
            Err(err) => {
                let duration = t0.elapsed();
                log::warn!("[WS] ← {} ERR ({}ms) {} (id={})",
                    app_method, duration.as_millis(), err.message, request_id);
                // 映射 ServiceError 到 JSON-RPC 错误码
                let code = match err.code {
                    crate::core::error::ErrorCode::BadRequest => -32602,
                    crate::core::error::ErrorCode::Forbidden => 1001,
                    crate::core::error::ErrorCode::NotFound => -32601,
                    crate::core::error::ErrorCode::Timeout => 1004,
                    _ => 1003,
                };
                JsonRpcResponse::error(request_id, code, &err.message)
            }
        }
    }

    /// 参数安全摘要（脱敏 env 等字段）
    fn preview_params(params: &serde_json::Value) -> String {
        if params.is_null() {
            return String::new();
        }
        
        let mut safe_params = params.clone();
        if let Some(obj) = safe_params.as_object_mut() {
            if obj.contains_key("env") {
                obj.insert("env".to_string(), serde_json::json!("[HIDDEN]"));
            }
        }
        serde_json::to_string(&safe_params).unwrap_or_default()
    }

    /// 结果全量记录
    fn preview_result(value: &serde_json::Value) -> String {
        serde_json::to_string(value).unwrap_or_default()
    }
}
