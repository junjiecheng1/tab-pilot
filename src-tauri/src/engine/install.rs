use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{exit, Command, Stdio};

/// npmmirror Chrome for Testing 镜像 — 国内可直接访问
const NPMMIRROR_CFT_BASE: &str = "https://registry.npmmirror.com/-/binary/chrome-for-testing";

/// 官方版本信息 URL (国内可能无法访问, 作为 fallback)
const OFFICIAL_LAST_KNOWN_GOOD_URL: &str =
    "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";

pub fn get_browsers_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agent-browser")
        .join("browsers")
}

pub fn find_installed_chrome() -> Option<PathBuf> {
    let browsers_dir = get_browsers_dir();
    if !browsers_dir.exists() {
        return None;
    }

    let mut versions: Vec<_> = fs::read_dir(&browsers_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("chrome-"))
        })
        .collect();

    versions.sort_by_key(|b| std::cmp::Reverse(b.file_name()));

    for entry in versions {
        if let Some(bin) = chrome_binary_in_dir(&entry.path()) {
            if bin.exists() {
                return Some(bin);
            }
        }
    }

    None
}

fn chrome_binary_in_dir(dir: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let app =
            dir.join("Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing");
        if app.exists() {
            return Some(app);
        }
        let inner = dir.join("chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing");
        if inner.exists() {
            return Some(inner);
        }
        let inner_x64 = dir.join(
            "chrome-mac-x64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
        );
        if inner_x64.exists() {
            return Some(inner_x64);
        }
        None
    }

    #[cfg(target_os = "linux")]
    {
        let bin = dir.join("chrome");
        if bin.exists() {
            return Some(bin);
        }
        let inner = dir.join("chrome-linux64/chrome");
        if inner.exists() {
            return Some(inner);
        }
        None
    }

    #[cfg(target_os = "windows")]
    {
        let bin = dir.join("chrome.exe");
        if bin.exists() {
            return Some(bin);
        }
        let inner = dir.join("chrome-win64/chrome.exe");
        if inner.exists() {
            return Some(inner);
        }
        None
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

fn platform_key() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "mac-arm64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "mac-x64"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux64"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "win64"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        panic!("Unsupported platform for Chrome for Testing download")
    }
}

/// 获取 Chrome 下载信息 — 双源策略:
/// 1. 优先从 npmmirror 版本目录获取最新版本 (国内可用)
/// 2. 失败则 fallback 到官方 JSON
async fn fetch_download_url() -> Result<(String, String), String> {
    // 策略 1: npmmirror 版本目录 (国内优先)
    match fetch_from_npmmirror().await {
        Ok(result) => return Ok(result),
        Err(e) => log::warn!("[install] npmmirror 获取失败: {}, 尝试官方源...", e),
    }

    // 策略 2: 官方 JSON (fallback)
    fetch_from_official().await
}

/// 从 npmmirror 版本目录获取最新 Chrome 版本
async fn fetch_from_npmmirror() -> Result<(String, String), String> {
    log::info!("[install] 从 npmmirror 获取 Chrome 版本...");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client 创建失败: {}", e))?;

    // 获取版本目录列表
    let resp = client
        .get(format!("{}/", NPMMIRROR_CFT_BASE))
        .send()
        .await
        .map_err(|e| format!("npmmirror 请求失败: {}", e))?;

    let entries: Vec<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| format!("npmmirror JSON 解析失败: {}", e))?;

    // 从版本目录名中提取版本号, 找到最新的稳定版
    // 版本目录格式: "131.0.6778.264/"
    let platform = platform_key();
    let mut best_version: Option<String> = None;

    for entry in &entries {
        if let Some(name) = entry.get("name").and_then(|n| n.as_str()) {
            let ver = name.trim_end_matches('/');
            // 只选主版本号 >= 120 的稳定版 (有多段 . 分隔)
            let parts: Vec<&str> = ver.split('.').collect();
            if parts.len() == 4 {
                if let Ok(major) = parts[0].parse::<u32>() {
                    if major >= 120 {
                        // 比较版本号 — 选最大的
                        if best_version.as_ref().map_or(true, |cur| {
                            compare_versions(ver, cur) == std::cmp::Ordering::Greater
                        }) {
                            best_version = Some(ver.to_string());
                        }
                    }
                }
            }
        }
    }

    let version = best_version.ok_or("npmmirror 未找到可用的 Chrome 版本")?;

    // 构造下载 URL: npmmirror/{version}/{platform}/chrome-{platform}.zip
    let url = format!(
        "{}/{}/{}/chrome-{}.zip",
        NPMMIRROR_CFT_BASE, version, platform, platform
    );

    log::info!("[install] npmmirror 找到版本: {}", version);
    Ok((version, url))
}

