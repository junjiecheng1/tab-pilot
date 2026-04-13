// toolkit/base/export — 导出 Bitable 数据
//
// 移植自: aily_base/commands/export.py (123行)

use serde_json::{json, Value};

use crate::toolkit::client::{self, Result, TabClient};

/// 导出单表全部数据
pub async fn export_table(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
    include_record_id: bool,
) -> Result<Value> {
    // 获取字段定义
    let fields_data = client::bitable::list_fields(client, app_token, table_id).await?;
    let fields = fields_data.items;

    // 分页获取全部记录
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

    // 规范化记录
    let mut rows: Vec<Value> = Vec::new();
    for r in &all_records {
        let record_fields = r.get("fields").and_then(|v| v.as_object());
        let mut row = serde_json::Map::new();

        if include_record_id {
            if let Some(id) = r.get("record_id").and_then(|v| v.as_str()) {
                row.insert("record_id".into(), json!(id));
            }
        }

        if let Some(rf) = record_fields {
            for (k, v) in rf {
                let normalized = normalize_field_value(v);
                row.insert(k.clone(), normalized);
            }
        }

        rows.push(Value::Object(row));
    }

    let primary_field = fields
        .first()
        .and_then(|f| f.get("field_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    Ok(json!({
        "fields": fields,
        "rows": rows,
        "total": rows.len(),
        "primary_field": primary_field,
    }))
}

/// 导出多表数据（并发）
pub async fn export_tables(
    client: &TabClient,
    app_token: &str,
    table_ids: Option<&[String]>,
    include_record_id: bool,
) -> Result<Value> {
    // 如果没有指定表，获取所有表
    let tables = match table_ids {
        Some(ids) => ids.to_vec(),
        None => {
            let tables_data = client::bitable::list_tables(client, app_token).await?;
            tables_data
                .items
                .iter()
                .filter_map(|t| t.get("table_id").and_then(|v| v.as_str()).map(String::from))
                .collect()
        }
    };

    let mut results = serde_json::Map::new();
    let mut success_count = 0;

    // 并发导出每个表
    let mut handles = Vec::new();
    for table_id in &tables {
        let c = client.clone();
        let at = app_token.to_string();
        let tid = table_id.clone();
        let irid = include_record_id;

        handles.push(tokio::spawn(async move {
            let result = export_table(&c, &at, &tid, irid).await;
            (tid, result)
        }));
    }

    for handle in handles {
        if let Ok((tid, result)) = handle.await {
            match result {
                Ok(data) => {
                    results.insert(tid, data);
                    success_count += 1;
                }
                Err(e) => {
                    results.insert(tid, json!({"error": e.to_string()}));
                }
            }
        }
    }

    Ok(json!({
        "tables": results,
        "success_count": success_count,
        "total": tables.len(),
    }))
}

/// 规范化字段值
fn normalize_field_value(v: &Value) -> Value {
    match v {
        Value::Object(obj) if obj.contains_key("text") => obj
            .get("text")
            .cloned()
            .unwrap_or(Value::String(String::new())),
        Value::Array(arr) => {
            let normalized: Vec<Value> = arr
                .iter()
                .map(|item| {
                    if let Some(obj) = item.as_object() {
                        obj.get("text")
                            .cloned()
                            .unwrap_or_else(|| json!(item.to_string()))
                    } else {
                        item.clone()
                    }
                })
                .collect();
            Value::Array(normalized)
        }
        other => other.clone(),
    }
}
