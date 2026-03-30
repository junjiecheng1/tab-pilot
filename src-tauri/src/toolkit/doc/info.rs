// toolkit/doc/info — 文档信息 + 列出 + 搜索
//
// 移植自: aily_doc/commands/info.py + list.py (112行)

use serde_json::{json, Value};
use crate::toolkit::client::{self, Result, TabClient};

/// 获取文档信息
pub async fn get_doc_info(
    client: &TabClient,
    doc_id: &str,
) -> Result<Value> {
    client::doc::list_doc_blocks(client, doc_id, 50, None).await
}

/// 搜索文档
pub async fn search_docs(
    client: &TabClient,
    query: &str,
    owner_ids: Option<&[String]>,
) -> Result<Value> {
    client::doc::search_docs(client, query, owner_ids, None).await
}

/// 批量获取文档信息
pub async fn batch_get_info(
    client: &TabClient,
    urls: &[String],
) -> Result<Value> {
    client::doc::batch_get_doc_info(client, urls, true).await
}

/// 解析文档 URL
pub async fn parse_urls(
    client: &TabClient,
    urls: &[String],
) -> Result<Value> {
    client::doc::parse_doc_urls(client, urls).await
}

/// 列出文档内容块
///
/// 移植自: aily_doc/commands/list.py
pub async fn list_doc(
    client: &TabClient,
    doc_id: &str,
    page_size: i32,
) -> Result<Value> {
    let raw = client::doc::list_doc_blocks(client, doc_id, page_size, None).await?;

    let items = raw.get("items").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
    Ok(json!({
        "items": raw.get("items").unwrap_or(&json!([])),
        "count": items,
        "message": format!("Found {items} blocks"),
    }))
}

