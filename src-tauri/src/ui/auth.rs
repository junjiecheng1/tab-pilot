// Token + 设备管理
//
// 单一内存源 + 文件持久化，不再有多处同步问题
// device_id: 首次启动生成 UUID，持久化到 data_dir/device_id

use log;
use std::path::PathBuf;

/// 认证管理
pub struct AuthManager {
    token_file: PathBuf,
    token: Option<String>,
    challenge: Option<String>,
    pub user_id: String,
    pub user_display: String,
    /// 设备唯一 ID (持久化)
    pub device_id: String,
    /// 设备名 (hostname)
    pub device_name: String,
}

impl AuthManager {
    pub fn new(data_dir: &PathBuf) -> Self {
        let token_file = data_dir.join("auth_token.json");
        let token = Self::load_from_file(&token_file);
        if token.is_some() {
            log::info!("[Auth] 加载已存储 Token");
        }

        // device_id: 基于硬件指纹的确定性 8 位 ID
        let device_id = Self::generate_device_id();
        log::info!("[Auth] device_id: {}", device_id);

        // hostname
        let device_name = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        Self {
            token_file,
            token,
            challenge: None,
            user_id: String::new(),
            user_display: String::new(),
            device_id,
            device_name,
        }
    }

    /// 获取 token
    pub fn get_token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// 保存 token (内存 + 文件, 一步到位)
    pub fn save_token(&mut self, token: String) {
        self.token = Some(token.clone());
        if let Some(parent) = self.token_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let data = serde_json::json!({ "token": token });
        if let Err(e) = std::fs::write(&self.token_file, data.to_string()) {
            log::error!("[Auth] 保存 Token 失败: {}", e);
        } else {
            log::info!("[Auth] Token 已保存");
        }
    }

    /// 清除 token (内存 + 文件 + 用户信息, 一步到位)
    pub fn clear_token(&mut self) {
        self.token = None;
        self.user_id.clear();
        self.user_display.clear();
        if self.token_file.exists() {
            let _ = std::fs::remove_file(&self.token_file);
            log::info!("[Auth] Token 已清除");
        }
    }

    /// 生成 challenge (用于 OAuth 回调验证)
    pub fn set_challenge(&mut self) -> String {
        let challenge = uuid::Uuid::new_v4().to_string();
        self.challenge = Some(challenge.clone());
        challenge
    }

    /// 验证 challenge (一次性)
    pub fn verify_challenge(&mut self, c: &str) -> bool {
        if let Some(expected) = &self.challenge {
            if expected == c {
                self.challenge = None;
                return true;
            }
        }
        false
    }

    /// 从文件加载 token
    fn load_from_file(path: &PathBuf) -> Option<String> {
        if !path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(path).ok()?;
        let data: serde_json::Value = serde_json::from_str(&content).ok()?;
        data["token"].as_str().map(|s| s.to_string())
    }

    /// 异步获取用户信息
    pub async fn fetch_user_info(&mut self, http_url: &str) {
        let token = match &self.token {
            Some(t) if !t.is_empty() => t.clone(),
            _ => return,
        };
        let client = reqwest::Client::new();
        match client
            .get(format!("{}/api/auth/me", http_url))
            .header("Authorization", format!("Bearer {}", token))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    self.user_id = body["user_id"].as_str().unwrap_or("").to_string();
                    self.user_display = body["display_name"]
                        .as_str()
                        .or_else(|| body["phone"].as_str())
                        .unwrap_or("")
                        .to_string();
                    log::info!("[Auth] 用户信息: {} ({})", self.user_display, self.user_id);
                }
            }
            Ok(resp) => {
                log::warn!("[Auth] 获取用户信息失败: {}", resp.status());
            }
            Err(e) => {
                log::warn!("[Auth] 获取用户信息异常: {}", e);
            }
        }
    }

    /// 生成确定性 device_id (8 位 hex)
    ///
    /// 算法: SHA256(hardware_uuid + "tabpilot")[:8]
    /// macOS: ioreg 获取 IOPlatformUUID
    /// 其他: hostname 作为种子
    fn generate_device_id() -> String {
        let seed = Self::get_hardware_uuid().unwrap_or_else(|| {
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string())
        });

        // SHA256(seed + salt) → 取前 8 位 hex
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        format!("{}:tabpilot", seed).hash(&mut hasher);
        let hash = hasher.finish();
        format!("{:08x}", hash & 0xFFFFFFFF)
    }

    /// macOS: 获取硬件 UUID (IOPlatformUUID)
    fn get_hardware_uuid() -> Option<String> {
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;
            let output = Command::new("ioreg")
                .args(["-rd1", "-c", "IOPlatformExpertDevice"])
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if line.contains("IOPlatformUUID") {
                    // 格式: "IOPlatformUUID" = "XXXXXXXX-..."
                    if let Some(uuid) = line.split('"').nth(3) {
                        return Some(uuid.to_string());
                    }
                }
            }
        }
        None
    }
}

/// 独立函数: 轮询后端 auth-poll 获取 token
///
/// 每 3 秒轮询一次, 最多 5 分钟
/// 拿到 token → save_token + wake connector + confirm challenge
pub async fn poll_for_token(
    challenge: String,
    api_base: String,
    auth: std::sync::Arc<tokio::sync::RwLock<AuthManager>>,
    connector: std::sync::Arc<crate::router::connector::Connector>,
) {
    let client = reqwest::Client::new();
    let poll_url = format!("{}/api/pilot/auth-poll?challenge={}", api_base, challenge);
    let confirm_url = format!("{}/api/pilot/auth-confirm", api_base);
    let short = &challenge[..8.min(challenge.len())];

    log::info!("[AuthPoll] 开始轮询: {}...", short);

    for attempt in 0..100 {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // deep link 已先到 → 停止
        if auth.read().await.get_token().is_some() {
            log::info!("[AuthPoll] 已有 token (deep link), 停止轮询");
            return;
        }

        match client.get(&poll_url).send().await {
            Ok(resp) => {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(token) = body["token"].as_str() {
                        if !token.is_empty() {
                            log::info!("[AuthPoll] 轮询到 token (attempt {})", attempt + 1);
                            auth.write().await.save_token(token.to_string());
                            connector.wake();

                            // 确认 challenge
                            let _ = client
                                .post(&confirm_url)
                                .header("Authorization", format!("Bearer {}", token))
                                .json(&serde_json::json!({"challenge": challenge}))
                                .send()
                                .await;

                            log::info!("[AuthPoll] 授权完成");
                            return;
                        }
                    }
                }
            }
            Err(e) => {
                log::debug!("[AuthPoll] 轮询失败 (attempt {}): {}", attempt + 1, e);
            }
        }
    }

    log::warn!("[AuthPoll] 轮询超时 (5分钟), 放弃");
}
