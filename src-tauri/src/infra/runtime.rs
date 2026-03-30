// RuntimeManager — Node.js 共享运行时管理
//
// 按需下载 portable Node.js + 安装 Playwright Chromium
// Browser 和 MCP 共享同一份 Node 运行时

use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Node.js 版本
const NODE_VERSION: &str = "20.11.1";

/// 安装进度
#[derive(Debug, Clone)]
pub enum SetupProgress {
    /// 检查已有安装
    Checking,
    /// 下载 Node.js
    DownloadingNode { percent: u8 },
    /// 解压 Node.js
    ExtractingNode,
    /// 安装 npm 包 (playwright-core)
    InstallingPackages,
    /// 下载 Chromium
    InstallingChromium,
    /// 复制脚本
    CopyingScripts,
    /// 完成
    Ready,
}

/// 运行时错误
#[derive(Debug)]
pub enum RuntimeError {
    /// 网络下载失败
    DownloadFailed(String),
    /// 解压失败
    ExtractFailed(String),
    /// npm 安装失败
    PackageInstallFailed(String),
    /// Chromium 安装失败
    ChromiumInstallFailed(String),
    /// 文件系统错误
    IoError(String),
    /// 不支持的平台
    UnsupportedPlatform(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DownloadFailed(m) => write!(f, "下载失败: {m}"),
            Self::ExtractFailed(m) => write!(f, "解压失败: {m}"),
            Self::PackageInstallFailed(m) => write!(f, "包安装失败: {m}"),
            Self::ChromiumInstallFailed(m) => write!(f, "Chromium 安装失败: {m}"),
            Self::IoError(m) => write!(f, "IO 错误: {m}"),
            Self::UnsupportedPlatform(m) => write!(f, "不支持的平台: {m}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Node.js 共享运行时管理器
pub struct RuntimeManager {
    /// 运行时根目录 (~/.tabpilot/runtime/)
    runtime_dir: PathBuf,
    /// 数据目录 (~/.tabpilot/data/)
    data_dir: PathBuf,
}

impl RuntimeManager {
    pub fn new(data_dir: &Path) -> Self {
        // runtime 目录和 data 目录同级
        let runtime_dir = data_dir
            .parent()
            .unwrap_or(data_dir)
            .join("runtime");

        Self {
            runtime_dir,
            data_dir: data_dir.to_path_buf(),
        }
    }

    /// 环境是否就绪
    pub fn is_ready(&self) -> bool {
        self.runtime_dir.join(".ready").exists()
    }

    /// Node 可执行文件路径
    pub fn node_bin(&self) -> PathBuf {
        if cfg!(windows) {
            self.runtime_dir.join("node").join("node.exe")
        } else {
            self.runtime_dir.join("node").join("bin").join("node")
        }
    }

    /// npx 可执行文件路径
    pub fn npx_bin(&self) -> PathBuf {
        if cfg!(windows) {
            self.runtime_dir.join("node").join("npx.cmd")
        } else {
            self.runtime_dir.join("node").join("bin").join("npx")
        }
    }

    /// 脚本目录
    pub fn scripts_dir(&self) -> PathBuf {
        self.runtime_dir.join("scripts")
    }

    /// Playwright browsers 路径
    pub fn playwright_browsers(&self) -> PathBuf {
        self.runtime_dir.join("playwright")
    }

    /// Chrome profile 路径 (持久化登录态)
    pub fn chrome_profile(&self) -> PathBuf {
        self.data_dir.join("chrome-profile")
    }

    /// 运行时根目录
    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    /// 按需安装 — 幂等, 已安装跳过
    pub async fn ensure_ready<F: Fn(SetupProgress)>(
        &self, on_progress: F,
    ) -> Result<(), RuntimeError> {
        if self.is_ready() {
            // 即使已安装, 也更新脚本 (开发时改 JS 无需重装)
            let _ = self.copy_scripts();
            return Ok(());
        }

        on_progress(SetupProgress::Checking);
        log::info!("[Runtime] 开始安装 Node.js 运行时...");

        // 确保目录存在
        std::fs::create_dir_all(&self.runtime_dir)
            .map_err(|e| RuntimeError::IoError(e.to_string()))?;

        // ① 下载 Node.js
        on_progress(SetupProgress::DownloadingNode { percent: 0 });
        self.download_node().await?;

        // ② 安装 playwright-core
        on_progress(SetupProgress::InstallingPackages);
        self.install_packages().await?;

        // ③ 下载 Chromium
        on_progress(SetupProgress::InstallingChromium);
        self.install_chromium().await?;

        // ④ 复制脚本
        on_progress(SetupProgress::CopyingScripts);
        self.copy_scripts()?;

        // ⑤ 标记就绪
        let version_file = self.runtime_dir.join(".node-version");
        std::fs::write(&version_file, NODE_VERSION)
            .map_err(|e| RuntimeError::IoError(e.to_string()))?;
        std::fs::write(self.runtime_dir.join(".ready"), "ok")
            .map_err(|e| RuntimeError::IoError(e.to_string()))?;

        on_progress(SetupProgress::Ready);
        log::info!("[Runtime] 安装完成 ✅");
        Ok(())
    }

    /// 下载 portable Node.js
    async fn download_node(&self) -> Result<(), RuntimeError> {
        let (url, ext) = node_download_url()?;
        let archive_path = self.runtime_dir.join(format!("node-download.{ext}"));

        log::info!("[Runtime] 下载 Node.js: {}", url);

        // 使用 reqwest 下载
        let resp = reqwest::get(&url).await
            .map_err(|e| RuntimeError::DownloadFailed(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RuntimeError::DownloadFailed(
                format!("HTTP {}", resp.status()),
            ));
        }

        let bytes = resp.bytes().await
            .map_err(|e| RuntimeError::DownloadFailed(e.to_string()))?;

        std::fs::write(&archive_path, &bytes)
            .map_err(|e| RuntimeError::IoError(e.to_string()))?;

        log::info!("[Runtime] 下载完成, 解压中...");

        // 解压
        let node_dir = self.runtime_dir.join("node");
        std::fs::create_dir_all(&node_dir)
            .map_err(|e| RuntimeError::IoError(e.to_string()))?;

        if cfg!(windows) {
            // Windows: zip 解压
            self.extract_zip(&archive_path, &node_dir).await?;
        } else {
            // macOS / Linux: tar.gz 解压
            self.extract_tar_gz(&archive_path, &node_dir).await?;
        }

        // 清理下载文件
        let _ = std::fs::remove_file(&archive_path);

        // 验证 node 可执行
        let output = Command::new(self.node_bin())
            .arg("--version")
            .output()
            .await
            .map_err(|e| RuntimeError::ExtractFailed(
                format!("node --version 失败: {e}"),
            ))?;

        let version = String::from_utf8_lossy(&output.stdout);
        log::info!("[Runtime] Node.js 已安装: {}", version.trim());

        Ok(())
    }

    /// 解压 tar.gz (macOS / Linux)
    async fn extract_tar_gz(&self, archive: &Path, target: &Path) -> Result<(), RuntimeError> {
        let status = Command::new("tar")
            .args([
                "-xzf",
                &archive.to_string_lossy(),
                "--strip-components=1",
                "-C",
                &target.to_string_lossy(),
            ])
            .status()
            .await
            .map_err(|e| RuntimeError::ExtractFailed(e.to_string()))?;

        if !status.success() {
            return Err(RuntimeError::ExtractFailed("tar 解压失败".to_string()));
        }
        Ok(())
    }

    /// 解压 zip (Windows) — 处理嵌套目录
    async fn extract_zip(&self, archive: &Path, target: &Path) -> Result<(), RuntimeError> {
        // 先解压到临时目录
        let temp_dir = self.runtime_dir.join("_node_extract_tmp");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| RuntimeError::IoError(e.to_string()))?;

        let status = Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                    archive.display(),
                    temp_dir.display(),
                ),
            ])
            .status()
            .await
            .map_err(|e| RuntimeError::ExtractFailed(e.to_string()))?;

