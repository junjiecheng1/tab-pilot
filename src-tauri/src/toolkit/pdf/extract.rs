// toolkit/pdf/extract — 文本提取
//
// 使用 pdf-extract (基于 lopdf, 支持 Type3/CID 等复杂字体)

use std::path::Path;
use serde_json::{json, Value};
use crate::toolkit::client::TabClientError;

/// 提取 PDF 文本
pub fn extract_text(path: &Path) -> Result<Value, TabClientError> {
    let pages = pdf_extract::extract_text_by_pages(path)
        .map_err(|e| TabClientError::Other(format!("PDF 文本提取失败: {e}")))?;

    let page_count = pages.len();
    let pages_text: Vec<Value> = pages
        .iter()
        .enumerate()
        .map(|(i, text)| json!({
            "page": i + 1,
            "text": text.trim(),
        }))
        .collect();

    let full_text: String = pages
        .iter()
        .map(|t| t.trim())
        .collect::<Vec<_>>()
        .join("\n\n");

    Ok(json!({
        "page_count": page_count,
        "pages": pages_text,
        "full_text": full_text,
        "char_count": full_text.len(),
    }))
}

/// 提取 PDF 图片
pub fn extract_images(path: &Path, output_dir: &Path) -> Result<Value, TabClientError> {
    let doc = lopdf::Document::load(path)
        .map_err(|e| TabClientError::Other(format!("无法打开 PDF: {e}")))?;

    std::fs::create_dir_all(output_dir)
        .map_err(|e| TabClientError::Other(e.to_string()))?;

    let mut images: Vec<Value> = Vec::new();
    let mut img_idx = 0;

    for (obj_id, obj) in &doc.objects {
        if let Ok(stream) = obj.as_stream() {
            let subtype = stream
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|v| v.as_name_str().ok())
                .unwrap_or("");

            if subtype == "Image" {
                img_idx += 1;
                let filename = format!("image_{img_idx}.bin");
                let out_path = output_dir.join(&filename);

                if let Ok(data) = stream.decompressed_content() {
                    let _ = std::fs::write(&out_path, &data);
                    images.push(json!({
                        "index": img_idx,
                        "file": filename,
                        "size": data.len(),
                        "object_id": format!("{} {}", obj_id.0, obj_id.1),
                    }));
                }
            }
        }
    }

    Ok(json!({
        "image_count": images.len(),
        "images": images,
        "output_dir": output_dir.to_string_lossy(),
    }))
}
