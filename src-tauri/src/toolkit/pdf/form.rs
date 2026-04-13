// toolkit/pdf/form — PDF 表单读写
//
// 移植自: aily_pdf/cmd_form.py (336行)
// 依赖: lopdf

use crate::toolkit::client::TabClientError;
use lopdf::Object;
use serde_json::{json, Value};
use std::path::Path;

/// 读取 PDF 表单字段
pub fn read_form(path: &Path) -> Result<Value, TabClientError> {
    let doc = lopdf::Document::load(path)
        .map_err(|e| TabClientError::Other(format!("无法打开 PDF: {e}")))?;

    let mut fields: Vec<Value> = Vec::new();

    // 遍历 AcroForm 字段
    if let Ok(catalog) = doc.catalog() {
        if let Ok(acroform_ref) = catalog.get(b"AcroForm") {
            if let Ok(form_obj) = doc.dereference(acroform_ref) {
                if let Ok(form_dict) = form_obj.1.as_dict() {
                    if let Ok(field_refs) = form_dict.get(b"Fields").and_then(|v| v.as_array()) {
                        for field_ref in field_refs {
                            if let Ok((_, field_obj)) = doc.dereference(field_ref) {
                                if let Ok(field_dict) = field_obj.as_dict() {
                                    let name = field_dict
                                        .get(b"T")
                                        .ok()
                                        .and_then(|v| v.as_str().ok())
                                        .map(|s| String::from_utf8_lossy(s).to_string())
                                        .unwrap_or_default();

                                    let value = field_dict
                                        .get(b"V")
                                        .ok()
                                        .and_then(|v| v.as_str().ok())
                                        .map(|s| String::from_utf8_lossy(s).to_string())
                                        .unwrap_or_default();

                                    let field_type = field_dict
                                        .get(b"FT")
                                        .ok()
                                        .and_then(|v| v.as_name_str().ok())
                                        .unwrap_or("unknown")
                                        .to_string();

                                    fields.push(json!({
                                        "name": name,
                                        "value": value,
                                        "type": field_type,
                                    }));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(json!({
        "field_count": fields.len(),
        "fields": fields,
    }))
}

/// 填写 PDF 表单
pub fn fill_form(
    input_path: &Path,
    output_path: &Path,
    values: &Value,
) -> Result<Value, TabClientError> {
    let mut doc = lopdf::Document::load(input_path)
        .map_err(|e| TabClientError::Other(format!("无法打开 PDF: {e}")))?;

    let entries = values
        .as_object()
        .ok_or_else(|| TabClientError::InvalidParam("values must be object".into()))?;

    // 找到所有表单字段的 object id
    let mut field_ids: Vec<(lopdf::ObjectId, String)> = Vec::new();

    if let Ok(catalog) = doc.catalog().cloned() {
        if let Ok(acroform_ref) = catalog.get(b"AcroForm") {
            if let Ok(form_pair) = doc.dereference(acroform_ref) {
                if let Ok(form_dict) = form_pair.1.as_dict() {
                    if let Ok(field_refs) = form_dict.get(b"Fields").and_then(|v| v.as_array()) {
                        for field_ref in field_refs {
                            if let Ok(pair) = doc.dereference(field_ref) {
                                if let Some(obj_id) = pair.0 {
                                    if let Ok(field_dict) = pair.1.as_dict() {
                                        let name = field_dict
                                            .get(b"T")
                                            .ok()
                                            .and_then(|v| v.as_str().ok())
                                            .map(|s| String::from_utf8_lossy(s).to_string())
                                            .unwrap_or_default();
                                        field_ids.push((obj_id, name));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut filled = 0;
    for (obj_id, name) in &field_ids {
        if let Some(val) = entries.get(name) {
            let val_str = val.as_str().unwrap_or(&val.to_string()).to_string();
            if let Ok(obj) = doc.get_object_mut(*obj_id) {
                if let Ok(dict) = obj.as_dict_mut() {
                    dict.set(
                        b"V",
                        Object::String(val_str.into_bytes(), lopdf::StringFormat::Literal),
                    );
                    filled += 1;
                }
            }
        }
    }

    doc.save(output_path)
        .map_err(|e| TabClientError::Other(format!("保存失败: {e}")))?;

    Ok(json!({
        "filled": filled,
        "total_entries": entries.len(),
        "output": output_path.to_string_lossy(),
    }))
}
