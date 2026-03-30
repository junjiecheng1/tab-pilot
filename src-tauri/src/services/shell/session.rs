// PTY 会话 — 终端生命周期管理
//
// 封装 portable_pty 的创建、写入、终止逻辑

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use super::collector::OutputCollector;

/// Shell 会话
pub struct ShellSession {
    pub id: String,
    pub shell: String,
    pub working_dir: PathBuf,
    pub created_at: Instant,
    pub last_used: Instant,
    pub active: bool,
    /// PTY master (写入端)
    writer: Box<dyn Write + Send>,
    /// 子进程
    child: Box<dyn portable_pty::Child + Send>,
    /// 输出收集器
    pub collector: OutputCollector,
}

impl ShellSession {
    /// 创建 PTY 会话 (阻塞操作, 需在 spawn_blocking 中调用)
    pub fn create(
        session_id: &str,
        shell_cmd: &str,
        cwd: &std::path::Path,
        environment: Option<&HashMap<String, String>>,
    ) -> Result<Self, String> {
        let pty_system = portable_pty::native_pty_system();
        let pair = pty_system
            .openpty(portable_pty::PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("PTY 创建失败: {e}"))?;

        let mut cmd = portable_pty::CommandBuilder::new(shell_cmd);
        cmd.arg("-i");
        cmd.cwd(cwd);

        // 注入环境变量
        if let Some(env) = environment {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }
        cmd.env("SESSION_ID", session_id);
        cmd.env("TERM", "xterm-256color");

        // 注入 CLI 工具 PATH (rg, fd, jq, yq, markitdown 等)
        let tools_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".tabpilot")
            .join("runtime")
            .join("tools");
        if tools_dir.exists() {
            let system_path = std::env::var("PATH").unwrap_or_default();
            let sep = if cfg!(windows) { ";" } else { ":" };
            // 基础 tools/ + Archive 子目录 (如 tools/markitdown/)
            let mut path_parts = vec![tools_dir.display().to_string()];
            let markitdown_dir = tools_dir.join("markitdown");
            if markitdown_dir.exists() {
                path_parts.push(markitdown_dir.display().to_string());
            }
            path_parts.push(system_path);
            cmd.env("PATH", path_parts.join(sep));
        }

        let child = pair.slave.spawn_command(cmd)
            .map_err(|e| format!("进程启动失败: {e}"))?;

        let writer = pair.master.take_writer()
            .map_err(|e| format!("PTY writer 获取失败: {e}"))?;

        let reader = pair.master.try_clone_reader()
            .map_err(|e| format!("PTY reader 获取失败: {e}"))?;

        // 启动输出收集
        let collector = OutputCollector::new();
        collector.spawn_reader(reader, session_id.to_string());

        log::info!("[Shell] 会话已创建: {}", session_id);

        Ok(Self {
            id: session_id.to_string(),
            shell: shell_cmd.to_string(),
            working_dir: cwd.to_path_buf(),
            created_at: Instant::now(),
            last_used: Instant::now(),
            active: true,
            writer,
            child,
            collector,
        })
    }

    /// 写入命令到 PTY
    pub fn write_command(&mut self, command: &str) -> Result<(), String> {
        let cmd_line = if command.ends_with('\n') {
            command.to_string()
        } else {
            format!("{command}\n")
        };
        self.writer
            .write_all(cmd_line.as_bytes())
            .map_err(|e| format!("写入失败: {e}"))?;
        let _ = self.writer.flush();
        self.last_used = Instant::now();
        Ok(())
    }

    /// 写入任意文本
    pub fn write_raw(&mut self, text: &str) -> Result<usize, String> {
        self.writer
            .write_all(text.as_bytes())
            .map_err(|e| format!("写入失败: {e}"))?;
        let _ = self.writer.flush();
        self.last_used = Instant::now();
        Ok(text.len())
    }

    /// 获取 exit_code (非阻塞)
    pub fn try_exit_code(&mut self) -> Option<i32> {
        match self.child.try_wait() {
            Ok(Some(status)) => {
                self.active = false;
                Some(status.exit_code() as i32)
            }
            _ => None,
        }
    }

    /// 终止进程
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        self.active = false;
    }
}
