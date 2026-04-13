// CLI 工具 + Toolkit 自动下载管理
// 分为三个模块: registry (列表与常量), downloader (网络下载与解压), manager (初始化检查与生命周期)

mod downloader;
mod manager;
mod registry;

pub use manager::ToolsManager;
pub use registry::{tool_list, ToolKind};
