// ToolGuard — 命令级安全门控
//
// 三层判断: 黑名单 → 白名单 → 已记住 → 按模式

use regex::Regex;
use std::collections::HashSet;
use std::path::PathBuf;
use log;

/// 安全判断结果
#[derive(Debug, Clone, PartialEq)]
pub enum GuardDecision {
    Allow,
    Confirm,
    Deny,
}

impl std::fmt::Display for GuardDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Allow => write!(f, "allow"),
            Self::Confirm => write!(f, "confirm"),
            Self::Deny => write!(f, "deny"),
        }
    }
}

/// 黑名单命令 (正则)
const BLACKLIST_PATTERNS: &[&str] = &[
    r"rm\s+-rf\s+/",
    r"sudo\s+rm",
    r"mkfs",
    r"chmod\s+777",
    r":\(\)\s*\{",
    r"dd\s+if=",
    r">\s*/dev/sd",
    r"format\s+[a-zA-Z]:",
];

/// 白名单命令前缀
const WHITELIST_PREFIXES: &[&str] = &[
    "ls", "cat", "pwd", "whoami", "echo", "date", "uname",
    "git status", "git log", "git diff", "git branch", "git show",
    "node --version", "python --version", "npm --version",
    "which", "type", "file", "head", "tail", "wc",
    "find", "grep", "tree", "env", "printenv", "dir",
];

/// 安全门控
pub struct ToolGuard {
    mode: String,
    blacklist: Vec<Regex>,
    protected_paths: Vec<String>,
    remembered: HashSet<String>,
    db_path: PathBuf,
}

impl ToolGuard {
    pub fn new(mode: &str, data_dir: &PathBuf, os_name: &str) -> Self {
        let blacklist = BLACKLIST_PATTERNS
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        let raw_paths: &[&str] = match os_name {
            "darwin" => &["/etc", "/System", "/Library"],
            "windows" => &["C:\\Windows", "C:\\Program Files"],
            _ => &["/etc", "/root", "/boot"],
        };

        let home = dirs::home_dir().unwrap_or_default();
        let mut protected: Vec<String> = raw_paths
            .iter()
            .map(|p| p.to_string())
            .collect();
        protected.push(format!("{}/.ssh", home.display()));
        protected.push(format!("{}/.gnupg", home.display()));

        Self {
            mode: mode.to_string(),
            blacklist,
            protected_paths: protected,
            remembered: HashSet::new(),
            db_path: data_dir.join("guard.db"),
        }
    }

    /// 当前模式
    pub fn mode(&self) -> &str {
        &self.mode
    }

    /// 设置模式
    pub fn set_mode(&mut self, mode: &str) {
        self.mode = mode.to_string();
    }

    /// 从 SQLite 加载已记住命令
    pub async fn load_remembered(&mut self) {
        let db = match sqlx::SqlitePool::connect(
            &format!("sqlite:{}?mode=rwc", self.db_path.display())
        ).await {
            Ok(db) => db,
            Err(e) => {
                log::warn!("[Guard] 数据库连接失败: {e}");
                return;
            }
        };

        let _ = sqlx::query(
            "CREATE TABLE IF NOT EXISTS remembered_commands (prefix TEXT PRIMARY KEY, added_at REAL)"
        ).execute(&db).await;

        if let Ok(rows) = sqlx::query_as::<_, (String,)>(
            "SELECT prefix FROM remembered_commands"
        ).fetch_all(&db).await {
            self.remembered = rows.into_iter().map(|r| r.0).collect();
            log::info!("[Guard] 加载 {} 条已记住命令", self.remembered.len());
        }
    }

    /// 记住命令前缀
    pub async fn remember(&mut self, command: &str) {
        let prefix = command.split_whitespace().next().unwrap_or(command).to_string();
        self.remembered.insert(prefix.clone());

        if let Ok(db) = sqlx::SqlitePool::connect(
            &format!("sqlite:{}?mode=rwc", self.db_path.display())
        ).await {
            let now = chrono::Utc::now().timestamp() as f64;
            let _ = sqlx::query(
                "INSERT OR REPLACE INTO remembered_commands VALUES (?, ?)"
            ).bind(&prefix).bind(now).execute(&db).await;
        }
    }

    /// 清空已记住命令
    pub async fn clear_remembered(&mut self) {
        self.remembered.clear();
        if let Ok(db) = sqlx::SqlitePool::connect(
            &format!("sqlite:{}?mode=rwc", self.db_path.display())
        ).await {
            let _ = sqlx::query("DELETE FROM remembered_commands")
                .execute(&db).await;
        }
    }

    /// 获取已记住列表
    pub fn get_remembered(&self) -> Vec<String> {
        self.remembered.iter().cloned().collect()
    }

    /// 移除一条已记住命令
    pub async fn remove_remembered(&mut self, prefix: &str) {
        self.remembered.remove(prefix);
        if let Ok(db) = sqlx::SqlitePool::connect(
            &format!("sqlite:{}?mode=rwc", self.db_path.display())
        ).await {
            let _ = sqlx::query("DELETE FROM remembered_commands WHERE prefix = ?")
                .bind(prefix).execute(&db).await;
        }
    }

    /// 检查安全性
    pub fn check(&self, tool_type: &str, args: &serde_json::Value) -> GuardDecision {
        match tool_type {
            "shell" => self.check_shell(args),
            "file" => self.check_file(args),
            _ => GuardDecision::Confirm,
        }
    }

    /// 获取保护路径列表
    pub fn protected_paths(&self) -> &[String] {
        &self.protected_paths
    }

    fn check_shell(&self, args: &serde_json::Value) -> GuardDecision {
        let command = args["command"].as_str().unwrap_or("");

        // 1. 黑名单
        for pattern in &self.blacklist {
            if pattern.is_match(command) {
                return GuardDecision::Deny;
            }
        }

        // 2. 白名单
        let trimmed = command.trim();
        for prefix in WHITELIST_PREFIXES {
            if trimmed.starts_with(prefix) {
                return GuardDecision::Allow;
            }
        }

        // 3. 已记住
        if let Some(first) = trimmed.split_whitespace().next() {
            if self.remembered.contains(first) {
                return GuardDecision::Allow;
            }
        }

        // 4. 按模式
        self.decide_by_mode(true)
    }

    fn check_file(&self, args: &serde_json::Value) -> GuardDecision {
        let path = args["path"].as_str().unwrap_or("");
        let action = args["action"].as_str().unwrap_or("");

        // 保护路径检查
        for protected in &self.protected_paths {
            if path.starts_with(protected) {
                if action == "write" {
                    return GuardDecision::Deny;
                }
                return GuardDecision::Confirm;
            }
        }

        self.decide_by_mode(action == "write")
    }

    fn decide_by_mode(&self, is_write: bool) -> GuardDecision {
        match self.mode.as_str() {
            "conservative" => GuardDecision::Confirm,
            "trust" => GuardDecision::Allow,
            _ => {
                if is_write { GuardDecision::Confirm } else { GuardDecision::Allow }
            }
        }
    }
}
