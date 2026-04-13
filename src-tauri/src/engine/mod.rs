// engine — 从 agent-browser 提取的浏览器自动化引擎
//
// 基于 CDP (Chrome DevTools Protocol) 直连 Chrome,
// 不依赖 Node.js / Playwright。
//
// 核心入口: BrowserState + execute_command
// 注: 大量函数通过 execute_command 动态分发调用, 静态分析报 dead_code 是误报

pub mod browser;
pub mod cdp;
pub mod cookies;
pub mod diff;
pub mod element;
pub mod install;
pub mod interaction;
pub mod network;
pub mod screenshot;
pub mod snapshot;
pub mod state;
pub mod storage;
pub mod stream;
pub mod webdriver;

use browser::BrowserManager;
use cdp::chrome::LaunchOptions;
use element::RefMap;
use screenshot::ScreenshotOptions;
use serde_json::{json, Value};
use snapshot::SnapshotOptions;
use stream::StreamServer;

// ── BrowserState ──────────────────────────

/// 浏览器运行时状态
pub struct BrowserState {
    /// Chrome 浏览器管理器
    pub browser: Option<BrowserManager>,
    /// ARIA ref 映射表 (每次 snapshot 更新)
    pub ref_map: RefMap,
    /// CDP 会话 ID
    pub session_id: String,
    /// 帧推流 StreamServer
    pub stream_server: Option<StreamServer>,
    /// AI 是否正在操作 (true 时禁止人工交互, 显示蓝色边框)
    pub agent_active: bool,
}

impl BrowserState {
    pub fn new() -> Self {
        Self {
            browser: None,
            ref_map: RefMap::new(),
            session_id: String::new(),
            stream_server: None,
            agent_active: false,
        }
    }
}

// ── execute_command 命令分发 ──────────────

