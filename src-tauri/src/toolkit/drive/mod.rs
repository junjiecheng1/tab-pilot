// toolkit/drive — 云空间文件操作
//
// 移植自: aily_drive/ (282行)
// 独立实现上传/下载逻辑 (含 mount 解析、预览、大小格式化)
// 底层调用 toolkit/client/drive

pub mod dispatch;
pub mod ops;

// 保留 base::file_ops 的 re-export 用于兼容
pub use crate::toolkit::base::file_ops::{download_files, upload_files};
