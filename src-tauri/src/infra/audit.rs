// 审计日志 — SQLite 本地记录
//
// 分两步写入: log_start (执行前) → log_finish (执行后)
// 这样即使执行中崩溃, 也能看到"开始了但没完成"的记录

use std::path::PathBuf;
use log;

/// 审计日志
pub struct AuditLog {
    db_url: String,
}

impl AuditLog {
    pub fn new(data_dir: &PathBuf) -> Self {
        let db_path = data_dir.join("audit.db");
        Self {
            db_url: format!("sqlite:{}?mode=rwc", db_path.display()),
        }
    }

    /// 初始化表 (新增 status 字段区分执行状态)
    pub async fn init(&self) {
        let db = match sqlx::SqlitePool::connect(&self.db_url).await {
            Ok(db) => db,
            Err(e) => {
                log::warn!("[Audit] 数据库连接失败: {e}");
                return;
            }
        };

        let _ = sqlx::query(
            "CREATE TABLE IF NOT EXISTS audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp REAL NOT NULL,
                tool_type TEXT NOT NULL,
                action TEXT NOT NULL,
                args_json TEXT,
                result TEXT,
                exit_code INTEGER,
                duration REAL,
                guard_decision TEXT,
                status TEXT DEFAULT 'started'
            )"
        ).execute(&db).await;

        // 迁移: 旧表可能没有 status 列
        let _ = sqlx::query(
            "ALTER TABLE audit_log ADD COLUMN status TEXT DEFAULT 'completed'"
        ).execute(&db).await;

        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp DESC)"
        ).execute(&db).await;
    }

    /// 执行前记录 — 返回 log_id, 用于 log_finish 更新
    pub async fn log_start(
        &self,
        tool_type: &str,
        action: &str,
        args: &serde_json::Value,
        guard_decision: &str,
    ) -> Option<i64> {
        let db = match sqlx::SqlitePool::connect(&self.db_url).await {
            Ok(db) => db,
            Err(_) => return None,
        };

        let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
        let args_json = serde_json::to_string(args).unwrap_or_default();

        let result = sqlx::query(
            "INSERT INTO audit_log (timestamp, tool_type, action, args_json, guard_decision, status)
             VALUES (?, ?, ?, ?, ?, 'started')"
        )
        .bind(now)
        .bind(tool_type)
        .bind(action)
        .bind(&args_json)
        .bind(guard_decision)
        .execute(&db)
        .await;

        match result {
            Ok(r) => Some(r.last_insert_rowid()),
            Err(_) => None,
        }
    }

    /// 执行后更新 — 补充 result, exit_code, duration, status
    pub async fn log_finish(
        &self,
        log_id: i64,
        result: &str,
        exit_code: i32,
        duration: f64,
        status: &str,
    ) {
        let db = match sqlx::SqlitePool::connect(&self.db_url).await {
            Ok(db) => db,
            Err(_) => return,
        };

        let _ = sqlx::query(
            "UPDATE audit_log SET result = ?, exit_code = ?, duration = ?, status = ?
             WHERE id = ?"
        )
        .bind(result)
        .bind(exit_code)
        .bind(duration)
        .bind(status)
        .bind(log_id)
        .execute(&db)
        .await;
    }

    /// 兼容旧 API: 一步写入 (用于 guard deny 等不需要分步的场景)
    pub async fn log(
        &self,
        tool_type: &str,
        action: &str,
        args: &serde_json::Value,
        result: &str,
        exit_code: i32,
        duration: f64,
        guard_decision: &str,
    ) {
        let db = match sqlx::SqlitePool::connect(&self.db_url).await {
            Ok(db) => db,
            Err(_) => return,
        };

        let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
        let args_json = serde_json::to_string(args).unwrap_or_default();

        let _ = sqlx::query(
            "INSERT INTO audit_log (timestamp, tool_type, action, args_json, result, exit_code, duration, guard_decision, status)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'completed')"
        )
        .bind(now)
        .bind(tool_type)
        .bind(action)
        .bind(&args_json)
        .bind(result)
        .bind(exit_code)
        .bind(duration)
        .bind(guard_decision)
        .execute(&db)
        .await;
    }

    /// 查询日志
    pub async fn query(&self, limit: i64) -> Vec<serde_json::Value> {
        let db = match sqlx::SqlitePool::connect(&self.db_url).await {
            Ok(db) => db,
            Err(_) => return vec![],
        };

        let rows: Vec<(i64, f64, String, String, Option<String>, Option<String>, Option<i32>, Option<f64>, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT id, timestamp, tool_type, action, args_json, result, exit_code, duration, guard_decision, status
                 FROM audit_log ORDER BY timestamp DESC LIMIT ?"
            )
            .bind(limit)
            .fetch_all(&db)
            .await
            .unwrap_or_default();

        rows.into_iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.0,
                    "timestamp": r.1,
                    "tool_type": r.2,
                    "action": r.3,
                    "args_json": r.4,
                    "result": r.5,
                    "exit_code": r.6,
                    "duration": r.7,
                    "guard_decision": r.8,
                    "status": r.9,
                })
            })
            .collect()
    }
}
