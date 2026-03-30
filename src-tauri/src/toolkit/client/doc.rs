// toolkit/client/doc — 文档 API
//
// 移植自: aily_client/api/docs/ + url.py (90行合并)

use serde_json::{json, Value};

use super::{Result, TabClient};

/// 列出文档块
pub async fn list_doc_blocks(
    client: &TabClient,
    doc_id: &str,
    page_size: i32,
    page_token: Option<&str>,
) -> Result<Value> {
    let mut params = vec![("page_size", page_size.to_string())];
    if let Some(pt) = page_token {
        params.push(("page_token", pt.to_string()));
    }

    let str_params: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();

    client
        .get(
            &format!("/docx/v1/documents/{doc_id}/blocks"),
            &str_params,
        )
        .await
}

/// 搜索文档
pub async fn search_docs(
    client: &TabClient,
    query: &str,
    owner_ids: Option<&[String]>,
    docs_types: Option<&[String]>,
) -> Result<Value> {
    let mut body = json!({
        "search_key": query,
        "count": 50,
    });
    if let Some(owners) = owner_ids {
        body["owner_ids"] = json!(owners);
    }
    if let Some(types) = docs_types {
        body["docs_types"] = json!(types);
    }
    client.post("/suite/docs-api/search/object", &body).await
}

/// 批量获取文档信息
pub async fn batch_get_doc_info(
    client: &TabClient,
    urls: &[String],
    with_stats: bool,
) -> Result<Value> {
    let mut body = json!({ "urls": urls });
    if with_stats {
        body["with_url"] = json!(true);
    }
    client
        .post("/drive/v1/metas/batch_query", &body)
        .await
}

/// 获取文档评论
pub async fn get_comments(
    client: &TabClient,
    file_token: &str,
    file_type: &str,
) -> Result<Value> {
    client
        .get(
            &format!("/drive/v1/files/{file_token}/comments"),
            &[("file_type", file_type)],
        )
        .await
}

/// 添加评论
pub async fn add_comment(
    client: &TabClient,
    file_token: &str,
    file_type: &str,
    content: &str,
) -> Result<Value> {
    let body = json!({
        "file_type": file_type,
        "content": content,
    });
    client
        .post(
            &format!("/drive/v1/files/{file_token}/comments"),
            &body,
        )
        .await
}

/// 解析文档 URL
pub async fn parse_doc_urls(
    client: &TabClient,
    urls: &[String],
) -> Result<Value> {
    let body = json!({ "urls": urls });
    client
        .post("/drive/v1/metas/batch_query", &body)
        .await
}
