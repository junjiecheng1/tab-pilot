// toolkit/skillhub — 技能市场
//
// 移植自: aily_skillhub/ (387行, 6个命令)

pub mod dispatch;

use serde_json::{json, Value};
use crate::toolkit::client::{Result, TabClient, TabClientError};

/// 探索技能
pub async fn explore(client: &TabClient) -> Result<Value> {
    // 通过飞书 OpenAPI 获取技能市场列表
    client.get_raw("/aily/v1/skills/explore", &[]).await
}

/// 搜索技能
pub async fn search(client: &TabClient, query: &str, page_size: i32) -> Result<Value> {
    let body = json!({
        "query": query,
        "page_size": page_size,
    });
    client.post_raw("/aily/v1/skills/search", &body).await
}

/// 列出已安装技能
pub async fn list_installed(client: &TabClient) -> Result<Value> {
    client.get_raw("/aily/v1/skills/installed", &[]).await
}

/// 查看技能详情
pub async fn inspect(client: &TabClient, skill_id: &str) -> Result<Value> {
    client
        .get_raw(&format!("/aily/v1/skills/{skill_id}"), &[])
        .await
}

/// 安装技能
pub async fn install(client: &TabClient, skill_id: &str) -> Result<Value> {
    let body = json!({ "skill_id": skill_id });
    client.post_raw("/aily/v1/skills/install", &body).await
}

/// 卸载技能
pub async fn uninstall(client: &TabClient, skill_id: &str) -> Result<Value> {
    let body = json!({ "skill_id": skill_id });
    client.post_raw("/aily/v1/skills/uninstall", &body).await
}
