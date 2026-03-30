// CLI 工具 + Toolkit 自动下载管理
//
// 首次启动时按需下载到 ~/.tabpilot/runtime/tools/
// Shell PTY 启动时将 tools/ 注入 PATH 前端，Agent 通过 bash 直接调用
//
// 两类工具:
//   1. 通用 CLI (单文件): rg, fd, jq, yq — 直接下载 bin
//   2. 打包工具 (目录):   markitdown — 下载 tar.gz 解压为目录
//
// 统一从 OSS 下载，你上传到 OSS 后即可用。
// 支持: macOS (arm64/x64), Linux (arm64/x64), Windows (x64)

use std::path::{Path, PathBuf};
use tokio::process::Command;

/// OSS 下载 URL 通过 ToolsManager::new() 传入 (来自 PilotConfig)

/// 平台标识 (对应 OSS 目录名)
fn platform_key() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("darwin-arm64"),
        ("macos", "x86_64") => Ok("darwin-x64"),
        ("linux", "x86_64") => Ok("linux-x64"),
        ("linux", "aarch64") => Ok("linux-arm64"),
        ("windows", "x86_64") => Ok("win-x64"),
        (os, arch) => Err(format!("不支持的平台: {os}-{arch}")),
    }
}

/// 工具类型
enum ToolKind {
    /// 单文件二进制 (rg, fd, jq, yq)
    /// OSS 路径: {platform}/{name}
    /// 下载到:   tools/{name}
    Binary,

    /// tar.gz 打包目录 (markitdown)
    /// OSS 路径: {platform}/{name}.tar.gz
    /// 解压到:   tools/{name}/  (入口 bin 在 tools/{name}/{name})
    Archive,
}

/// 工具清单
///
/// OSS 目录结构约定:
///   {OSS_BASE_URL}/
///   ├── darwin-arm64/
///   │   ├── rg                       ← Binary
///   │   ├── fd                       ← Binary
///   │   ├── jq                       ← Binary
///   │   ├── yq                       ← Binary
///   │   └── markitdown.tar.gz        ← Archive (解压后为 markitdown/ 目录)
///   ├── darwin-x64/
///   │   └── ...
///   ├── linux-x64/
///   │   └── ...
///   ├── linux-arm64/
///   │   └── ...
///   └── win-x64/
///       ├── rg.exe
///       ├── fd.exe
///       ├── jq.exe
///       ├── yq.exe
///       └── markitdown.tar.gz
// 第二个参数: ToolKind
// 第三个参数: 是否采用动态版本探测 (true: 去 OSS 读 {name}-version.txt 获取最新版并回源 history 目录，false: 直接下载默认目录)
fn tool_list() -> Vec<(&'static str, ToolKind, bool)> {
    vec![
        // ── 通用 CLI (单文件) ────────
        ("rg", ToolKind::Binary, false),
        ("fd", ToolKind::Binary, false),
        ("jq", ToolKind::Binary, false),
        ("yq", ToolKind::Binary, false),
        // ── 打包工具 (目录) ──────────
        ("markitdown", ToolKind::Archive, false),
        ("lark-cli", ToolKind::Archive, true), // 动态版本探测
    ]
}

/// CLI 工具管理器
pub struct ToolsManager {
    tools_dir: PathBuf,
    oss_url: String,
}

