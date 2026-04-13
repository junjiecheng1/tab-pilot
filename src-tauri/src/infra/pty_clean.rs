// PTY 输出清洗 — 去掉 ANSI 转义、shell banner、命令回显、prompt
//
// 被 shell.rs exec_command 调用, 确保返回给上层的 output 是纯净内容。

/// 清洗 PTY 输出
pub fn clean_pty_output(raw: &str, command: &str) -> String {
    // 1. 去掉 ANSI 转义序列
    let stripped = strip_ansi(raw);

    // 2. 按行过滤
    let cmd_trimmed = command.trim();
    let mut lines: Vec<&str> = Vec::new();

    for line in stripped.lines() {
        let trimmed = line.trim();

        // 跳过噪音行 (banner + prompt)
        if is_noise_line(trimmed) {
            continue;
        }

        // 跳过命令回显
        if trimmed == cmd_trimmed {
            continue;
        }

        lines.push(line);
    }

    // 去掉尾部空行
    while lines.last().map_or(false, |l| l.trim().is_empty()) {
        lines.pop();
    }

    lines.join("\n")
}

/// 去掉 ANSI 转义序列 (CSI / ESC)
fn strip_ansi(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                loop {
                    match chars.next() {
                        Some(c) if ('@'..='~').contains(&c) => break,
                        None => break,
                        _ => {}
                    }
                }
            } else {
                chars.next();
            }
        } else if ch == '\r' {
            continue;
        } else {
            result.push(ch);
        }
    }

    result
}

/// 判断是否为噪音行 (shell banner + prompt)
fn is_noise_line(line: &str) -> bool {
    // 空行
    if line.is_empty() {
        return false; // 空行保留，靠尾部清理
    }

    // ── Shell banner / macOS 提示 ──────────────
    if line.starts_with("The default interactive shell is now")
        || line.starts_with("To update your account to use zsh")
        || line.starts_with("For more details, please visit https://support.apple.com")
    {
        return true;
    }

    // ── Windows cmd.exe banner ──────────────
    if line.starts_with("Microsoft Windows")
        || line.starts_with("(c) Microsoft Corporation")
        || line.starts_with("(C) Microsoft Corporation")
    {
        return true;
    }

    // ── Shell prompt ──────────────────────────
    // bash-3.2$ cmd | bash$ cmd
    if line.starts_with("bash") && line.contains("$ ") {
        return true;
    }
    // 纯 prompt
    if line == "bash-3.2$" || line == "$" {
        return true;
    }
    // zsh prompt
    if line.ends_with('%') && line.len() < 80 {
        return true;
    }
    // Windows cmd prompt: C:\Users\xxx> 或 D:\workspace>
    if line.len() > 2
        && line.as_bytes().get(1) == Some(&b':')
        && line.as_bytes().get(2) == Some(&b'\\')
        && line.ends_with('>')
    {
        return true;
    }

    false
}
