// MCP Bridge — HTTP + stdio 双传输 MCP 服务器管理
//
// V2: 支持 HTTP (远程) + stdio (本地) 双传输
//
// 子模块:
//   config  — 配置解析 (McpTransport, McpConfig, McpToolInfo)
//   session — 传输会话 (HttpSession, StdioSession)
//   bridge  — 统一管理层 (McpBridge)

mod bridge;
mod config;
mod session;

pub use bridge::McpBridge;