/// 核心入口: 执行浏览器命令
pub async fn execute_command(cmd: &Value, state: &mut BrowserState) -> Value {
    let action = cmd.get("action").and_then(|v| v.as_str()).unwrap_or("");

    // 非 launch/close 命令自动启动浏览器 (仅当无实例时)
    let skip_launch = matches!(action, "" | "launch" | "close");
    if !skip_launch && state.browser.is_none() {
        if let Err(e) = auto_launch(state).await {
            return error_response(&format!("自动启动失败: {}", e));
        }
    }

    let result = match action {
        // Chrome 生命周期
        "launch" => handle_launch(cmd, state).await,
        "close" => handle_close(state).await,

        // 导航
        "navigate" => handle_navigate(cmd, state).await,
        "back" => handle_back(state).await,
        "forward" => handle_forward(state).await,
        "reload" => handle_reload(state).await,

        // 观察
        "snapshot" => handle_snapshot(cmd, state).await,
        "screenshot" => handle_screenshot(cmd, state).await,
        "url" => handle_url(state).await,
        "title" => handle_title(state).await,

        // 交互
        "click" => handle_click(cmd, state).await,
        "dblclick" => handle_dblclick(cmd, state).await,
        "fill" => handle_fill(cmd, state).await,
        "type" => handle_type(cmd, state).await,
        "press" => handle_press(cmd, state).await,
        "hover" => handle_hover(cmd, state).await,
        "scroll" => handle_scroll(cmd, state).await,
        "select" => handle_select(cmd, state).await,
        "check" => handle_check(cmd, state).await,
        "uncheck" => handle_uncheck(cmd, state).await,

        // 等待
        "wait" => handle_wait(cmd).await,

        // 元素查询
        "gettext" => handle_gettext(cmd, state).await,
        "isvisible" => handle_isvisible(cmd, state).await,

        // 视觉
        "highlight" => handle_highlight(cmd, state).await,

        // Tab 管理
        "tab_list" => handle_tab_list(state).await,
        "tab_new" => handle_tab_new(cmd, state).await,
        "tab_switch" => handle_tab_switch(cmd, state).await,
        "tab_close" => handle_tab_close(cmd, state).await,

        // JS 执行
        "evaluate" => handle_evaluate(cmd, state).await,

        // Agent 操作状态
        "agent_start" => {
            state.agent_active = true;
            // 推送到 BrowserView
            if let Some(ref ss) = state.stream_server {
                let _ = ss.broadcast_frame(
                    &serde_json::json!({
                        "type": "agent_state", "active": true
                    })
                    .to_string(),
                );
            }
            // 注入蓝色呼吸边框到 Chrome 页面
            if let Some(ref mgr) = state.browser {
                let inject_js = r#"
                (function(){
                    if(document.getElementById('__tab_agent_border')) return;
                    var s = document.createElement('style');
                    s.id = '__tab_agent_style';
                    s.textContent = '@keyframes __tab_glow{0%,100%{box-shadow:inset 0 0 0 2px rgba(66,133,244,0.5),inset 0 0 30px rgba(66,133,244,0.06),0 0 8px rgba(66,133,244,0.15)}50%{box-shadow:inset 0 0 0 2px rgba(66,133,244,0.8),inset 0 0 50px rgba(66,133,244,0.1),0 0 16px rgba(66,133,244,0.25)}}#__tab_agent_border{position:fixed;inset:0;z-index:2147483647;pointer-events:all;cursor:not-allowed;background:rgba(0,0,0,0.02);animation:__tab_glow 2.5s ease-in-out infinite}';
                    document.head.appendChild(s);
                    var el = document.createElement('div');
                    el.id = '__tab_agent_border';
                    document.body.appendChild(el);
                })()
                "#;
                let _ = mgr
                    .client
                    .send_command(
                        "Runtime.evaluate",
                        Some(json!({"expression": inject_js})),
                        Some(&state.session_id),
                    )
                    .await;
            }
            Ok(json!({"agent_active": true}))
        }
        "agent_stop" => {
            state.agent_active = false;
            if let Some(ref ss) = state.stream_server {
                let _ = ss.broadcast_frame(
                    &serde_json::json!({
                        "type": "agent_state", "active": false
                    })
                    .to_string(),
                );
            }
            // 移除 Chrome 页面上的蓝色边框
            if let Some(ref mgr) = state.browser {
                let remove_js = r#"
                (function(){
                    ['__tab_agent_border','__tab_agent_badge','__tab_agent_style']
                    .forEach(function(id){ var e=document.getElementById(id); if(e) e.remove(); });
                })()
                "#;
                let _ = mgr
                    .client
                    .send_command(
                        "Runtime.evaluate",
                        Some(json!({"expression": remove_js})),
                        Some(&state.session_id),
                    )
                    .await;
            }
            Ok(json!({"agent_active": false}))
        }

        _ => Err(format!("未知 action: {}", action)),
    };

    match result {
        Ok(data) => json!({"success": true, "result": data}),
        Err(msg) => {
            // CDP 连接断开 或 Session 失效: 清理状态, 下次命令 auto_launch 自动重开
            if msg.contains("closed connection")
                || msg.contains("connection closed")
                || msg.contains("Session with given id not found")
            {
                log::warn!("[engine] CDP 连接/session 失效, 清理状态 (下次自动重开)");
                state.browser = None;
                state.session_id.clear();
                state.stream_server = None;
            }
            error_response(&msg)
        }
    }
}

fn error_response(msg: &str) -> Value {
    json!({"success": false, "error": {"message": msg}})
}

// ── 辅助宏: 获取 browser + session_id ──

macro_rules! require_browser {
    ($state:expr) => {{
        let mgr = $state
            .browser
            .as_ref()
            .ok_or_else(|| "浏览器未启动".to_string())?;
        let sid = mgr.active_session_id()?.to_string();
        (mgr, sid)
    }};
}

