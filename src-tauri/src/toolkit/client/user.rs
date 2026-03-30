// toolkit/client/user — 用户 API
//
// 移植自: aily_client/api/user/user.py (80行)

use serde_json::{json, Value};

use super::{Result, TabClient};

/// 批量获取员工信息
pub async fn batch_get_users(
    client: &TabClient,
    user_ids: &[String],
    user_id_type: &str,
) -> Result<Value> {
    let mut params: Vec<(&str, String)> = vec![
        ("user_id_type", user_id_type.to_string()),
    ];
    for id in user_ids {
        params.push(("user_ids", id.clone()));
    }

    let str_params: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();

    client.get("/contact/v3/users/batch", &str_params).await
}

/// 搜索员工（自动分页）
pub async fn search_users(
    client: &TabClient,
    query: &str,
    page_size: i32,
) -> Result<Vec<Value>> {
    let mut all_users = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let mut params = vec![
            ("query", query.to_string()),
            ("page_size", page_size.to_string()),
        ];
        if let Some(ref pt) = page_token {
            params.push(("page_token", pt.clone()));
        }

        let str_params: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect();

        let resp: Value = client
            .get("/contact/v3/users/find_by_department", &str_params)
            .await?;

        if let Some(items) = resp.get("items").and_then(|v| v.as_array()) {
            all_users.extend(items.clone());
        }

        let has_more = resp.get("has_more").and_then(|v| v.as_bool()).unwrap_or(false);
        if !has_more {
            break;
        }
        page_token = resp
            .get("page_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if page_token.is_none() {
            break;
        }
    }

    Ok(all_users)
}

/// 获取下属信息（自动分页）
pub async fn get_subordinates(
    client: &TabClient,
    user_id: &str,
    page_size: i32,
) -> Result<Vec<Value>> {
    let mut all_subordinates = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let mut params = vec![
            ("user_id", user_id.to_string()),
            ("page_size", page_size.to_string()),
        ];
        if let Some(ref pt) = page_token {
            params.push(("page_token", pt.clone()));
        }

        let str_params: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect();

        let resp: Value = client
            .get("/contact/v3/users/find_by_department", &str_params)
            .await?;

        if let Some(items) = resp.get("items").and_then(|v| v.as_array()) {
            all_subordinates.extend(items.clone());
        }

        let has_more = resp.get("has_more").and_then(|v| v.as_bool()).unwrap_or(false);
        if !has_more {
            break;
        }
        page_token = resp
            .get("page_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if page_token.is_none() {
            break;
        }
    }

    Ok(all_subordinates)
}