/// 比较两个版本号 (a.b.c.d 格式)
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u32> { s.split('.').filter_map(|p| p.parse().ok()).collect() };
    parse(a).cmp(&parse(b))
}

/// 从官方 JSON 获取 Chrome 下载信息
async fn fetch_from_official() -> Result<(String, String), String> {
    log::info!("[install] 从官方源获取 Chrome 版本信息...");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client 创建失败: {}", e))?;

    let resp = client
        .get(OFFICIAL_LAST_KNOWN_GOOD_URL)
        .send()
        .await
        .map_err(|e| format!("官方源请求失败: {}", e))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("官方 JSON 解析失败: {}", e))?;

    let channel = body
        .get("channels")
        .and_then(|c| c.get("Stable"))
        .ok_or("未找到 Stable channel")?;

    let version = channel
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or("未找到版本号")?
        .to_string();

    let platform = platform_key();

    let url = channel
        .get("downloads")
        .and_then(|d| d.get("chrome"))
        .and_then(|c| c.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|entry| {
                if entry.get("platform")?.as_str()? == platform {
                    Some(entry.get("url")?.as_str()?.to_string())
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| format!("未找到平台 {} 的下载链接", platform))?;

    Ok((version, url))
}

async fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    let total = resp.content_length();
    let mut bytes = Vec::new();
    let mut stream = resp;
    let mut downloaded: u64 = 0;
    let mut last_pct: u64 = 0;

    loop {
        let chunk = stream
            .chunk()
            .await
            .map_err(|e| format!("Download error: {}", e))?;
        match chunk {
            Some(data) => {
                downloaded += data.len() as u64;
                bytes.extend_from_slice(&data);

                if let Some(total) = total {
                    let pct = (downloaded * 100) / total;
                    if pct >= last_pct + 5 {
                        last_pct = pct;
                        let mb = downloaded as f64 / 1_048_576.0;
                        let total_mb = total as f64 / 1_048_576.0;
                        eprint!("\r  {:.0}/{:.0} MB ({pct}%)", mb, total_mb);
                        let _ = io::stderr().flush();
                    }
                }
            }
            None => break,
        }
    }

    eprintln!();
    Ok(bytes)
}

fn extract_zip(bytes: Vec<u8>, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("Failed to create directory: {}", e))?;

    let cursor = io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Failed to read zip archive: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read zip entry: {}", e))?;

        let raw_name = file.name().to_string();
        // 跳过 macOS 元数据
        if raw_name.contains("__MACOSX") || raw_name.contains(".DS_Store") {
            continue;
        }
        let rel_path: String = raw_name
            .strip_prefix("chrome-")
            .and_then(|s: &str| s.split_once('/'))
            .map(|(_, rest): (&str, &str)| rest.to_string())
            .unwrap_or(raw_name.clone());

        if rel_path.is_empty() {
            continue;
        }

        let out_path = dest.join(&rel_path);

        // Defense-in-depth: ensure the resolved path is inside dest
        if !out_path.starts_with(dest) {
            continue;
        }

        if file.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|e| format!("Failed to create dir {}: {}", out_path.display(), e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!("Failed to create parent dir {}: {}", parent.display(), e)
                })?;
            }
            let mut out_file = fs::File::create(&out_path)
                .map_err(|e| format!("Failed to create file {}: {}", out_path.display(), e))?;
            io::copy(&mut file, &mut out_file)
                .map_err(|e| format!("Failed to write {}: {}", out_path.display(), e))?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    let _ = fs::set_permissions(&out_path, fs::Permissions::from_mode(mode));
                }
            }
        }
    }

    Ok(())
}