fn get_str<'a>(cmd: &'a Value, key: &str) -> Option<&'a str> {
    cmd.get(key).and_then(|v| v.as_str())
}

// ── Handler 实现 ──────────────────────────

/// TabPilot Chrome profile 目录
fn get_profile_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".tabpilot")
        .join("chrome-profile")
}

/// 尝试连接已有 Chrome 实例 (通过 profile 目录的 DevToolsActivePort)
async fn try_connect_existing(profile_dir: &std::path::Path) -> Option<BrowserManager> {
    use cdp::chrome::read_devtools_active_port;

    let (port, ws_path) = read_devtools_active_port(profile_dir)?;
    let ws_url = format!("ws://127.0.0.1:{}{}", port, ws_path);

    // 验证端口可达
    let addr = format!("127.0.0.1:{}", port);
    let reachable = tokio::task::spawn_blocking(move || {
        std::net::TcpStream::connect_timeout(
            &addr.parse().unwrap(),
            std::time::Duration::from_millis(500),
        )
        .is_ok()
    })
    .await
    .unwrap_or(false);

    if !reachable {
        // 旧文件残留, 清理
        let _ = std::fs::remove_file(profile_dir.join("DevToolsActivePort"));
        log::info!("[engine] 旧 DevToolsActivePort 失效, 已清理");
        return None;
    }

    match BrowserManager::connect_cdp(&ws_url).await {
        Ok(mgr) => {
            log::info!("[engine] 复用已有 Chrome 实例 (port={port})");
            Some(mgr)
        }
        Err(e) => {
            log::warn!("[engine] 连接已有 Chrome 失败: {e}");
            None
        }
    }
}

async fn auto_launch(state: &mut BrowserState) -> Result<(), String> {
    // 已有浏览器实例则复用
    if state.browser.is_some() {
        log::info!("[engine] Chrome 已运行, 复用现有实例");
        return Ok(());
    }

    let profile_dir = get_profile_dir();

    // 优先连接已有实例 (避免多窗口)
    if let Some(mgr) = try_connect_existing(&profile_dir).await {
        let sid = mgr.active_session_id()?.to_string();
        if state.stream_server.is_none() {
            let client = mgr.client.clone();
            match StreamServer::start(9223, client, sid.clone()).await {
                Ok(ss) => {
                    log::info!("[engine] StreamServer 已启动 (port={})", ss.port());
                    state.stream_server = Some(ss);
                }
                Err(e) => log::warn!("[engine] StreamServer 启动失败: {e}"),
            }
        }
        state.browser = Some(mgr);
        state.session_id = sid;
        state.ref_map.clear();
        return Ok(());
    }

    // 无已有实例 → 新启动
    let options = LaunchOptions {
        headless: false,
        profile: Some(profile_dir.to_string_lossy().to_string()),
        ..Default::default()
    };
    let mgr = BrowserManager::launch(options, Some("chrome")).await?;
    let sid = mgr.active_session_id()?.to_string();

    // 启动 StreamServer (如果还没有)
    if state.stream_server.is_none() {
        let client = mgr.client.clone();
        match StreamServer::start(9223, client, sid.clone()).await {
            Ok(ss) => {
                log::info!("[engine] StreamServer 已启动 (port={})", ss.port());
                state.stream_server = Some(ss);
            }
            Err(e) => log::warn!("[engine] StreamServer 启动失败: {e}"),
        }
    }

    state.browser = Some(mgr);
    state.session_id = sid;
    state.ref_map.clear();
    log::info!("[engine] Chrome 已自动启动");
    Ok(())
}

