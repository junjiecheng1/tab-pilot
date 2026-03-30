use std::path::Path;
use tokio::process::Command;

/// 下载单个二进制文件
pub async fn download_binary(url: &str, dest: &Path) -> Result<(), String> {
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
pub async fn download_and_extract(url: &str, dest_dir: &Path) -> Result<(), String> {
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
                } else if !path.to_string_lossy().ends_with(".gz") {
                     // For TarGzDirect root binaries like lark-cli
                     let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
                }
            }
        }
    }

    Ok(())
}