        if !status.success() {
            return Err(RuntimeError::ExtractFailed("zip 解压失败".to_string()));
        }

        // Node.js zip 内部有嵌套目录 (node-vXX-win-x64/), 移动内容到 target
        let _move_status = Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "Get-ChildItem -Path '{}' -Directory | ForEach-Object {{ \
                     Get-ChildItem $_.FullName | Move-Item -Destination '{}' -Force }}",
                    temp_dir.display(),
                    target.display(),
                ),
            ])
            .status()
            .await;

        let _ = std::fs::remove_dir_all(&temp_dir);
        Ok(())
    }

    /// 安装 npm 包
    async fn install_packages(&self) -> Result<(), RuntimeError> {
        log::info!("[Runtime] 安装 playwright-core...");

        let npm = if cfg!(windows) {
            self.runtime_dir.join("node").join("npm.cmd")
        } else {
            self.runtime_dir.join("node").join("bin").join("npm")
        };

        let node_bin_dir = self.node_bin().parent().unwrap_or(Path::new("")).to_path_buf();
        let system_path = std::env::var("PATH").unwrap_or_default();
        let full_path = format!("{}:{}", node_bin_dir.display(), system_path);

        let status = Command::new(&npm)
            .args(["install", "--prefix", &self.runtime_dir.to_string_lossy(), "playwright-core"])
            .env("PATH", &full_path)
            .status()
            .await
            .map_err(|e| RuntimeError::PackageInstallFailed(e.to_string()))?;

        if !status.success() {
            return Err(RuntimeError::PackageInstallFailed(
                "npm install playwright-core 失败".to_string(),
            ));
        }

        Ok(())
    }

    /// 安装 Chromium
    async fn install_chromium(&self) -> Result<(), RuntimeError> {
        log::info!("[Runtime] 安装 Chromium...");

        let npx = self.npx_bin();
        let browsers_path = self.playwright_browsers();
        std::fs::create_dir_all(&browsers_path)
            .map_err(|e| RuntimeError::IoError(e.to_string()))?;

        let node_bin_dir = self.node_bin().parent().unwrap_or(Path::new("")).to_path_buf();
        let system_path = std::env::var("PATH").unwrap_or_default();
        let full_path = format!("{}:{}", node_bin_dir.display(), system_path);

        let status = Command::new(&npx)
            .args(["playwright-core", "install", "chromium"])
            .env("PLAYWRIGHT_BROWSERS_PATH", &browsers_path)
            .env("PATH", &full_path)
            .status()
            .await
            .map_err(|e| RuntimeError::ChromiumInstallFailed(e.to_string()))?;

        if !status.success() {
            return Err(RuntimeError::ChromiumInstallFailed(
                "playwright install chromium 失败".to_string(),
            ));
        }

        Ok(())
    }

    /// 复制 bridge 脚本到 scripts/
    fn copy_scripts(&self) -> Result<(), RuntimeError> {
        let scripts_dir = self.scripts_dir();
        std::fs::create_dir_all(&scripts_dir)
            .map_err(|e| RuntimeError::IoError(e.to_string()))?;

        // bridge 脚本内嵌到二进制中 (通过 include_str!)
        let bridge_js = include_str!("../../scripts/browser-bridge.js");
        std::fs::write(scripts_dir.join("browser-bridge.js"), bridge_js)
            .map_err(|e| RuntimeError::IoError(e.to_string()))?;

        log::info!("[Runtime] 脚本已复制");
        Ok(())
    }

    /// 清理运行时
    pub async fn cleanup(&self) -> Result<(), RuntimeError> {
        if self.runtime_dir.exists() {
            std::fs::remove_dir_all(&self.runtime_dir)
                .map_err(|e| RuntimeError::IoError(e.to_string()))?;
            log::info!("[Runtime] 运行时已清理");
        }
        Ok(())
    }
}

/// 根据平台生成 Node.js 下载 URL
fn node_download_url() -> Result<(String, &'static str), RuntimeError> {
    let (os, arch) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => ("darwin", "arm64"),
        ("macos", "x86_64") => ("darwin", "x64"),
        ("windows", "x86_64") => ("win", "x64"),
        ("linux", "x86_64") => ("linux", "x64"),
        ("linux", "aarch64") => ("linux", "arm64"),
        (os, arch) => {
            return Err(RuntimeError::UnsupportedPlatform(
                format!("{os}-{arch}"),
            ));
        }
    };

    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };

    // npmmirror 加速中国下载
    let url = format!(
        "https://npmmirror.com/mirrors/node/v{NODE_VERSION}/node-v{NODE_VERSION}-{os}-{arch}.{ext}"
    );

    Ok((url, ext))
}
