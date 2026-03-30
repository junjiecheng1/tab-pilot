// toolkit/base/file_ops — 文件上传/下载
//
// 移植自: aily_base/commands/file_upload.py + file_download.py (181行)

use std::path::Path;

use serde_json::{json, Value};

use crate::toolkit::client::{self, Result, TabClient};

/// 格式化文件大小
fn format_size(size: u64) -> String {
    if size < 1024 {
        format!("{size}B")
    } else if size < 1024 * 1024 {
        format!("{:.1}KB", size as f64 / 1024.0)
    } else {
        format!("{:.1}MB", size as f64 / (1024.0 * 1024.0))
    }
}

/// 上传多个文件
pub async fn upload_files(
    client: &TabClient,
    file_paths: &[String],
    parent_type: &str,
    parent_node: &str,
) -> Result<Value> {
    let mut results: Vec<Value> = Vec::new();
    let mut uploaded = 0;
    let mut failed = 0;

    for file_path in file_paths {
        let path = Path::new(file_path);
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());

        match tokio::fs::read(path).await {
            Ok(content) => {
                let size = content.len() as u64;
                match client::drive::upload_file(
                    client,
                    &file_name,
                    content,
                    parent_type,
                    parent_node,
                )
                .await
                {
                    Ok(result) => {
                        let token = result
                            .get("file_token")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        results.push(json!({
                            "file_name": file_name,
                            "size": format_size(size),
                            "token": token,
                            "status": "ok",
                        }));
                        uploaded += 1;
                    }
                    Err(e) => {
                        results.push(json!({
                            "file_name": file_name,
                            "error": e.to_string(),
                            "status": "failed",
                        }));
                        failed += 1;
                    }
                }
            }
            Err(e) => {
                results.push(json!({
                    "file_name": file_name,
                    "error": e.to_string(),
                    "status": "failed",
                }));
                failed += 1;
            }
        }
    }

    Ok(json!({
        "uploaded": uploaded,
        "failed": failed,
        "results": results,
    }))
}

/// 下载多个文件
pub async fn download_files(
    client: &TabClient,
    file_tokens: &[String],
    dir_path: &str,
) -> Result<Value> {
    tokio::fs::create_dir_all(dir_path)
        .await
        .map_err(|e| client::TabClientError::Other(e.to_string()))?;

    let mut items: Vec<Value> = Vec::new();
    let mut downloaded = 0;
    let mut failed = 0;

    for token in file_tokens {
        match client::drive::download_file(client, token).await {
            Ok(bytes) => {
                let file_name = format!("{token}.bin");
                let path = Path::new(dir_path).join(&file_name);

                match tokio::fs::write(&path, &bytes).await {
                    Ok(_) => {
                        items.push(json!({
                            "file_name": file_name,
                            "path": path.to_string_lossy(),
                            "size": bytes.len(),
                        }));
                        downloaded += 1;
                    }
                    Err(e) => {
                        items.push(json!({
                            "token": token,
                            "error": e.to_string(),
                        }));
                        failed += 1;
                    }
                }
            }
            Err(e) => {
                items.push(json!({
                    "token": token,
                    "error": e.to_string(),
                }));
                failed += 1;
            }
        }
    }

    Ok(json!({
        "downloaded": downloaded,
        "failed": failed,
        "items": items,
    }))
}