pub fn run_install(with_deps: bool) {
    if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        log::error!("Chrome for Testing does not provide Linux ARM64 builds.");
        log::error!("Install Chromium from your system package manager instead.");
        exit(1);
    }

    let is_linux = cfg!(target_os = "linux");

    if is_linux {
        if with_deps {
            install_linux_deps();
        } else {
            log::warn!("Linux detected. If browser fails to launch, run install --with-deps");
        }
    }

    log::info!("Installing Chrome...");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|e| {
            log::error!("Failed to create runtime: {}", e);
            exit(1);
        });

    let (version, url) = match rt.block_on(fetch_download_url()) {
        Ok(v) => v,
        Err(e) => {
            log::error!("获取 Chrome 下载信息失败: {}", e);
            exit(1);
        }
    };

    let dest = get_browsers_dir().join(format!("chrome-{}", version));

    if let Some(bin) = chrome_binary_in_dir(&dest) {
        if bin.exists() {
            log::info!("Chrome {} is already installed", version);
            return;
        }
    }

    log::info!("Downloading Chrome {} for {}", version, platform_key());
    log::info!("URL: {}", url);

    let bytes = match rt.block_on(download_bytes(&url)) {
        Ok(b) => b,
        Err(e) => {
            log::error!("Chrome 下载失败: {}", e);
            exit(1);
        }
    };

    match extract_zip(bytes, &dest) {
        Ok(()) => {
            log::info!(
                "Chrome {} installed successfully at {}",
                version,
                dest.display()
            );
            if is_linux && !with_deps {
                log::warn!("If you see shared library errors, run install --with-deps");
            }
        }
        Err(e) => {
            let _ = fs::remove_dir_all(&dest);
            log::error!("Chrome 解压失败: {}", e);
            exit(1);
        }
    }
}

fn report_install_status(status: io::Result<std::process::ExitStatus>) {
    match status {
        Ok(s) if s.success() => log::info!("System dependencies installed"),
        Ok(_) => log::warn!("Failed to install some deps. Run manually with sudo."),
        Err(e) => log::warn!("Could not run install command: {}", e),
    }
}

