// TabPilot — Tauri 入口
//
// Pure Rust: 无 Python sidecar

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// ── 模块声明 ──────────────────────────────
#[allow(dead_code)]
mod core;          // 基础配置 + 错误类型
mod services;      // 业务服务层
mod models;        // 数据模型
#[allow(dead_code)]
mod engine;        // CDP 浏览器引擎
mod router;        // WS 通信 + 消息分发
mod infra;         // 审计/门控/存储/运行时
mod ui;            // Tauri Commands + 认证 + Deep Link
mod tray;
#[allow(dead_code)]
mod toolkit;       // 工具箱 (从 Aily SO 移植)

use tauri::{Listener, Manager};

fn main() {
    init_logger();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // 获取数据目录 (dev/prod 隔离)
            let mut data_dir = app.path().app_data_dir()
                .unwrap_or_else(|_| {
                    dirs::data_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                        .join("tabpilot")
                });

            // debug 模式: 追加 -dev 避免和生产数据互相干扰
            #[cfg(debug_assertions)]
            {
                let dir_name = data_dir.file_name()
                    .map(|n| format!("{}-dev", n.to_string_lossy()))
                    .unwrap_or_else(|| "tabpilot-dev".to_string());
                data_dir.set_file_name(dir_name);
                log::info!("[TabPilot] DEV 数据目录: {:?}", data_dir);
            }

            // 初始化 AppState
            let state = tauri::async_runtime::block_on(router::AppState::init(data_dir));
            app.manage(state);

            // 系统托盘
            tray::setup_tray(app)?;

            // Deep Link 处理
            let handle = app.handle().clone();
            app.listen("deep-link://new-url", move |event| {
                let h = handle.clone();
                let payload = event.payload().to_string();
                tauri::async_runtime::spawn(async move {
                    ui::deeplink::handle_deep_link(h, &payload).await;
                });
            });

            // 窗口关闭 → 隐藏到托盘
            if let Some(window) = app.get_webview_window("main") {
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }

            log::info!("[TabPilot] 应用已启动 (Pure Rust)");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ui::commands::get_status,
            ui::commands::get_logs,
            ui::commands::logout,
            ui::commands::save_token,
            ui::commands::get_auth_challenge,
            ui::commands::start_auth_poll,
            ui::commands::set_guard_mode,
            ui::commands::set_workspace,
            ui::commands::set_browser_enabled,
            ui::commands::set_audit_enabled,
            ui::commands::get_remembered,
            ui::commands::remove_remembered,
            ui::commands::clear_guard,
            ui::commands::get_protected_paths,
            ui::commands::get_tray_visible,
            ui::commands::set_tray_visible,
            ui::commands::browser_action,
            ui::commands::list_shell_sessions,
            ui::commands::read_shell_output,
            ui::commands::kill_shell_session,
            ui::commands::exec_shell_command,
        ])
        .build(tauri::generate_context!())
        .expect("TabPilot 构建失败")
        .run(|app, event| {
            // macOS: 点击 Dock 图标时重新显示窗口
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { has_visible_windows, .. } = event {
                if !has_visible_windows {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
            let _ = (app, event); // 避免 unused 警告
        });
}

/// 日志初始化 — 直接写文件, 不输出 console
///
/// 路径: ~/.tabpilot/logs/pilot.log
/// 按天轮转, 保留 7 个旧文件
fn init_logger() {
    use std::fs;
    use std::io::Write;
    use std::sync::Mutex;

    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join(".tabpilot")
        .join("logs");
    let _ = fs::create_dir_all(&log_dir);

    // 轮转: 如果 pilot.log 超过 10MB, 重命名为 pilot.1.log ~ pilot.7.log
    let log_path = log_dir.join("pilot.log");
    if let Ok(meta) = fs::metadata(&log_path) {
        if meta.len() > 10 * 1024 * 1024 {
            for i in (1..7).rev() {
                let from = log_dir.join(format!("pilot.{}.log", i));
                let to = log_dir.join(format!("pilot.{}.log", i + 1));
                let _ = fs::rename(&from, &to);
            }
            let _ = fs::rename(&log_path, log_dir.join("pilot.1.log"));
        }
    }

    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect(&format!("无法打开日志文件: {:?}", log_path));

    let file = Mutex::new(file);

    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .format(move |_buf, record| {
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let line = format!(
                "{} {} [{}] {}\n",
                now,
                record.level(),
                record.module_path().unwrap_or(""),
                record.args()
            );
            if let Ok(mut f) = file.lock() {
                let _ = f.write_all(line.as_bytes());
                let _ = f.flush();
            }
            Ok(())
        })
        .init();

    log::info!("[TabPilot] 日志已初始化: {:?}", log_path);
}
