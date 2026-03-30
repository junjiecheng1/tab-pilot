// toolkit/base/create — 建表 + 批量写入
//
// 移植自: aily_base/commands/create.py (208行)

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::toolkit::client::{self, Result, TabClient, TabClientError};

/// Bitable 字段类型映射
fn field_type_map() -> HashMap<&'static str, i32> {
    HashMap::from([
        ("text", 1),
        ("number", 2),
        ("single_select", 3),
        ("multi_select", 4),
        ("date", 5),
        ("checkbox", 7),
        ("person", 11),
        ("url", 15),
        ("attachment", 17),
        ("formula", 20),
    ])
}

/// 创建 Bitable 表并写入数据
pub async fn create_table(
    client: &TabClient,
    fields: &[Value],
    rows: Option<&[Value]>,
    app_token: Option<&str>,
    app_name: Option<&str>,
    table_name: Option<&str>,
    url: Option<&str>,
) -> Result<Value> {
    let type_map = field_type_map();

    // 解析 URL
    let mut resolved_app_token = app_token.map(String::from);
    if let Some(u) = url {
        if resolved_app_token.is_none() {
            let info = super::parser::parse_bitable_url(u);
            resolved_app_token = info.get("app_token").cloned();
        }
    }

    // 创建 app（如果没有 app_token）
    let app_token_str = match resolved_app_token {
        Some(t) if !t.is_empty() => t,
        _ => {
            let name = app_name.unwrap_or("Tab App");
            let result = client::bitable::create_app(client, name, None).await?;
            result
                .get("app_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        }
    };

    if app_token_str.is_empty() {
        return Err(TabClientError::Other("Failed to get app_token".into()));
    }

    // 构建字段定义
    let mut api_fields: Vec<Value> = Vec::new();
    for field in fields {
        if let Some(fobj) = field.as_object() {
            let fname = fobj
                .get("field_name")
                .or_else(|| fobj.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let ftype_val = fobj.get("type");
            let ftype: i32 = match ftype_val {
                Some(Value::String(s)) => *type_map.get(s.as_str()).unwrap_or(&1),
                Some(Value::Number(n)) => n.as_i64().unwrap_or(1) as i32,
                _ => 1,
            };

            let mut api_field = json!({
                "field_name": fname,
                "type": ftype,
            });

            if let Some(prop) = fobj.get("property") {
                api_field["property"] = prop.clone();
            }
            if let Some(desc) = fobj.get("description") {
                api_field["description"] = desc.clone();
            }

            api_fields.push(api_field);
        } else if let Some(name) = field.as_str() {
            api_fields.push(json!({
                "field_name": name,
                "type": 1,
            }));
        }
    }

    // 创建表
    let tname = table_name.unwrap_or("Table");
    let table_result = client::bitable::add_table(
        client,
        &app_token_str,
        tname,
        None,
        Some(&api_fields),
    )
    .await?;

    let table_id = table_result
        .get("table_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 写入数据
    let mut records_added = 0;
    if let Some(rows) = rows {
        let clean_rows: Vec<Value> = rows
            .iter()
            .filter_map(|row| {
                if let Some(robj) = row.as_object() {
                    let mut clean = serde_json::Map::new();
                    for field in fields {
                        let fname = if let Some(fobj) = field.as_object() {
                            fobj.get("field_name")
                                .or_else(|| fobj.get("name"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                        } else {
                            field.as_str().unwrap_or("")
                        };
                        if let Some(val) = robj.get(fname) {
                            clean.insert(fname.to_string(), val.clone());
                        }
                    }
                    Some(json!({"fields": clean}))
                } else {
                    None
                }
            })
            .collect();

        // 分批 500 条
        for chunk in clean_rows.chunks(500) {
            match client::bitable::add_records(client, &app_token_str, &table_id, chunk).await {
                Ok(_) => records_added += chunk.len(),
                Err(_) => {}
            }
        }
    }

    let mut result = json!({
        "app_token": app_token_str,
        "table_id": table_id,
        "table_name": tname,
        "field_count": api_fields.len(),
        "records_added": records_added,
        "status": "ok",
    });

    result["url"] = json!(format!(
        "https://base.feishu.cn/base/{app_token_str}?table={table_id}"
    ));

    Ok(result)
}
