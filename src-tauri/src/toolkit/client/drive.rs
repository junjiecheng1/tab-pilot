// toolkit/client/drive — 文件存储 API
//
// 移植自: aily_client/api/drive.py (26行)

use serde_json::Value;

use super::{Result, TabClient};

/// 上传文件
pub async fn upload_file(
    client: &TabClient,
    file_name: &str,
    file_content: Vec<u8>,
    parent_type: &str,
    parent_node: &str,
) -> Result<Value> {
    let part = reqwest::multipart::Part::bytes(file_content)
        .file_name(file_name.to_string())
        .mime_str("application/octet-stream")
        .map_err(|e| super::TabClientError::Other(e.to_string()))?;

    let form = reqwest::multipart::Form::new()
        .text("file_name", file_name.to_string())
        .text("parent_type", parent_type.to_string())
        .text("parent_node", parent_node.to_string())
        .part("file", part);

    client
        .post_multipart("/drive/v1/files/upload_all", form)
        .await
}

/// 下载文件
pub async fn download_file(
    client: &TabClient,
    file_token: &str,
) -> Result<Vec<u8>> {
    client
        .get_bytes(&format!("/drive/v1/medias/{file_token}/download"))
        .await
}
