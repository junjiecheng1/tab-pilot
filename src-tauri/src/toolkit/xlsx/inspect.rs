// toolkit/xlsx/inspect — Excel 结构分析
//
// calamine 读取 + 列类型推断

use std::path::Path;
use serde_json::{json, Value};
use crate::toolkit::client::TabClientError;

/// 分析 Excel 文件结构
pub fn inspect_file(path: &Path) -> Result<Value, TabClientError> {
    use calamine::{open_workbook_auto, Reader, Data};

    let mut workbook = open_workbook_auto(path)
        .map_err(|e| TabClientError::Other(format!("无法打开文件: {e}")))?;

    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();
    let mut sheets = Vec::new();

    for name in &sheet_names {
        if let Ok(range) = workbook.worksheet_range(name) {
            let rows = range.rows().collect::<Vec<_>>();
            let row_count = rows.len();
            let col_count = if row_count > 0 { rows[0].len() } else { 0 };

            // 提取表头
            let headers: Vec<String> = if row_count > 0 {
                rows[0].iter().map(|c| cell_to_string(c)).collect()
            } else {
                Vec::new()
            };

            // 推断列类型（采样前 100 行）
            let sample_rows = rows.iter().skip(1).take(100);
            let mut col_types: Vec<String> = vec!["unknown".into(); col_count];

            for row in sample_rows {
                for (i, cell) in row.iter().enumerate() {
                    if i < col_count {
                        let t = match cell {
                            Data::Int(_) | Data::Float(_) => "number",
                            Data::Bool(_) => "boolean",
                            Data::DateTime(_) | Data::DateTimeIso(_) | Data::DurationIso(_) => "datetime",
                            Data::String(s) if s.starts_with("http") => "url",
                            Data::String(_) => "text",
                            Data::Empty => "empty",
                            _ => "unknown",
                        };
                        if col_types[i] == "unknown" || col_types[i] == "empty" {
                            col_types[i] = t.to_string();
                        }
                    }
                }
            }

            sheets.push(json!({
                "name": name,
                "row_count": row_count,
                "col_count": col_count,
                "headers": headers,
                "col_types": col_types,
            }));
        }
    }

    Ok(json!({
        "file": path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        "sheet_count": sheet_names.len(),
        "sheets": sheets,
    }))
}

fn cell_to_string(cell: &calamine::Data) -> String {
    use calamine::Data;
    match cell {
        Data::String(s) => s.clone(),
        Data::Float(f) => f.to_string(),
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Empty => String::new(),
        Data::Error(e) => format!("#ERR:{e:?}"),
    }
}