async fn handle_launch(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    // 已有浏览器实例则复用
    if state.browser.is_some() {
        log::info!("[engine] Chrome 已运行, 复用现有实例");
        return Ok(json!({"launched": true, "reused": true}));
    }

    let headed = cmd.get("headed").and_then(|v| v.as_bool()).unwrap_or(true);
    let profile_dir = get_profile_dir();

    // 优先连接已有实例
    if let Some(mgr) = try_connect_existing(&profile_dir).await {
        let sid = mgr.active_session_id()?.to_string();
        if state.stream_server.is_none() {
            let client = mgr.client.clone();
            match StreamServer::start(9223, client, sid.clone()).await {
                Ok(ss) => {
                    log::info!("[engine] StreamServer 已启动 (port={})", ss.port());
                    state.stream_server = Some(ss);
                }
                Err(e) => log::warn!("[engine] StreamServer 启动失败: {e}"),
            }
        }
        state.browser = Some(mgr);
        state.session_id = sid;
        state.ref_map.clear();
        return Ok(json!({"launched": true, "reused": true}));
    }

    let options = LaunchOptions {
        headless: !headed,
        profile: Some(profile_dir.to_string_lossy().to_string()),
        ..Default::default()
    };

    let mgr = BrowserManager::launch(options, Some("chrome")).await?;
    let sid = mgr.active_session_id()?.to_string();

    // 启动 StreamServer
    if state.stream_server.is_none() {
        let client = mgr.client.clone();
        match StreamServer::start(9223, client, sid.clone()).await {
            Ok(ss) => {
                log::info!("[engine] StreamServer 已启动 (port={})", ss.port());
                state.stream_server = Some(ss);
            }
            Err(e) => log::warn!("[engine] StreamServer 启动失败: {e}"),
        }
    }

    state.browser = Some(mgr);
    state.session_id = sid;
    state.ref_map.clear();

    Ok(json!({"launched": true, "headed": headed}))
}

async fn handle_close(state: &mut BrowserState) -> Result<Value, String> {
    if let Some(ref mut mgr) = state.browser {
        let _ = mgr.close().await;
    }
    state.browser = None;
    state.ref_map.clear();
    state.session_id.clear();
    Ok(json!({"closed": true}))
}

async fn handle_navigate(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let url = get_str(cmd, "url").ok_or("缺少 url 参数")?;
    let mgr = state.browser.as_mut().ok_or("浏览器未启动")?;
    let wait_until = browser::WaitUntil::Load;
    mgr.navigate(url, wait_until).await?;
    let current_url = mgr.get_url().await.unwrap_or_default();
    let title = mgr.get_title().await.unwrap_or_default();
    Ok(json!({"url": current_url, "title": title}))
}

async fn handle_back(state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    mgr.client
        .send_command_no_params("Page.goBack", Some(&sid))
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    let url = mgr.get_url().await.unwrap_or_default();
    Ok(json!({"url": url}))
}

async fn handle_forward(state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    mgr.client
        .send_command_no_params("Page.goForward", Some(&sid))
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    let url = mgr.get_url().await.unwrap_or_default();
    Ok(json!({"url": url}))
}

async fn handle_reload(state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    mgr.client
        .send_command_no_params("Page.reload", Some(&sid))
        .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    let url = mgr.get_url().await.unwrap_or_default();
    Ok(json!({"url": url}))
}

async fn handle_snapshot(_cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);

    let options = SnapshotOptions::default();
    let snapshot_text =
        snapshot::take_snapshot(&mgr.client, &sid, &options, &mut state.ref_map, None).await?;

    let url = mgr.get_url().await.unwrap_or_default();
    let title = mgr.get_title().await.unwrap_or_default();

    Ok(json!({
        "snapshot": snapshot_text,
        "url": url,
        "title": title,
    }))
}

async fn handle_screenshot(_cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let options = ScreenshotOptions::default();
    let result = screenshot::take_screenshot(&mgr.client, &sid, &state.ref_map, &options).await?;
    Ok(json!({"image_base64": result.base64, "path": result.path}))
}

async fn handle_url(state: &mut BrowserState) -> Result<Value, String> {
    let mgr = state.browser.as_ref().ok_or("浏览器未启动")?;
    let url = mgr.get_url().await.unwrap_or_default();
    Ok(json!({"url": url}))
}

