// toolkit/client/bitable — Bitable API
//
// 移植自: aily_client/api/bitable/ (record+field+table+view+app, 224行)
// 改动: client.call("bitable.xxx") → 飞书 OpenAPI POST/GET/DELETE

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{PageData, Result, TabClient};

// ── Record ──────────────────────────────

/// 搜索记录（分页）
pub async fn search_records(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
    page_size: i32,
    page_token: Option<&str>,
) -> Result<PageData<Value>> {
    let mut body = json!({
        "page_size": page_size,
    });
    if let Some(pt) = page_token {
        body["page_token"] = json!(pt);
    }
    client
        .post(
            &format!("/bitable/v1/apps/{app_token}/tables/{table_id}/records/search"),
            &body,
        )
        .await
}

/// 高级搜索记录
pub async fn advanced_search_records(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
    filter: Option<&Value>,
    sort: Option<&Value>,
    field_names: Option<&[String]>,
    automatic_fields: bool,
    view_id: Option<&str>,
    page_size: i32,
    page_token: Option<&str>,
) -> Result<PageData<Value>> {
    let mut body = json!({
        "page_size": page_size,
    });
    if let Some(f) = filter {
        body["filter"] = f.clone();
    }
    if let Some(s) = sort {
        body["sort"] = s.clone();
    }
    if let Some(names) = field_names {
        body["field_names"] = json!(names);
    }
    if automatic_fields {
        body["automatic_fields"] = json!(true);
    }
    if let Some(vid) = view_id {
        body["view_id"] = json!(vid);
    }
    if let Some(pt) = page_token {
        body["page_token"] = json!(pt);
    }
    client
        .post(
            &format!("/bitable/v1/apps/{app_token}/tables/{table_id}/records/search"),
            &body,
        )
        .await
}

/// 批量添加记录
pub async fn add_records(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
    records: &[Value],
) -> Result<Value> {
    let body = json!({ "records": records });
    client
        .post(
            &format!("/bitable/v1/apps/{app_token}/tables/{table_id}/records/batch_create"),
            &body,
        )
        .await
}

/// 批量更新记录
pub async fn update_records(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
    records: &[Value],
) -> Result<Value> {
    let body = json!({ "records": records });
    client
        .post(
            &format!("/bitable/v1/apps/{app_token}/tables/{table_id}/records/batch_update"),
            &body,
        )
        .await
}

/// 批量删除记录
pub async fn delete_records(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
    record_ids: &[String],
) -> Result<Value> {
    let body = json!({ "records": record_ids });
    client
        .delete(
            &format!("/bitable/v1/apps/{app_token}/tables/{table_id}/records/batch_delete"),
            &body,
        )
        .await
}

/// 批量获取记录
pub async fn batch_get_records(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
    record_ids: &[String],
) -> Result<Value> {
    let body = json!({ "record_ids": record_ids });
    client
        .post(
            &format!("/bitable/v1/apps/{app_token}/tables/{table_id}/records/batch_get"),
            &body,
        )
        .await
}

/// 获取记录总数
pub async fn get_records_count(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
) -> Result<Value> {
    client
        .get(
            &format!("/bitable/v1/apps/{app_token}/tables/{table_id}/records"),
            &[("page_size", "1")],
        )
        .await
}

// ── Field ──────────────────────────────

/// 列出字段
pub async fn list_fields(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
) -> Result<PageData<Value>> {
    client
        .get(
            &format!("/bitable/v1/apps/{app_token}/tables/{table_id}/fields"),
            &[],
        )
        .await
}

/// 添加字段
pub async fn add_field(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
    field_name: &str,
    field_type: i32,
    ui_type: Option<&str>,
    property: Option<&Value>,
) -> Result<Value> {
    let mut body = json!({
        "field_name": field_name,
        "type": field_type,
    });
    if let Some(ut) = ui_type {
        body["ui_type"] = json!(ut);
    }
    if let Some(prop) = property {
        body["property"] = prop.clone();
    }
    client
        .post(
            &format!("/bitable/v1/apps/{app_token}/tables/{table_id}/fields"),
            &body,
        )
        .await
}

/// 删除字段
pub async fn delete_field(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
    field_id: &str,
) -> Result<Value> {
    client
        .delete(
            &format!("/bitable/v1/apps/{app_token}/tables/{table_id}/fields/{field_id}"),
            &json!({}),
        )
        .await
}

// ── Table ──────────────────────────────

/// 列出表格
pub async fn list_tables(
    client: &TabClient,
    app_token: &str,
) -> Result<PageData<Value>> {
    client
        .get(
            &format!("/bitable/v1/apps/{app_token}/tables"),
            &[],
        )
        .await
}

/// 添加表格
pub async fn add_table(
    client: &TabClient,
    app_token: &str,
    name: &str,
    default_view_name: Option<&str>,
    fields: Option<&[Value]>,
) -> Result<Value> {
    let mut table = json!({ "name": name });
    if let Some(dvn) = default_view_name {
        table["default_view_name"] = json!(dvn);
    }
    if let Some(f) = fields {
        table["fields"] = json!(f);
    }
    let body = json!({ "table": table });
    client
        .post(
            &format!("/bitable/v1/apps/{app_token}/tables"),
            &body,
        )
        .await
}

/// 删除表格
pub async fn delete_table(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
) -> Result<Value> {
    client
        .delete(
            &format!("/bitable/v1/apps/{app_token}/tables/{table_id}"),
            &json!({}),
        )
        .await
}

// ── View ──────────────────────────────

/// 列出视图
pub async fn list_views(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
) -> Result<PageData<Value>> {
    client
        .get(
            &format!("/bitable/v1/apps/{app_token}/tables/{table_id}/views"),
            &[],
        )
        .await
}

/// 获取视图详情
pub async fn get_view(
    client: &TabClient,
    app_token: &str,
    table_id: &str,
    view_id: &str,
) -> Result<Value> {
    client
        .get(
            &format!("/bitable/v1/apps/{app_token}/tables/{table_id}/views/{view_id}"),
            &[],
        )
        .await
}

// ── App ──────────────────────────────

/// 创建多维表格应用
pub async fn create_app(
    client: &TabClient,
    name: &str,
    extra: Option<&Value>,
) -> Result<Value> {
    let mut body = json!({ "name": name });
    if let Some(ext) = extra {
        if let Some(obj) = ext.as_object() {
            for (k, v) in obj {
                body[k] = v.clone();
            }
        }
    }
    client.post("/bitable/v1/apps", &body).await
}
