// toolkit/base/delete — 按 key 删行/删字段
//
// 移植自: aily_base/commands/delete.py (204行)

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::toolkit::client::{self, Result, TabClient};

/// 删除匹配指定 key 字段值的行
pub async fn delete_rows(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
    key_field: &str,
    values: &[String],
) -> Result<Value> {
    // 构建值集合
    let values_set: HashSet<String> = values.iter().map(|v| v.to_string()).collect();

    // 分页获取所有记录
    let mut all_records: Vec<Value> = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let page = client::bitable::search_records(
            client,
            app_token,
            table_id,
            100,
            page_token.as_deref(),
        )
        .await?;

        all_records.extend(page.items);

        if !page.has_more {
            break;
        }
        page_token = page.page_token;
        if page_token.is_none() {
            break;
        }
    }

    // 匹配要删除的记录
    let mut to_delete: Vec<String> = Vec::new();
    let mut matched_keys: Vec<String> = Vec::new();

    for record in &all_records {
        let record_id = record
            .get("record_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let record_fields = record.get("fields").and_then(|v| v.as_object());

        if let Some(fields) = record_fields {
            if let Some(key_value) = fields.get(key_field) {
                let key_str = match key_value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => key_value.to_string(),
                };

                if values_set.contains(&key_str) {
                    to_delete.push(record_id.to_string());
                    matched_keys.push(key_str);
                } else if let Some(arr) = key_value.as_array() {
                    // 多值字段
                    for v in arr {
                        let vs = if let Some(obj) = v.as_object() {
                            obj.get("text")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string()
                        } else {
                            v.to_string()
                        };
                        if values_set.contains(&vs) {
                            to_delete.push(record_id.to_string());
                            matched_keys.push(vs);
                            break;
                        }
                    }
                }
            }
        }
    }

    // 批量删除
    if !to_delete.is_empty() {
        client::bitable::delete_records(client, app_token, table_id, &to_delete).await?;
    }

    Ok(json!({
        "app_token": app_token,
        "table_id": table_id,
        "deleted_count": to_delete.len(),
        "deleted_keys": matched_keys,
        "skipped_count": values_set.len() - matched_keys.len(),
        "total_records": all_records.len(),
        "status": "ok",
    }))
}

/// 删除字段
pub async fn delete_field(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
    field_name: &str,
) -> Result<Value> {
    // 获取现有字段
    let fields_data = client::bitable::list_fields(client, app_token, table_id).await?;

    let mut field_id: Option<String> = None;
    let mut is_primary = false;

    for f in &fields_data.items {
        let fname = f.get("field_name").and_then(|v| v.as_str()).unwrap_or("");
        if fname == field_name {
            field_id = f.get("field_id").and_then(|v| v.as_str()).map(String::from);
            is_primary = f
                .get("is_primary")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            break;
        }
    }

    let fid = match field_id {
        Some(id) => id,
        None => {
            return Ok(json!({
                "error": format!("Field not found: {field_name}"),
                "status": "failed",
            }))
        }
    };

    if is_primary {
        return Ok(json!({
            "error": format!("Cannot delete primary field: {field_name}"),
            "status": "failed",
        }));
    }

    client::bitable::delete_field(client, app_token, table_id, &fid).await?;

    Ok(json!({
        "app_token": app_token,
        "table_id": table_id,
        "field_name": field_name,
        "field_id": fid,
        "status": "ok",
    }))
}
