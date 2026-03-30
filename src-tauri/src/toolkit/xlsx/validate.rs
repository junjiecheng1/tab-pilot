// toolkit/xlsx/validate — Excel 文件验证
//
// zip crate 检查 OpenXML 结构

use std::io::Read;
use std::path::Path;
use serde_json::{json, Value};
use crate::toolkit::client::TabClientError;

/// 验证 Excel 文件
pub fn validate_file(path: &Path) -> Result<Value, TabClientError> {
    let file = std::fs::File::open(path)
        .map_err(|e| TabClientError::Other(format!("无法打开文件: {e}")))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| TabClientError::Other(format!("无效 ZIP/XLSX: {e}")))?;

    let mut issues: Vec<String> = Vec::new();

    // 检查必需文件
    let required = ["[Content_Types].xml", "xl/workbook.xml"];
    for req in &required {
        if archive.by_name(req).is_err() {
            issues.push(format!("Missing required file: {req}"));
        }
    }

    // 检查禁止的函数（在 sharedStrings/sheet 中搜索）
    let forbidden = ["IMPORTDATA", "IMPORTRANGE", "WEBSERVICE", "FILTERXML"];

    for i in 0..archive.len() {
        if let Ok(mut file) = archive.by_index(i) {
            let name = file.name().to_string();
            if name.contains("sheet") || name.contains("sharedStrings") {
                let mut content = String::new();
                if file.read_to_string(&mut content).is_ok() {
                    for func in &forbidden {
                        if content.contains(func) {
                            issues.push(format!("Forbidden function {func} in {name}"));
                        }
                    }
                }
            }
        }
    }

    // 检查外部引用
    if let Ok(mut rels) = archive.by_name("xl/_rels/workbook.xml.rels") {
        let mut content = String::new();
        if rels.read_to_string(&mut content).is_ok() {
            if content.contains("TargetMode=\"External\"") {
                issues.push("External references found in workbook.xml.rels".into());
            }
        }
    }

    Ok(json!({
        "valid": issues.is_empty(),
        "issues": issues,
        "file_count": archive.len(),
    }))
}
