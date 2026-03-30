// toolkit/drive/ops — 云空间上传/下载
//
// 移植自: aily_drive/commands/upload.py + download.py
// 独立实现，不复用 base::file_ops

use std::path::Path;
use serde_json::{json, Value};
use crate::toolkit::client::{self, Result, TabClient, TabClientError};

/// 格式化文件大小
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// 从 URL 解析 mount 信息
fn parse_mount_info(url: &str) -> (String, String) {
    let parts: Vec<&str> = url.trim_matches('/').split('/').collect();
    match parts.len() {
        n if n >= 2 => (parts[n - 2].to_string(), parts[n - 1].to_string()),
        1 => (String::new(), parts[0].to_string()),
        _ => (String::new(), String::new()),
    }
}

/// 上传单文件
pub async fn upload_file(
    client: &TabClient,
    file_path: &str,
    parent_type: &str,
    parent_node: &str,
) -> Result<Value> {
    let path = Path::new(file_path);
    let file_name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unnamed".to_string());

    let file_content = tokio::fs::read(path).await
        .map_err(|e| TabClientError::Other(format!("读取文件失败: {e}")))?;

    let file_size = file_content.len() as u64;

    // 调用 client API 上传
    let raw = client::drive::upload_file(client, &file_name, file_content, parent_type, parent_node).await?;

    // 规范化结果
    let mut result = json!({
        "file_name": file_name,
        "file_path": file_path,
        "file_size": file_size,
        "size_display": format_size(file_size),
        "parent_type": parent_type,
        "parent_node": parent_node,
        "status": "ok",
    });

    // 合并 API 返回
    if let Some(obj) = raw.as_object() {
        for (k, v) in obj {
            result[k] = v.clone();
        }
    }

    Ok(result)
}

/// 批量上传
pub async fn upload_files(
    client: &TabClient,
    file_paths: &[String],
    parent_type: &str,
    parent_node: &str,
) -> Result<Value> {
    let mut results = Vec::new();
    let mut success = 0;
    let mut failed = 0;

    for path in file_paths {
        match upload_file(client, path, parent_type, parent_node).await {
            Ok(r) => {
                results.push(r);
                success += 1;
            }
            Err(e) => {
                results.push(json!({
                    "file_path": path,
                    "status": "failed",
                    "error": format!("{e}"),
                }));
                failed += 1;
            }
        }
    }

    Ok(json!({
        "items": results,
        "total": file_paths.len(),
        "success_count": success,
        "failed_count": failed,
    }))
}

/// 批量下载
pub async fn download_files(
    client: &TabClient,
    file_tokens: &[String],
    output_dir: &str,
) -> Result<Value> {
    tokio::fs::create_dir_all(output_dir).await
        .map_err(|e| TabClientError::Other(format!("创建目录失败: {e}")))?;

    let mut results = Vec::new();
    let mut success = 0;
    let mut failed = 0;

    for token in file_tokens {
        match client::drive::download_file(client, token).await {
            Ok(bytes) => {
                let filename = format!("{token}");
                let out_path = Path::new(output_dir).join(&filename);
                match tokio::fs::write(&out_path, &bytes).await {
                    Ok(_) => {
                        results.push(json!({
                            "token": token,
                            "file": out_path.to_string_lossy(),
                            "size": bytes.len(),
                            "size_display": format_size(bytes.len() as u64),
                            "status": "ok",
                        }));
                        success += 1;
                    }
                    Err(e) => {
                        results.push(json!({
                            "token": token,
                            "status": "failed",
                            "error": format!("写入失败: {e}"),
                        }));
                        failed += 1;
                    }
                }
            }
            Err(e) => {
                results.push(json!({
                    "token": token,
                    "status": "failed",
                    "error": format!("{e}"),
                }));
                failed += 1;
            }
        }
    }

    Ok(json!({
        "items": results,
        "total": file_tokens.len(),
        "success_count": success,
        "failed_count": failed,
        "output_dir": output_dir,
    }))
}
