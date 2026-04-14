use crate::infra::tools::downloader::{download_and_extract, download_binary};
use crate::infra::tools::registry::{platform_key, tool_list, ToolKind};
use std::path::{Path, PathBuf};

/// CLI 工具管理器
pub struct ToolsManager {
    tools_dir: PathBuf,
    oss_url: String,
}

impl ToolsManager {
    pub fn new(_data_dir: &Path, oss_url: &str) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let tools_dir = home.join(".tabpilot").join("runtime").join("tools");
        Self {
            tools_dir,
            oss_url: oss_url.to_string(),
        }
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
        for (name, kind, _) in tool_list() {
            if matches!(kind, ToolKind::Archive) {
                if let Some(entry) = self.find_archive_entry(name) {
                    if let Some(parent) = entry.parent() {
                        dirs.push(parent.to_path_buf());
                    }
                } else {
                    let sub_dir = self.tools_dir.join(name);
                    if sub_dir.exists() {
                        dirs.push(sub_dir);
                    }
                }
            }
        }
        dirs
    }

    /// 所有工具是否就绪 (逐个检查)
    pub fn is_ready(&self) -> bool {
        for (name, kind, _) in tool_list() {
            let exists = match kind {
                ToolKind::Binary | ToolKind::TarGzDirect => {
                    let bin_name = crate::infra::platform::bin_name(name);
                    self.tools_dir.join(&bin_name).exists()
                }
                ToolKind::Archive => {
                    self.find_archive_entry(name).is_some()
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
            let prefix = self.resolve_prefix(name, dynamic_version).await;

            let (entry_bin, url, dest_dir, is_archive) = match kind {
                ToolKind::Binary => {
                    let bin_name = crate::infra::platform::bin_name(name);
                    let bin_path = self.tools_dir.join(&bin_name);
                    let url = format!("{}/{platform}/{bin_name}", prefix);
                    (bin_path.clone(), url, bin_path, false)
                }
                ToolKind::TarGzDirect => {
                    let bin_name = crate::infra::platform::bin_name(name);
                    let bin_path = self.tools_dir.join(&bin_name);
                    let url = format!("{}/{platform}/{name}.tar.gz", prefix);
                    (bin_path, url, self.tools_dir.clone(), true)
                }
                ToolKind::Archive => {
                    let dest_dir = self.tools_dir.clone();
                    let entry_bin = self.archive_entry_candidate(&self.tools_dir.join(name), name);
                    let url = format!("{}/{platform}/{name}.tar.gz", prefix);
                    let _ = std::fs::create_dir_all(&self.tools_dir);
                    (entry_bin, url, dest_dir, true)
                }
            };

            if matches!(kind, ToolKind::Archive) && self.find_archive_entry(name).is_some() {
                log::info!("[Tools] {name} 已存在, 跳过");
                continue;
            }

            if entry_bin.exists() {
                log::info!("[Tools] {name} 已存在, 跳过");
                continue;
            }

            log::info!("[Tools] 下载 {name}: {url}");
            let res = if is_archive {
                download_and_extract(&url, &dest_dir).await
            } else {
                download_binary(&url, &dest_dir).await
            };

            match res {
                Ok(_) => log::info!("[Tools] {name} ✅"),
                Err(e) => log::warn!("[Tools] {name} 下载失败 (非致命): {e}"),
            }
        }

        log::info!("[Tools] 全部工具就绪 ✅");
        Ok(())
    }

    /// 解析工具的 OSS 前缀 (支持版本探测)
    async fn resolve_prefix(&self, name: &str, dynamic_version: bool) -> String {
        if !dynamic_version {
            return self.oss_url.clone();
        }

        if let Ok(platform_key) = platform_key() {
            let version_url = format!(
                "https://crafto.oss-cn-beijing.aliyuncs.com/tools/tabpilot-tools/{platform}/{name}-version.txt",
                platform=platform_key, name=name
            );

            log::info!("[Tools] 探测 {name} 动态版本: {version_url}");
            match reqwest::get(&version_url).await {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(v) = resp.text().await {
                        let v = v.trim();
                        if !v.is_empty() {
                            log::info!("[Tools] 解析 {name} 动态版本: {v}");
                            return format!("{}/history/{}", self.oss_url, v);
                        }
                    }
                }
                _ => log::warn!("[Tools] 无法获取 {name} 的动态版本号，回退到默认路径"),
            }
        }

        self.oss_url.clone()
    }

    fn archive_entry_candidate(&self, dest_dir: &Path, name: &str) -> PathBuf {
        dest_dir.join(crate::infra::platform::bin_name(name))
    }

    fn find_archive_entry(&self, name: &str) -> Option<PathBuf> {
        let tool_dir = self.tools_dir.join(name);
        self.find_archive_entry_in_dir(&tool_dir, name, 4)
    }

    fn find_archive_entry_in_dir(&self, dir: &Path, name: &str, remaining_depth: usize) -> Option<PathBuf> {
        let direct = self.archive_entry_candidate(dir, name);
        if direct.exists() {
            return Some(direct);
        }

        if remaining_depth == 0 {
            return None;
        }

        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if let Some(found) = self.find_archive_entry_in_dir(&path, name, remaining_depth - 1) {
                return Some(found);
            }
        }

        None
    }
}
