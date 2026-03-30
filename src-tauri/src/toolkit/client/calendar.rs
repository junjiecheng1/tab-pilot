// toolkit/client/calendar — 日历 API
//
// 移植自: aily_client/api/calendar/event.py (152行)

use serde_json::{json, Value};

use super::{Result, TabClient};

/// 列出日历事件（自动分页）
pub async fn list_events(
    client: &TabClient,
    calendar_id: &str,
    start_time: &str,
    end_time: &str,
) -> Result<Vec<Value>> {
    let base_params = vec![
        ("start_time", start_time.to_string()),
        ("end_time", end_time.to_string()),
    ];
    client
        .get_all_pages(
            &format!("/calendar/v4/calendars/{calendar_id}/events"),
            &base_params
                .iter()
                .map(|(k, v)| (*k, v.clone()))
                .collect::<Vec<_>>(),
        )
        .await
}

/// 搜索日历事件（自动分页）
pub async fn search_events(
    client: &TabClient,
    calendar_id: &str,
    query: &str,
    start_time: &str,
    end_time: Option<&str>,
    page_size: i32,
) -> Result<Vec<Value>> {
    let mut all_events = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let mut body = json!({
            "query": query,
            "filter": {
                "start_time": { "timestamp": start_time },
            },
            "page_size": page_size,
        });
        if let Some(et) = end_time {
            body["filter"]["end_time"] = json!({ "timestamp": et });
        }
        if let Some(ref pt) = page_token {
            body["page_token"] = json!(pt);
        }

        let resp: Value = client
            .post(
                &format!("/calendar/v4/calendars/{calendar_id}/events/search"),
                &body,
            )
            .await?;

        if let Some(items) = resp.get("items").and_then(|v| v.as_array()) {
            all_events.extend(items.clone());
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

    Ok(all_events)
}

/// 获取事件参与者（自动分页）
pub async fn get_attendees(
    client: &TabClient,
    calendar_id: &str,
    event_id: &str,
    page_size: i32,
) -> Result<Vec<Value>> {
    let base_params = vec![("page_size", page_size.to_string())];
    client
        .get_all_pages(
            &format!("/calendar/v4/calendars/{calendar_id}/events/{event_id}/attendees"),
            &base_params
                .iter()
                .map(|(k, v)| (*k, v.clone()))
                .collect::<Vec<_>>(),
        )
        .await
}

/// 创建日历事件
pub async fn create_event(
    client: &TabClient,
    calendar_id: &str,
    title: &str,
    start_time: &str,
    end_time: Option<&str>,
    description: Option<&str>,
) -> Result<Value> {
    let mut body = json!({
        "summary": title,
        "start_time": { "timestamp": start_time },
    });
    if let Some(et) = end_time {
        body["end_time"] = json!({ "timestamp": et });
    }
    if let Some(desc) = description {
        body["description"] = json!(desc);
    }
    client
        .post(
            &format!("/calendar/v4/calendars/{calendar_id}/events"),
            &body,
        )
        .await
}

/// 删除日历事件
pub async fn delete_event(
    client: &TabClient,
    calendar_id: &str,
    event_id: &str,
) -> Result<Value> {
    client
        .delete(
            &format!("/calendar/v4/calendars/{calendar_id}/events/{event_id}"),
            &json!({}),
        )
        .await
}

/// 获取空闲时间
pub async fn get_free_times(
    client: &TabClient,
    user_ids: &[String],
    start_time: &str,
    end_time: &str,
) -> Result<Value> {
    let body = json!({
        "time_min": start_time,
        "time_max": end_time,
        "user_id": user_ids.first().unwrap_or(&String::new()),
    });
    client
        .post("/calendar/v4/freebusy/list", &body)
        .await
}
