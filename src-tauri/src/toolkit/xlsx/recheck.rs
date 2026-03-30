// toolkit/xlsx/recheck — 错误检测
//
// 移植自: cmd_recheck

use std::path::Path;
use serde_json::{json, Value};
use calamine::{open_workbook_auto, Reader, Data};
use crate::toolkit::client::TabClientError;

/// 检测 Excel 中的错误单元格
pub fn recheck_file(path: &Path) -> Result<Value, TabClientError> {
    let mut workbook = open_workbook_auto(path)
        .map_err(|e| TabClientError::Other(format!("无法打开: {e}")))?;

    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();
    let mut errors: Vec<Value> = Vec::new();

    for name in &sheet_names {
        if let Ok(range) = workbook.worksheet_range(name) {
            for (row_idx, row) in range.rows().enumerate() {
                for (col_idx, cell) in row.iter().enumerate() {
                    if let Some(error) = check_cell(cell) {
                        errors.push(json!({
                            "sheet": name,
                            "cell": format!("{}{}", col_letter(col_idx), row_idx + 1),
                            "error": error,
                        }));
                    }
                }
            }
        }
    }

    Ok(json!({
        "errors": errors,
        "error_count": errors.len(),
        "sheets_checked": sheet_names.len(),
    }))
}

fn check_cell(cell: &Data) -> Option<String> {
    match cell {
        Data::Error(e) => Some(format!("{e:?}")),
        Data::String(s) if s.starts_with("#") => Some(s.clone()),
        _ => None,
    }
}

fn col_letter(idx: usize) -> String {
    let mut result = String::new();
    let mut n = idx;
    loop {
        result.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 { break; }
        n = n / 26 - 1;
    }
    result
}
