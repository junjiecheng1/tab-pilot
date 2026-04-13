// Tab Toolkit — 从 Aily SO 反编译移植的工具箱
//
// 包含飞书 OpenAPI 调用、数据表操作、消息处理、文档、
// 文件存储、Excel、PDF、图表、图、图片等功能模块。

pub mod base; // Bitable 数据操作
pub mod chart; // Plotly 图表
pub mod client; // 飞书 OpenAPI 客户端
pub mod diagram; // Mermaid 图
pub mod doc; // 文档操作
pub mod download; // URL 下载辅助 (pdf/xlsx 共用)
pub mod drive; // 文件存储
pub mod im; // 消息处理
pub mod image; // 图片水印
pub mod mcp; // MCP 工具调用
pub mod openai; // OpenAI 兼容传输层
pub mod pdf; // PDF 处理
pub mod skillhub;
pub mod xlsx; // Excel 处理 // 技能市场