async fn handle_title(state: &mut BrowserState) -> Result<Value, String> {
    let mgr = state.browser.as_ref().ok_or("浏览器未启动")?;
    let title = mgr.get_title().await.unwrap_or_default();
    Ok(json!({"title": title}))
}

async fn handle_click(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let selector = get_str(cmd, "selector").ok_or("缺少 selector 参数")?;
    let button = get_str(cmd, "button").unwrap_or("left");
    let click_count = cmd.get("clickCount").and_then(|v| v.as_i64()).unwrap_or(1) as i32;

    // 蓝色高亮 (忽略错误)
    let _ = interaction::highlight(&mgr.client, &sid, &state.ref_map, selector).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    interaction::click(
        &mgr.client,
        &sid,
        &state.ref_map,
        selector,
        button,
        click_count,
    )
    .await?;
    Ok(json!({"clicked": selector}))
}

async fn handle_dblclick(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let selector = get_str(cmd, "selector").ok_or("缺少 selector 参数")?;
    interaction::dblclick(&mgr.client, &sid, &state.ref_map, selector).await?;
    Ok(json!({"dblclicked": selector}))
}

async fn handle_fill(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let selector = get_str(cmd, "selector").ok_or("缺少 selector 参数")?;
    let value = get_str(cmd, "value").ok_or("缺少 value 参数")?;

    let _ = interaction::highlight(&mgr.client, &sid, &state.ref_map, selector).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    interaction::fill(&mgr.client, &sid, &state.ref_map, selector, value).await?;
    Ok(json!({"filled": selector, "value": value}))
}

async fn handle_type(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let selector = get_str(cmd, "selector").ok_or("缺少 selector 参数")?;
    let text = get_str(cmd, "text")
        .or_else(|| get_str(cmd, "value"))
        .ok_or("缺少 text 参数")?;
    let clear = cmd.get("clear").and_then(|v| v.as_bool()).unwrap_or(false);
    let delay = cmd.get("delay").and_then(|v| v.as_u64());

    interaction::type_text(
        &mgr.client,
        &sid,
        &state.ref_map,
        selector,
        text,
        clear,
        delay,
    )
    .await?;
    Ok(json!({"typed": selector, "text": text}))
}

async fn handle_press(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let key = get_str(cmd, "key").ok_or("缺少 key 参数")?;
    interaction::press_key(&mgr.client, &sid, key).await?;
    Ok(json!({"pressed": key}))
}

async fn handle_hover(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let selector = get_str(cmd, "selector").ok_or("缺少 selector 参数")?;
    interaction::hover(&mgr.client, &sid, &state.ref_map, selector).await?;
    Ok(json!({"hovered": selector}))
}

