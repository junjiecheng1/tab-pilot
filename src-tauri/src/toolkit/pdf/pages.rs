// toolkit/pdf/pages — 页面操作（拆分/旋转）
//
// 移植自: aily_pdf/cmd_pages.py (158行)
// 注意: lopdf 0.34 没有 merge_document，合并功能先标记 TODO

use crate::toolkit::client::TabClientError;
use serde_json::{json, Value};
use std::path::Path;

/// 拆分 PDF
pub fn split_pages(
    input: &Path,
    output_dir: &Path,
    page_ranges: Option<&[(u32, u32)]>,
) -> Result<Value, TabClientError> {
    let doc = lopdf::Document::load(input)
        .map_err(|e| TabClientError::Other(format!("无法打开: {e}")))?;

    std::fs::create_dir_all(output_dir).map_err(|e| TabClientError::Other(e.to_string()))?;

    let page_count = doc.get_pages().len() as u32;
    let ranges = page_ranges
        .map(|r| r.to_vec())
        .unwrap_or_else(|| (1..=page_count).map(|i| (i, i)).collect());

    let mut outputs: Vec<Value> = Vec::new();

    for (start, end) in &ranges {
        let mut new_doc = doc.clone();
        let pages_to_remove: Vec<u32> = (1..=page_count)
            .filter(|p| *p < *start || *p > *end)
            .collect();

        for page_num in pages_to_remove.iter().rev() {
            new_doc.delete_pages(&[*page_num]);
        }

        let filename = format!("pages_{start}-{end}.pdf");
        let out_path = output_dir.join(&filename);
        new_doc
            .save(&out_path)
            .map_err(|e| TabClientError::Other(format!("保存失败: {e}")))?;

        outputs.push(json!({
            "file": filename,
            "pages": format!("{start}-{end}"),
        }));
    }

    Ok(json!({
        "output_count": outputs.len(),
        "outputs": outputs,
        "source_pages": page_count,
    }))
}

/// 合并多个 PDF（逐页拷贝对象）
pub fn merge_pdfs(inputs: &[&Path], output: &Path) -> Result<Value, TabClientError> {
    if inputs.is_empty() {
        return Err(TabClientError::InvalidParam("No input files".into()));
    }

    let mut base = lopdf::Document::load(inputs[0])
        .map_err(|e| TabClientError::Other(format!("无法打开: {e}")))?;
    let mut total_pages = base.get_pages().len();

    for path in inputs.iter().skip(1) {
        let other = lopdf::Document::load(path)
            .map_err(|e| TabClientError::Other(format!("无法打开 {:?}: {e}", path)))?;

        let pages = other.get_pages();
        total_pages += pages.len();

        // 逐页拷贝: 将 other 的每个 page 对象及其引用对象复制到 base
        for (_page_num, page_id) in &pages {
            // 深拷贝 page 对象到 base
            if let Ok(page_obj) = other.get_object(*page_id) {
                let new_page_id = base.add_object(page_obj.clone());
                // 将新页面加入 base 的 Pages 节点
                if let Ok(catalog) = base.catalog().cloned() {
                    if let Ok(pages_ref) = catalog.get(b"Pages") {
                        if let Ok(pair) = base.dereference(pages_ref) {
                            if let Some(pages_id) = pair.0 {
                                if let Ok(pages_obj) = base.get_object_mut(pages_id) {
                                    if let Ok(pages_dict) = pages_obj.as_dict_mut() {
                                        if let Ok(kids) = pages_dict.get_mut(b"Kids") {
                                            if let Ok(kids_arr) = kids.as_array_mut() {
                                                kids_arr
                                                    .push(lopdf::Object::Reference(new_page_id));
                                            }
                                        }
                                        // 更新 Count
                                        if let Ok(count) = pages_dict.get(b"Count") {
                                            if let Ok(n) = count.as_i64() {
                                                pages_dict
                                                    .set("Count", lopdf::Object::Integer(n + 1));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    base.save(output)
        .map_err(|e| TabClientError::Other(format!("保存失败: {e}")))?;

    Ok(json!({
        "merged_files": inputs.len(),
        "total_pages": total_pages,
        "output": output.to_string_lossy(),
    }))
}

/// 旋转页面
pub fn rotate_pages(
    input: &Path,
    output: &Path,
    pages: &[u32],
    angle: i32,
) -> Result<Value, TabClientError> {
    let mut doc = lopdf::Document::load(input)
        .map_err(|e| TabClientError::Other(format!("无法打开: {e}")))?;

    let page_map = doc.get_pages();
    let mut rotated = 0;

    for page_num in pages {
        if let Some(page_id) = page_map.get(page_num) {
            if let Ok(page) = doc.get_object_mut(*page_id) {
                if let Ok(dict) = page.as_dict_mut() {
                    dict.set("Rotate", lopdf::Object::Integer(angle as i64));
                    rotated += 1;
                }
            }
        }
    }

    doc.save(output)
        .map_err(|e| TabClientError::Other(format!("保存失败: {e}")))?;

    Ok(json!({
        "rotated": rotated,
        "angle": angle,
        "output": output.to_string_lossy(),
    }))
}