impl ToolsManager {
    pub fn new(_data_dir: &Path, oss_url: &str) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let tools_dir = home.join(".tabpilot").join("runtime").join("tools");
        Self { tools_dir, oss_url: oss_url.to_string() }
    }

    /// 从 home 目录创建 (默认路径, 默认 URL)
    pub fn default() -> Self {
        Self::new(
            &PathBuf::from("/unused"),
            "https://lingostatic.tweet.net.cn/tools/tabpilot-tools",
        )
    }

    /// tools 目录路径 (供 Shell PATH 注入)
    pub fn tools_dir(&self) -> &Path {
        &self.tools_dir
    }

    /// 生成完整 PATH 列表 (包含子目录工具的路径)
    /// Shell 启动时注入: tools/ + tools/markitdown/ + ...
    pub fn path_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.tools_dir.clone()];
        // Archive 类型工具的子目录也加入 PATH
        for (name, kind) in tool_list() {
            if matches!(kind, ToolKind::Archive) {
                let sub_dir = self.tools_dir.join(name);
                if sub_dir.exists() {
                    dirs.push(sub_dir);
                }
            }
        }
        dirs
    }

    /// 所有工具是否就绪 (逐个检查)
    pub fn is_ready(&self) -> bool {
        for (name, kind) in tool_list() {
            let exists = match kind {
                ToolKind::Binary => {
                    let bin_name = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
                    self.tools_dir.join(&bin_name).exists()
                }
                ToolKind::Archive => {
                    let entry = if cfg!(windows) {
                        self.tools_dir.join(name).join(format!("{name}.exe"))
                    } else {
                        self.tools_dir.join(name).join(name)
                    };
                    entry.exists()
                }
            };
            if !exists {
                return false;
            }
        }
        true
    }

    /// 按需下载全部工具 (幂等, 后台调用)
    pub async fn ensure_ready(&self) -> Result<(), String> {
        if self.is_ready() {
            return Ok(());
        }

        let platform = platform_key()?;

        std::fs::create_dir_all(&self.tools_dir)
            .map_err(|e| format!("创建 tools 目录失败: {e}"))?;

        for (name, kind, dynamic_version) in tool_list() {
            let mut prefix = self.oss_url.clone();

            if dynamic_version {
                let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                let version_url = format!("{}/{platform}/{name}-version.txt?_t={ts}", self.oss_url);
                match reqwest::get(&version_url).await {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(v) = resp.text().await {
                            let v = v.trim();
                            if !v.is_empty() {
                                log::info!("[Tools] 解析 {name} 动态版本: {v}");
                                prefix = format!("{}/history/{}", self.oss_url, v);
                            }
                        }
                    }
                    _ => log::warn!("[Tools] 无法获取 {name} 的动态版本号，回退到默认路径"),
                }
            }

            match kind {
                ToolKind::Binary => {
                    let bin_name = if cfg!(windows) {
                        format!("{name}.exe")
                    } else {
                        name.to_string()
                    };
                    let bin_path = self.tools_dir.join(&bin_name);

                    if bin_path.exists() {
                        log::info!("[Tools] {name} 已存在, 跳过");
                        continue;
                    }

                    let url = format!("{}/{platform}/{bin_name}", prefix);
                    log::info!("[Tools] 下载 {name}: {url}");

                    match self.download_binary(&url, &bin_path).await {
                        Ok(_) => log::info!("[Tools] {name} ✅"),
                        Err(e) => {
                            log::warn!("[Tools] {name} 下载失败 (非致命): {e}");
                            continue;
                        }
                    }
                }
                ToolKind::Archive => {
                    let dest_dir = self.tools_dir.join(name);

                    // 检查入口 bin 是否存在
                    let entry_bin = if cfg!(windows) {
                        dest_dir.join(format!("{name}.exe"))
                    } else {
                        dest_dir.join(name)
                    };

                    if entry_bin.exists() {
                        log::info!("[Tools] {name} 已存在, 跳过");
                        continue;
                    }

                    // 确保 dest_dir 存在
                    let _ = std::fs::create_dir_all(&dest_dir);

                    let url = format!("{}/{platform}/{name}.tar.gz", prefix);
                    log::info!("[Tools] 下载 {name} (archive): {url}");

                    match self.download_and_extract(&url, &dest_dir).await {
                        Ok(_) => log::info!("[Tools] {name} ✅"),
                        Err(e) => {
                            log::warn!("[Tools] {name} 下载失败 (非致命): {e}");
                            continue;
                        }
                    }
                }
            }
        }

        log::info!("[Tools] 全部工具就绪 ✅");
        Ok(())
    }

    /// 下载单个二进制文件
    async fn download_binary(&self, url: &str, dest: &Path) -> Result<(), String> {
        let resp = reqwest::get(url)
            .await
            .map_err(|e| format!("请求失败: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("读取失败: {e}"))?;

        std::fs::write(dest, &bytes)
            .map_err(|e| format!("写入失败: {e}"))?;

        // chmod +x (Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("chmod 失败: {e}"))?;
        }

        Ok(())
    }

    /// 下载 tar.gz 并解压到目标目录
    async fn download_and_extract(&self, url: &str, dest_dir: &Path) -> Result<(), String> {
        let resp = reqwest::get(url)
            .await
            .map_err(|e| format!("请求失败: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("读取失败: {e}"))?;

        // 写临时文件
        let tmp_path = dest_dir.join(".download.tar.gz");
        std::fs::write(&tmp_path, &bytes)
            .map_err(|e| format!("写入临时文件失败: {e}"))?;

        // 解压: tar -xzf xxx.tar.gz -C dest_dir
        let status = Command::new("tar")
            .args(["-xzf", tmp_path.to_str().unwrap_or(""), "-C", dest_dir.to_str().unwrap_or("")])
            .status()
            .await
            .map_err(|e| format!("tar 解压失败: {e}"))?;

        // 清理临时文件
        let _ = std::fs::remove_file(&tmp_path);

        if !status.success() {
            return Err("tar 解压返回非零状态".to_string());
        }

        // chmod +x 入口文件 (Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // 遍历解压目录，给所有可执行文件加权限
            if let Ok(entries) = std::fs::read_dir(dest_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        // 子目录中的入口 bin
                        let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
                        let bin_path = path.join(dir_name.as_ref());
                        if bin_path.exists() {
                            let _ = std::fs::set_permissions(&bin_path, std::fs::Permissions::from_mode(0o755));
                        }
                    }
                }
            }
        }

        Ok(())
    }

}