fn install_linux_deps() {
    log::info!("Installing system dependencies...");

    let (pkg_mgr, deps) = if which_exists("apt-get") {
        // On Ubuntu 24.04+, many libraries were renamed with a t64 suffix as
        // part of the 64-bit time_t transition. Using the old names can cause
        // apt to propose removing hundreds of system packages to resolve
        // conflicts. We check for the t64 variant first to avoid this.
        let apt_deps: Vec<&str> = vec![
            ("libxcb-shm0", None),
            ("libx11-xcb1", None),
            ("libx11-6", None),
            ("libxcb1", None),
            ("libxext6", None),
            ("libxrandr2", None),
            ("libxcomposite1", None),
            ("libxcursor1", None),
            ("libxdamage1", None),
            ("libxfixes3", None),
            ("libxi6", None),
            ("libgtk-3-0", Some("libgtk-3-0t64")),
            ("libpangocairo-1.0-0", Some("libpangocairo-1.0-0t64")),
            ("libpango-1.0-0", Some("libpango-1.0-0t64")),
            ("libatk1.0-0", Some("libatk1.0-0t64")),
            ("libcairo-gobject2", Some("libcairo-gobject2t64")),
            ("libcairo2", Some("libcairo2t64")),
            ("libgdk-pixbuf-2.0-0", Some("libgdk-pixbuf-2.0-0t64")),
            ("libxrender1", None),
            ("libasound2", Some("libasound2t64")),
            ("libfreetype6", None),
            ("libfontconfig1", None),
            ("libdbus-1-3", Some("libdbus-1-3t64")),
            ("libnss3", None),
            ("libnspr4", None),
            ("libatk-bridge2.0-0", Some("libatk-bridge2.0-0t64")),
            ("libdrm2", None),
            ("libxkbcommon0", None),
            ("libatspi2.0-0", Some("libatspi2.0-0t64")),
            ("libcups2", Some("libcups2t64")),
            ("libxshmfence1", None),
            ("libgbm1", None),
        ]
        .into_iter()
        .map(|(base, t64_variant)| {
            if let Some(t64) = t64_variant {
                if package_exists_apt(t64) {
                    return t64;
                }
            }
            base
        })
        .collect();

        ("apt-get", apt_deps)
    } else if which_exists("dnf") {
        (
            "dnf",
            vec![
                "nss",
                "nspr",
                "atk",
                "at-spi2-atk",
                "cups-libs",
                "libdrm",
                "libXcomposite",
                "libXdamage",
                "libXrandr",
                "mesa-libgbm",
                "pango",
                "alsa-lib",
                "libxkbcommon",
                "libxcb",
                "libX11-xcb",
                "libX11",
                "libXext",
                "libXcursor",
                "libXfixes",
                "libXi",
                "gtk3",
                "cairo-gobject",
            ],
        )
    } else if which_exists("yum") {
        (
            "yum",
            vec![
                "nss",
                "nspr",
                "atk",
                "at-spi2-atk",
                "cups-libs",
                "libdrm",
                "libXcomposite",
                "libXdamage",
                "libXrandr",
                "mesa-libgbm",
                "pango",
                "alsa-lib",
                "libxkbcommon",
            ],
        )
    } else {
        log::error!("No supported package manager found (apt-get, dnf, or yum)");
        exit(1);
    };

    if pkg_mgr == "apt-get" {
        // Run apt-get update first
        println!("Running: sudo apt-get update");
        let update_status = Command::new("sudo").args(["apt-get", "update"]).status();

        match update_status {
            Ok(s) if !s.success() => {
                log::warn!("apt-get update failed. Continuing with existing package lists.");
            }
            Err(e) => {
                log::warn!("Could not run apt-get update: {}", e);
            }
            _ => {}
        }

        // Simulate the install first to detect if apt would remove any
        // packages. This prevents the catastrophic scenario where installing
        // these libraries triggers removal of hundreds of system packages
        // due to dependency conflicts (e.g. on Ubuntu 24.04 with the
        // t64 transition).
        println!("Checking for conflicts...");
        let sim_output = Command::new("sudo")
            .args(["apt-get", "install", "--simulate"])
            .args(&deps)
            .output();

        match sim_output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{}\n{}", stdout, stderr);

                // Count packages that would be removed
                let removals: Vec<&str> = combined
                    .lines()
                    .filter(|line| line.starts_with("Remv "))
                    .collect();

                if !removals.is_empty() {
                    log::error!(
                        "Aborting: apt would remove {} package(s) to install these dependencies.",
                        removals.len()
                    );
                    eprintln!(
                        "  This usually means some package names have changed on your system"
                    );
                    eprintln!("  (e.g. Ubuntu 24.04 renamed libraries with a t64 suffix).");
                    eprintln!();
                    eprintln!("  Packages that would be removed:");
                    for line in removals.iter().take(20) {
                        eprintln!("    {}", line);
                    }
                    if removals.len() > 20 {
                        eprintln!("    ... and {} more", removals.len() - 20);
                    }
                    eprintln!();
                    eprintln!("  To install dependencies manually, run:");
                    eprintln!("    sudo apt-get install {}", deps.join(" "));
                    eprintln!();
                    eprintln!("  Review the apt output carefully before confirming.");
                    exit(1);
                }
            }
            Err(e) => {
                log::warn!(
                    "Could not simulate install ({}). Proceeding with caution.",
                    e
                );
            }
        }

        // Safe to proceed: no removals detected
        let install_cmd = format!("sudo apt-get install -y {}", deps.join(" "));
        println!("Running: {}", install_cmd);
        let status = Command::new("sudo")
            .args(["apt-get", "install", "-y"])
            .args(&deps)
            .status();

        report_install_status(status);
    } else {
        // dnf / yum path — these package managers do not remove packages
        // during install, so the simulate-first guard is not needed.
        let install_cmd = format!("sudo {} install -y {}", pkg_mgr, deps.join(" "));
        println!("Running: {}", install_cmd);
        let status = Command::new("sh").arg("-c").arg(&install_cmd).status();

        report_install_status(status);
    }
}

fn which_exists(cmd: &str) -> bool {
    #[cfg(unix)]
    {
        Command::new("which")
            .arg(cmd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        Command::new("where")
            .arg(cmd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

fn package_exists_apt(pkg: &str) -> bool {
    Command::new("apt-cache")
        .arg("show")
        .arg(pkg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
