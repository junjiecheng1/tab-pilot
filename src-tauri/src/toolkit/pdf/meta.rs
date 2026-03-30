// toolkit/pdf/meta — PDF 元数据读写
//
// 移植自: aily_pdf/cmd_meta.py (94行)

use std::path::Path;
use serde_json::{json, Value};
use crate::toolkit::client::TabClientError;

/// 读取 PDF 元数据
pub fn read_metadata(path: &Path) -> Result<Value, TabClientError> {
    let doc = lopdf::Document::load(path)
        .map_err(|e| TabClientError::Other(format!("无法打开: {e}")))?;

    let page_count = doc.get_pages().len();

    let mut info = serde_json::Map::new();
    info.insert("page_count".into(), json!(page_count));

    // 读取 Info dict
    if let Ok(info_ref) = doc.trailer.get(b"Info") {
        if let Ok(pair) = doc.dereference(info_ref) {
            if let Ok(info_dict) = pair.1.as_dict() {
                for (key, val) in info_dict.iter() {
                    let k = String::from_utf8_lossy(key).to_string();
                    let v = match val {
                        lopdf::Object::String(s, _) => {
                            String::from_utf8_lossy(s).to_string()
                        }
                        other => format!("{other:?}"),
                    };
                    info.insert(k, json!(v));
                }
            }
        }
    }

    // PDF 版本
    info.insert("version".into(), json!(&doc.version));

    Ok(Value::Object(info))
}

/// 设置 PDF 元数据
pub fn set_metadata(
    input: &Path,
    output: &Path,
    metadata: &Value,
) -> Result<Value, TabClientError> {
    let mut doc = lopdf::Document::load(input)
        .map_err(|e| TabClientError::Other(format!("无法打开: {e}")))?;

    let entries = metadata.as_object()
        .ok_or_else(|| TabClientError::InvalidParam("metadata must be object".into()))?;

    // 获取或创建 Info dict 的 object id
    let info_id = if let Ok(info_ref) = doc.trailer.get(b"Info") {
        if let Ok(pair) = doc.dereference(info_ref) {
            pair.0.unwrap_or_else(|| doc.new_object_id())
        } else {
            doc.new_object_id()
        }
    } else {
        doc.new_object_id()
    };

    let mut set_count = 0;
    for (key, val) in entries {
        let val_str = val.as_str().unwrap_or(&val.to_string()).to_string();
        if let Ok(obj) = doc.get_object_mut(info_id) {
            if let Ok(dict) = obj.as_dict_mut() {
                dict.set(
                    key.as_bytes(),
                    lopdf::Object::String(
                        val_str.into_bytes(),
                        lopdf::StringFormat::Literal,
                    ),
                );
                set_count += 1;
            }
        }
    }

    doc.save(output)
        .map_err(|e| TabClientError::Other(format!("保存失败: {e}")))?;

    Ok(json!({
        "set_count": set_count,
        "output": output.to_string_lossy(),
    }))
}