async fn handle_scroll(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let selector = cmd.get("selector").and_then(|v| v.as_str());
    let delta_x = cmd.get("delta_x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let delta_y = cmd.get("delta_y").and_then(|v| v.as_f64()).unwrap_or(500.0);
    interaction::scroll(
        &mgr.client,
        &sid,
        &state.ref_map,
        selector,
        delta_x,
        delta_y,
    )
    .await?;
    Ok(json!({"scrolled": true, "delta_x": delta_x, "delta_y": delta_y}))
}

async fn handle_select(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let selector = get_str(cmd, "selector").ok_or("缺少 selector 参数")?;
    let value = get_str(cmd, "value").ok_or("缺少 value 参数")?;
    interaction::select_option(
        &mgr.client,
        &sid,
        &state.ref_map,
        selector,
        &[value.to_string()],
    )
    .await?;
    Ok(json!({"selected": selector, "value": value}))
}

async fn handle_check(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let selector = get_str(cmd, "selector").ok_or("缺少 selector 参数")?;
    interaction::check(&mgr.client, &sid, &state.ref_map, selector).await?;
    Ok(json!({"checked": selector}))
}

async fn handle_uncheck(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let selector = get_str(cmd, "selector").ok_or("缺少 selector 参数")?;
    interaction::uncheck(&mgr.client, &sid, &state.ref_map, selector).await?;
    Ok(json!({"unchecked": selector}))
}

async fn handle_wait(cmd: &Value) -> Result<Value, String> {
    let ms = cmd.get("ms").and_then(|v| v.as_u64()).unwrap_or(1000);
    tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
    Ok(json!({"waited_ms": ms}))
}

async fn handle_gettext(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let selector = cmd.get("selector").and_then(|v| v.as_str());
    let text = if let Some(sel) = selector {
        element::get_element_text(&mgr.client, &sid, &state.ref_map, sel).await?
    } else {
        // 获取整页文本
        let result: cdp::types::EvaluateResult = mgr
            .client
            .send_command_typed(
                "Runtime.evaluate",
                &cdp::types::EvaluateParams {
                    expression: "document.body?.innerText || ''".to_string(),
                    return_by_value: Some(true),
                    await_promise: Some(false),
                },
                Some(&sid),
            )
            .await?;
        result
            .result
            .value
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default()
    };
    Ok(json!({"text": text}))
}

async fn handle_isvisible(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let selector = get_str(cmd, "selector").ok_or("缺少 selector 参数")?;
    let visible = element::is_element_visible(&mgr.client, &sid, &state.ref_map, selector).await?;
    Ok(json!({"visible": visible}))
}

async fn handle_highlight(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let selector = get_str(cmd, "selector").ok_or("缺少 selector 参数")?;
    interaction::highlight(&mgr.client, &sid, &state.ref_map, selector).await?;
    Ok(json!({"highlighted": selector}))
}

async fn handle_tab_list(state: &mut BrowserState) -> Result<Value, String> {
    let mgr = state.browser.as_ref().ok_or("浏览器未启动")?;
    let tabs = mgr.tab_list();
    Ok(json!({"tabs": tabs}))
}

async fn handle_tab_new(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let mgr = state.browser.as_mut().ok_or("浏览器未启动")?;
    let url = get_str(cmd, "url");
    mgr.tab_new(url).await?;
    state.session_id = mgr.active_session_id()?.to_string();
    Ok(json!({"new_tab": url.unwrap_or("about:blank")}))
}

async fn handle_tab_switch(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let mgr = state.browser.as_mut().ok_or("浏览器未启动")?;
    let index = cmd
        .get("index")
        .and_then(|v| v.as_u64())
        .ok_or("缺少 index 参数")? as usize;
    mgr.tab_switch(index).await?;
    state.session_id = mgr.active_session_id()?.to_string();
    Ok(json!({"switched_to": index}))
}

async fn handle_tab_close(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let mgr = state.browser.as_mut().ok_or("浏览器未启动")?;
    let index = cmd
        .get("index")
        .and_then(|v| v.as_u64())
        .map(|i| i as usize);
    mgr.tab_close(index).await?;
    state.session_id = mgr
        .active_session_id()
        .map(|s| s.to_string())
        .unwrap_or_default();
    Ok(json!({"tab_closed": true}))
}

async fn handle_evaluate(cmd: &Value, state: &mut BrowserState) -> Result<Value, String> {
    let (mgr, sid) = require_browser!(state);
    let expression = get_str(cmd, "expression")
        .or_else(|| get_str(cmd, "script"))
        .ok_or("缺少 expression 参数")?;

    let result: cdp::types::EvaluateResult = mgr
        .client
        .send_command_typed(
            "Runtime.evaluate",
            &cdp::types::EvaluateParams {
                expression: expression.to_string(),
                return_by_value: Some(true),
                await_promise: Some(true),
            },
            Some(&sid),
        )
        .await?;

    Ok(json!({"value": result.result.value}))
}
