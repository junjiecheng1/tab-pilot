// PTY 会话 — 终端生命周期管理
//
// 封装 portable_pty 的创建、写入、终止逻辑

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use super::collector::OutputCollector;
use crate::infra::tools::ToolsManager;

const MAX_COMMAND_HISTORY: usize = 20;

#[derive(Clone, Debug)]
pub struct ShellCommandState {
    pub id: String,
    pub command: String,
    pub marker: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub command_done: bool,
    pub timed_out: bool,
    pub output: String,
}

impl ShellCommandState {
    pub fn new(command: &str) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let marker = format!("__TABPILOT_DONE__{}__", id.replace('-', ""));
        Self {
            id,
            command: command.to_string(),
            marker,
            status: "running".to_string(),
            exit_code: None,
            command_done: false,
            timed_out: false,
            output: String::new(),
        }
    }
}

/// Shell 会话
pub struct ShellSession {
    pub id: String,
    pub shell: String,
    pub working_dir: PathBuf,
    pub created_at: Instant,
    pub last_used: Instant,
    pub active: bool,
    pub current_command: Option<ShellCommandState>,
    pub command_history: Vec<ShellCommandState>,
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

        let mut cmd = portable_pty::CommandBuilder::new(&shell_cmd);
        // -i (interactive) 仅对 bash/zsh 有意义，Windows cmd.exe 不支持
        if !cfg!(target_os = "windows") {
            cmd.arg("-i");
        }
        cmd.cwd(cwd);

        if let Some(env) = environment {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }
        cmd.env("SESSION_ID", session_id);
        cmd.env("TERM", "xterm-256color");

        let tools_mgr = ToolsManager::default();
        let tools_dir = tools_mgr.tools_dir().to_path_buf();
        if tools_dir.exists() {
            let system_path = std::env::var("PATH").unwrap_or_default();
            let sep = if cfg!(windows) { ";" } else { ":" };
            let mut path_parts = tools_mgr
                .path_dirs()
                .into_iter()
                .map(|dir| dir.display().to_string())
                .collect::<Vec<_>>();
            path_parts.push(system_path);
            cmd.env("PATH", path_parts.join(sep));
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("进程启动失败: {e}"))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("PTY writer 获取失败: {e}"))?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("PTY reader 获取失败: {e}"))?;

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
            current_command: None,
            command_history: Vec::new(),
            writer,
            child,
            collector,
        })
    }

    pub fn begin_command(&mut self, command: &str) -> Result<ShellCommandState, String> {
        self.refresh_active();
        if !self.active {
            return Err("shell session 已结束".to_string());
        }
        if self.has_running_command() {
            return Err("当前 session 已有运行中的 command".to_string());
        }
        self.collector.clear();
        let state = ShellCommandState::new(command);
        self.current_command = Some(state.clone());
        self.last_used = Instant::now();
        Ok(state)
    }

    /// 写入命令到 PTY
    pub fn write_command(&mut self, command: &str) -> Result<(), String> {
        self.refresh_active();
        if !self.active {
            return Err("shell session 已结束，无法写入".to_string());
        }
        let cmd_line = if command.ends_with('\n') {
            command.to_string()
        } else {
            format!("{command}\n")
        };
        if let Err(e) = self.writer.write_all(cmd_line.as_bytes()) {
            self.active = false;
            return Err(format!("写入失败 (shell 可能已退出): {e}"));
        }
        let _ = self.writer.flush();
        self.last_used = Instant::now();
        Ok(())
    }

    /// 写入任意文本
    pub fn write_raw(&mut self, text: &str) -> Result<usize, String> {
        self.refresh_active();
        if !self.active {
            return Err("shell session 已结束，无法写入".to_string());
        }
        if let Err(e) = self.writer.write_all(text.as_bytes()) {
            self.active = false;
            return Err(format!("写入失败 (shell 可能已退出): {e}"));
        }
        let _ = self.writer.flush();
        self.last_used = Instant::now();
        Ok(text.len())
    }

    pub fn has_running_command(&self) -> bool {
        self.current_command
            .as_ref()
            .map(|command| !command.command_done)
            .unwrap_or(false)
    }

    pub fn current_command_id(&self) -> Option<&str> {
        self.current_command
            .as_ref()
            .map(|command| command.id.as_str())
    }

    pub fn set_current_output(&mut self, output: String) -> Result<(), String> {
        let current = self
            .current_command
            .as_mut()
            .ok_or_else(|| "当前 session 没有运行中的 command".to_string())?;
        current.output = output;
        self.last_used = Instant::now();
        Ok(())
    }

    pub fn mark_current_timed_out(&mut self, output: String) -> Result<ShellCommandState, String> {
        let current = self
            .current_command
            .as_mut()
            .ok_or_else(|| "当前 session 没有运行中的 command".to_string())?;
        current.output = output;
        current.status = "timed_out".to_string();
        current.exit_code = None;
        current.command_done = false;
        current.timed_out = true;
        self.last_used = Instant::now();
        Ok(current.clone())
    }

    pub fn complete_current_command(
        &mut self,
        output: String,
        exit_code: i32,
    ) -> Result<ShellCommandState, String> {
        let mut current = self
            .current_command
            .take()
            .ok_or_else(|| "当前 session 没有运行中的 command".to_string())?;
        current.output = output;
        current.exit_code = Some(exit_code);
        current.command_done = true;
        current.timed_out = false;
        current.status = if exit_code == 0 {
            "completed".to_string()
        } else {
            "failed".to_string()
        };
        self.last_used = Instant::now();
        let snapshot = current.clone();
        self.push_history(current);
        Ok(snapshot)
    }

    pub fn interrupt_current_command(&mut self, output: String) -> Option<ShellCommandState> {
        let mut current = self.current_command.take()?;
        current.output = output;
        current.exit_code = None;
        current.command_done = true;
        current.timed_out = false;
        current.status = "interrupted".to_string();
        self.last_used = Instant::now();
        let snapshot = current.clone();
        self.push_history(current);
        Some(snapshot)
    }

    pub fn snapshot_command(
        &mut self,
        requested_id: Option<&str>,
    ) -> Result<(ShellCommandState, bool), String> {
        self.refresh_active();

        if let Some(current) = self.current_command.as_mut() {
            let matches = requested_id.map(|id| id == current.id).unwrap_or(true);
            if matches {
                return Ok((current.clone(), requested_id.is_none()));
            }
        }

        if let Some(command_id) = requested_id {
            if let Some(command) = self
                .command_history
                .iter()
                .rev()
                .find(|item| item.id == command_id)
            {
                return Ok((command.clone(), false));
            }
            return Err(format!("command 不存在: {command_id}"));
        }

        if let Some(command) = self.command_history.last() {
            return Ok((command.clone(), true));
        }

        Err("session 中没有 command".to_string())
    }

    pub fn refresh_active(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(Some(_)) => {
                self.active = false;
                false
            }
            Ok(None) => {
                self.active = true;
                true
            }
            Err(_) => self.active,
        }
    }

    /// 终止进程
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        self.active = false;
    }

    fn push_history(&mut self, command: ShellCommandState) {
        self.command_history.push(command);
        if self.command_history.len() > MAX_COMMAND_HISTORY {
            let overflow = self.command_history.len() - MAX_COMMAND_HISTORY;
            self.command_history.drain(0..overflow);
        }
    }
}
