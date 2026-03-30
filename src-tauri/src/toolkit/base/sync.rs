// toolkit/base/sync — 数据同步（upsert）
//
// 移植自: aily_base/commands/sync.py (130行)

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::toolkit::client::{self, Result, TabClient};

/// 同步数据到多维表格（存在则更新，不存在则插入）
pub async fn sync_table(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
    rows: &[Value],
    key_field: &str,
    create_missing_fields: bool,
) -> Result<Value> {
    // 获取字段定义
    let fields_data = client::bitable::list_fields(client, app_token, table_id).await?;
    let mut field_type_map: HashMap<String, i64> = HashMap::new();

    for field_def in &fields_data.items {
        let name = field_def.get("field_name").and_then(|v| v.as_str()).unwrap_or("");
        let ftype = field_def.get("type").and_then(|v| v.as_i64()).unwrap_or(0);
        field_type_map.insert(name.to_string(), ftype);
    }

    // 创建缺失字段
    let mut fields_added = 0;
    if create_missing_fields {
        for row in rows {
            if let Some(obj) = row.as_object() {
                for k in obj.keys() {
                    if !field_type_map.contains_key(k) {
                        match client::bitable::add_field(
                            client, app_token, table_id, k, 1, None, None,
                        )
                        .await
                        {
                            Ok(_) => {
                                field_type_map.insert(k.clone(), 1);
                                fields_added += 1;
                            }
                            Err(_) => {}
                        }
                    }
                }
            }
        }
    }

    // 获取现有记录，建立 key → record_id 映射
    let mut key_to_record_id: HashMap<String, String> = HashMap::new();
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

        for r in &page.items {
            if let Some(fields) = r.get("fields").and_then(|v| v.as_object()) {
                if let Some(key_val) = fields.get(key_field) {
                    let key_str = match key_val {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    let record_id = r
                        .get("record_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    key_to_record_id.insert(key_str, record_id);
                }
            }
        }

        if !page.has_more {
            break;
        }
        page_token = page.page_token;
        if page_token.is_none() {
            break;
        }
    }

    // 分类: 更新 vs 插入
    let mut to_update: Vec<Value> = Vec::new();
    let mut to_insert: Vec<Value> = Vec::new();
    let mut skipped = 0;

    for row in rows {
        if let Some(obj) = row.as_object() {
            let key_value = obj.get(key_field);
            let key_str = match key_value {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => {
                    skipped += 1;
                    continue;
                }
            };

            // 过滤 null 值
            let clean: serde_json::Map<String, Value> = obj
                .iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            if let Some(record_id) = key_to_record_id.get(&key_str) {
                to_update.push(json!({
                    "record_id": record_id,
                    "fields": clean,
                }));
            } else {
                to_insert.push(json!({"fields": clean}));
            }
        }
    }

    // 批量操作
    let mut inserted_count = 0;
    let mut updated_count = 0;

    for chunk in to_insert.chunks(100) {
        match client::bitable::add_records(client, app_token, table_id, chunk).await {
            Ok(_) => inserted_count += chunk.len(),
            Err(_) => {}
        }
    }

    for chunk in to_update.chunks(100) {
        match client::bitable::update_records(client, app_token, table_id, chunk).await {
            Ok(_) => updated_count += chunk.len(),
            Err(_) => {}
        }
    }

    Ok(json!({
        "inserted": inserted_count,
        "updated": updated_count,
        "skipped": skipped,
        "fields_added": fields_added,
        "total_rows": rows.len(),
    }))
}
