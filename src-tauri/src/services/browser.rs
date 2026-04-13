// Browser 服务 — 浏览器引擎封装
//
// 对应 Python app/services/browser.py
// 直接持有 BrowserState, 调用 engine::execute_command
// 不依赖 executors/browser.rs

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::core::error::{ServiceError, ServiceResult};
use crate::engine::{self, BrowserState};

/// 浏览器服务 — 持有独立的 BrowserState
pub struct BrowserService {
    state: Mutex<BrowserState>,
    last_activity: Mutex<Instant>,
}

impl BrowserService {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(BrowserState::new()),
            last_activity: Mutex::new(Instant::now()),
        }
    }

    /// 执行引擎命令 — 统一入口
    pub async fn execute(&self, cmd: Value) -> ServiceResult {
        let action = cmd["action"].as_str().unwrap_or("").to_string();
        log::info!("[BrowserService] {}", action);

        let mut state = self.state.lock().await;
        let result = engine::execute_command(&cmd, &mut state).await;
        drop(state);

        // 更新活跃时间
        *self.last_activity.lock().await = Instant::now();

        // 检查结果
        let success = result["success"].as_bool().unwrap_or(false);
        if success {
            Ok(result.get("result").cloned().unwrap_or(Value::Null))
        } else {
            let msg = result["error"]["message"]
                .as_str()
                .unwrap_or("未知错误")
                .to_string();
            Err(ServiceError::internal(msg))
        }
    }

    /// 获取浏览器页面状态
    pub async fn get_browser_state(&self) -> Option<Value> {
        let state = self.state.lock().await;
        let mgr = state.browser.as_ref()?;
        let tabs = mgr.tab_list();
        if tabs.is_empty() {
            return None;
        }
        Some(json!({"pages": tabs}))
    }

    /// 获取浏览器信息
    pub async fn get_info(&self) -> ServiceResult {
        let state = self.state.lock().await;
        let mgr = state
            .browser
            .as_ref()
            .ok_or_else(|| ServiceError::unavailable("浏览器未启动"))?;
        let tabs = mgr.tab_list();
        let url = mgr.get_url().await.unwrap_or_default();
        let title = mgr.get_title().await.unwrap_or_default();
        Ok(json!({
            "pages": tabs,
            "current_url": url,
            "current_title": title,
            "active_session_id": state.session_id,
        }))
    }

    /// 空闲超时检查
    pub async fn cleanup_if_idle(&self, max_idle: Duration) {
        let elapsed = self.last_activity.lock().await.elapsed();
        if elapsed > max_idle {
            log::info!("[BrowserService] 空闲超时, 关闭浏览器...");
            self.shutdown().await;
        }
    }

    /// 关闭浏览器
    pub async fn shutdown(&self) {
        let mut state = self.state.lock().await;
        if let Some(ref mut mgr) = state.browser {
            let _ = mgr.close().await;
        }
        state.browser = None;
        state.ref_map.clear();
        state.session_id.clear();
        log::info!("[BrowserService] 已关闭");
    }

    /// WS handler
    pub async fn handle(&self, action: &str, params: Value) -> ServiceResult {
        match action {
            "info" => self.get_info().await,
            "status" => {
                let state = self.get_browser_state().await;
                Ok(state.unwrap_or(json!({"running": false})))
            }
            "shutdown" => {
                self.shutdown().await;
                Ok(json!({"closed": true}))
            }
            // 其他操作直接转发给 engine
            _ => {
                let mut cmd = params;
                cmd["action"] = json!(action);
                self.execute(cmd).await
            }
        }
    }
}
