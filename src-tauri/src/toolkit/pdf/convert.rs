// toolkit/pdf/convert — 文档转 PDF
//
// 当前: 仅支持已是 PDF 的文件 (直接拷贝)
// 未来: 接入 docx → PDF 的纯 Rust 方案
//
// 注: Python 原版通过 LibreOffice headless,
// Rust 端暂无等价的纯 crate 方案 (docx2pdf 不成熟)
// 后续可通过 CDP (headless Chrome) 打印来实现

use std::path::Path;
use serde_json::{json, Value};
use crate::toolkit::client::TabClientError;

/// 将文件转换为 PDF
///
/// 支持的方式:
/// - .pdf → 直接拷贝
/// - .html → 通过 CDP 打印 (需要 browser agent)
/// - 其他格式 → 提示不支持 (后续集成)
pub fn convert_to_pdf(
    input: &Path,
    output: Option<&Path>,
) -> Result<Value, TabClientError> {
    let ext = input.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let output_path = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| input.with_extension("pdf"));

    match ext.as_str() {
        "pdf" => {
            // 已经是 PDF
            if input != output_path.as_path() {
                std::fs::copy(input, &output_path)
                    .map_err(|e| TabClientError::Other(format!("拷贝失败: {e}")))?;
            }
            Ok(json!({
                "output_path": output_path.to_string_lossy(),
                "status": "already_pdf",
            }))
        }
        "html" | "htm" => {
            // HTML → PDF 应通过 CDP, 参见 pdf/html.rs
            Err(TabClientError::InvalidParam(
                "HTML → PDF 请使用 tab-pdf html 命令 (通过 CDP 渲染)".into()
            ))
        }
        _ => {
            Err(TabClientError::InvalidParam(format!(
                "格式 .{ext} 暂不支持直接转换 PDF. 支持格式: pdf, html"
            )))
        }
    }
}
