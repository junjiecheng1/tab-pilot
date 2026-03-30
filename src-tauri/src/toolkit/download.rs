// toolkit/download — URL 下载辅助
//
// 供 tab-pdf、tab-xlsx 等 toolkit 命令复用的 URL → 本地文件下载逻辑

use std::path::{Path, PathBuf};
use crate::core::error::ServiceError;

/// 下载后的本地文件 (可能是临时的)
pub enum ResolvedInput {
    /// 本地文件, 不需要清理
    Local(PathBuf),
    /// 从 URL 下载的临时文件, 用完需删除
    Downloaded(PathBuf),
}

impl ResolvedInput {
    pub fn path(&self) -> &Path {
        match self {
            Self::Local(p) | Self::Downloaded(p) => p,
        }
    }

    pub fn cleanup(self) {
        if let Self::Downloaded(p) = self {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// 判断输入是 URL 还是本地路径, URL 则下载到 /tmp/
pub async fn resolve_input(input: &str, ext: &str) -> Result<ResolvedInput, ServiceError> {
    if input.starts_with("http://") || input.starts_with("https://") {
        let tmp_path = download_file(input, ext).await?;
        Ok(ResolvedInput::Downloaded(tmp_path))
    } else {
        let path = PathBuf::from(input);
        if !path.exists() {
            return Err(ServiceError::not_found(format!("文件不存在: {input}")));
        }
        Ok(ResolvedInput::Local(path))
    }
}

/// 下载文件到临时路径
async fn download_file(url: &str, ext: &str) -> Result<PathBuf, ServiceError> {
    let resp = reqwest::get(url)
        .await
        .map_err(|e| ServiceError::internal(format!("下载失败: {e}")))?;

    if !resp.status().is_success() {
        return Err(ServiceError::internal(format!(
            "下载失败: HTTP {}", resp.status()
        )));
    }

    let bytes = resp.bytes()
        .await
        .map_err(|e| ServiceError::internal(format!("读取失败: {e}")))?;

    // 50MB 上限
    if bytes.len() > 50 * 1024 * 1024 {
        return Err(ServiceError::bad_request(format!(
            "文件过大: {:.1}MB (上限 50MB)",
            bytes.len() as f64 / 1024.0 / 1024.0
        )));
    }

    let tmp_path = std::env::temp_dir().join(format!(
        "tab_dl_{}.{}",
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("tmp"),
        ext
    ));

    std::fs::write(&tmp_path, &bytes)
        .map_err(|e| ServiceError::internal(format!("临时文件写入失败: {e}")))?;

    Ok(tmp_path)
}
