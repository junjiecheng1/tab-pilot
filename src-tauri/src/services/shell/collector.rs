// 输出收集器 — 线程安全的 PTY 输出收集 + Prompt 检测
//
// 解决旧实现的两个问题:
//   1. try_lock 失败时丢弃输出
//   2. buf.join("") 每次轮询拼全量 → O(n²)

use std::io::Read;
use std::sync::{Arc, Mutex};

/// 尾部缓冲大小 (用于 prompt 检测)
const TAIL_SIZE: usize = 32;

/// PTY 输出收集器
///
/// 内部使用 Mutex (std, 非 tokio) 以便在同步读取线程中安全写入
pub struct OutputCollector {
    /// 完整输出
    buffer: Arc<Mutex<String>>,
    /// 最后 N 字符 (用于 prompt 检测，避免检查全量)
    tail: Arc<Mutex<String>>,
}

impl OutputCollector {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(String::with_capacity(4096))),
            tail: Arc::new(Mutex::new(String::new())),
        }
    }

    /// 追加输出 (从读取线程调用, 阻塞 Mutex)
    ///
    /// 使用 std::sync::Mutex 而非 try_lock:
    ///   - 锁持有时间极短 (只做 push_str)
    ///   - 保证不丢数据
    pub fn push(&self, text: &str) {
        if let Ok(mut buf) = self.buffer.lock() {
            buf.push_str(text);
        }
        if let Ok(mut tail) = self.tail.lock() {
            tail.push_str(text);
            // 只保留尾部 (确保在 char 边界切割)
            if tail.len() > TAIL_SIZE * 2 {
                let mut start = tail.len() - TAIL_SIZE;
                // 向后找到有效的 UTF-8 char 边界
                while start < tail.len() && !tail.is_char_boundary(start) {
                    start += 1;
                }
                *tail = tail[start..].to_string();
            }
        }
    }

    /// 清空缓冲 (新命令前调用)
    pub fn clear(&self) {
        if let Ok(mut buf) = self.buffer.lock() {
            buf.clear();
        }
        if let Ok(mut tail) = self.tail.lock() {
            tail.clear();
        }
    }

    /// 取出完整输出
    pub fn take(&self) -> String {
        self.buffer.lock().map(|b| b.clone()).unwrap_or_default()
    }

    /// 检查命令是否执行完成 (看到 shell prompt)
    /// 注意: 此方法已不被 exec_in_session 使用 (改用 contains_marker)
    /// 保留用于其他场景的兼容
    pub fn is_command_done(&self) -> bool {
        if let Ok(tail) = self.tail.lock() {
            // 去掉末尾的 \r\n 再检查
            let trimmed = tail.trim_end();
            trimmed.ends_with("$ ")
                || trimmed.ends_with("% ")
                || trimmed.ends_with("# ")
                || trimmed.ends_with("$")
                || trimmed.ends_with("%")
                || trimmed.ends_with("#")
        } else {
            false
        }
    }

    /// 检查输出中是否包含指定的结束标记 (排除命令回显)
    ///
    /// PTY 回显: `...cmd; echo __DONE_xxx__` → marker 前面是 "echo "
    /// 实际输出: `\n__DONE_xxx__` → marker 前面是换行符
    pub fn contains_marker(&self, marker: &str) -> bool {
        if let Ok(buf) = self.buffer.lock() {
            let needle = format!("\n{}", marker);
            buf.contains(&needle)
        } else {
            false
        }
    }

    /// 启动 PTY 读取线程
    ///
    /// 返回一个 handle, reader 结束时线程自动退出
    pub fn spawn_reader(
        &self,
        reader: Box<dyn Read + Send>,
        session_id: String,
    ) {
        let collector = OutputCollector {
            buffer: Arc::clone(&self.buffer),
            tail: Arc::clone(&self.tail),
        };
        std::thread::spawn(move || {
            read_loop(reader, collector, session_id);
        });
    }
}

/// PTY 读取循环 (同步线程)
fn read_loop(
    mut reader: Box<dyn Read + Send>,
    collector: OutputCollector,
    session_id: String,
) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let text = String::from_utf8_lossy(&buf[..n]);
                collector.push(&text);
            }
            Err(e) => {
                log::debug!("[Shell] PTY 读取结束 ({}): {}", session_id, e);
                break;
            }
        }
    }
}
