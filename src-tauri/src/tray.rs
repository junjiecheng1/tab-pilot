// tray.rs — 系统托盘
//
// 左键单击 → 显示/聚焦主窗口
// 右键单击 → 弹出菜单
//
// 菜单:
//   · TabPilot 运行中     (只读状态)
//   · ────────
//   · 安全模式 ▸ (保守/标准/信任)
//   · 开机启动
//   · ────────
//   · 退出

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu, CheckMenuItem},
    tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
    App, Emitter, Manager,
};

use crate::router::AppState;

/// 初始化系统托盘
pub fn setup_tray(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    // 读取当前状态
    let state = app.state::<AppState>();
    let current_mode = tauri::async_runtime::block_on(async {
        state.guard.read().await.mode().to_string()
    });
    let autostart_enabled = {
        // 尝试读 autostart 插件状态
        use tauri_plugin_autostart::ManagerExt;
        app.autolaunch().is_enabled().unwrap_or(false)
    };

    // 状态行 (只读, 不可点击)
    let status_item = MenuItem::with_id(app, "status", "TabPilot 运行中", false, None::<&str>)?;

    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    // 安全模式 — 根据当前状态设置 checked
    let mode_conservative = CheckMenuItem::with_id(
        app, "mode_conservative", "保守模式", true,
        current_mode == "conservative", None::<&str>,
    )?;
    let mode_standard = CheckMenuItem::with_id(
        app, "mode_standard", "标准模式", true,
        current_mode == "standard", None::<&str>,
    )?;
    let mode_trust = CheckMenuItem::with_id(
        app, "mode_trust", "信任模式", true,
        current_mode == "trust", None::<&str>,
    )?;
    let mode_submenu = Submenu::with_items(
        app, "安全模式", true,
        &[&mode_conservative, &mode_standard, &mode_trust],
    )?;

    // 开机启动 — 同步当前状态
    let autostart_item = CheckMenuItem::with_id(
        app, "autostart", "开机启动", true, autostart_enabled, None::<&str>,
    )?;

    // 退出
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&status_item, &sep1, &mode_submenu, &autostart_item, &sep2, &quit_item],
    )?;

    // 图标 (编译时嵌入 32x32 PNG)
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))
        .expect("托盘图标加载失败");

    let _tray = TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(false)
        .menu(&menu)
        .show_menu_on_left_click(false) // 左键不弹菜单
        .tooltip("TabPilot")
        .on_menu_event(move |app, event| {
            match event.id.as_ref() {
                "mode_conservative" => {
                    mode_conservative.set_checked(true).unwrap_or(());
                    mode_standard.set_checked(false).unwrap_or(());
                    mode_trust.set_checked(false).unwrap_or(());
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        set_guard_mode(&handle, "conservative").await;
                    });
                }
                "mode_standard" => {
                    mode_conservative.set_checked(false).unwrap_or(());
                    mode_standard.set_checked(true).unwrap_or(());
                    mode_trust.set_checked(false).unwrap_or(());
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        set_guard_mode(&handle, "standard").await;
                    });
                }
                "mode_trust" => {
                    mode_conservative.set_checked(false).unwrap_or(());
                    mode_standard.set_checked(false).unwrap_or(());
                    mode_trust.set_checked(true).unwrap_or(());
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        set_guard_mode(&handle, "trust").await;
                    });
                }
                "autostart" => {
                    // 读取当前勾选状态并切换
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        use tauri_plugin_autostart::ManagerExt;
                        let autolaunch = handle.autolaunch();
                        let enabled = autolaunch.is_enabled().unwrap_or(false);
                        if enabled {
                            let _ = autolaunch.disable();
                        } else {
                            let _ = autolaunch.enable();
                        }
                        log::info!("[Tray] 开机启动: {}", !enabled);
                    });
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            // 左键单击 → 显示窗口
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    log::info!("[Tray] 系统托盘已初始化");
    Ok(())
}

/// 直接操作 AppState (零 HTTP)
async fn set_guard_mode(handle: &tauri::AppHandle, mode: &str) {
    let state = handle.state::<AppState>();
    state.guard.write().await.set_mode(mode);
    state.store.set("settings", "guard_mode", serde_json::json!(mode));

    let mode_label = match mode {
        "conservative" => "保守模式",
        "trust" => "信任模式",
        _ => "标准模式",
    };
    let _ = handle.emit("guard-mode-changed", mode_label);
    log::info!("[Tray] 安全模式: {}", mode_label);
}
