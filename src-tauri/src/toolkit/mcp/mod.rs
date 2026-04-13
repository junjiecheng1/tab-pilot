// toolkit/mcp — MCP 工具调用
//
// 移植自: aily_mcp/commands/mcp.py
// 复用 infra::mcp::bridge 模块

use crate::infra::mcp::McpBridge;
use crate::toolkit::client::TabClientError;
use serde_json::{json, Value};

/// 列出所有 MCP 服务器
pub fn list_servers(bridge: &McpBridge) -> Result<Value, TabClientError> {
    let names = bridge.server_names();
    let status = bridge.session_status();

    Ok(json!({
        "servers": names,
        "count": names.len(),
        "status": status,
    }))
}

/// 列出指定服务器的可用工具
pub async fn list_tools(
    bridge: &mut McpBridge,
    server_name: &str,
) -> Result<Value, TabClientError> {
    let tools = bridge
        .list_tools_for(server_name)
        .await
        .map_err(|e: String| TabClientError::Other(format!("列出工具失败: {e}")))?;

    let count = tools.len();
    Ok(json!({
        "server": server_name,
        "tools": tools,
        "count": count,
    }))
}

/// 列出所有服务器的全部工具
pub async fn discover_all_tools(bridge: &mut McpBridge) -> Result<Value, TabClientError> {
    let discovered = bridge.discover_tools().await;

    let tool_list: Vec<Value> = discovered
        .iter()
        .map(|t| {
            json!({
                "server": &t.server,
                "name": &t.name,
                "description": &t.description,
            })
        })
        .collect();

    let count = tool_list.len();
    Ok(json!({
        "tools": tool_list,
        "count": count,
    }))
}

/// 执行 MCP 工具调用
pub async fn call_tool(
    bridge: &mut McpBridge,
    server_name: &str,
    tool_name: &str,
    arguments: Value,
) -> Result<Value, TabClientError> {
    let result = bridge
        .call(server_name, tool_name, arguments)
        .await
        .map_err(|e: String| TabClientError::Other(format!("MCP 调用失败: {e}")))?;

    Ok(json!({
        "server": server_name,
        "tool": tool_name,
        "result": result,
        "status": "ok",
    }))
}
