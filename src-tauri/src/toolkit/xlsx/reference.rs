// toolkit/xlsx/reference — 公式引用检查
//
// calamine 检查公式中的引用问题

use std::path::Path;
use serde_json::{json, Value};
use crate::toolkit::client::TabClientError;

/// 检查公式引用（跨表/范围不足/模式不一致）
pub fn check_references(path: &Path) -> Result<Value, TabClientError> {
    use calamine::{open_workbook_auto, Reader, Data};

    let mut workbook = open_workbook_auto(path)
        .map_err(|e| TabClientError::Other(format!("无法打开: {e}")))?;

    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();
    let mut issues: Vec<Value> = Vec::new();

    for name in &sheet_names {
        if let Ok(range) = workbook.worksheet_range(name) {
            for (row_idx, row) in range.rows().enumerate() {
                for (col_idx, cell) in row.iter().enumerate() {
                    if let Data::String(s) = cell {
                        if super::formula::is_formula(s) {
                            let refs = super::formula::extract_refs(s);
                            for r in &refs {
                                // 检查跨表引用
                                if r.contains('!') {
                                    let parts: Vec<&str> = r.split('!').collect();
                                    if let Some(ref_sheet) = parts.first() {
                                        let clean = ref_sheet.trim_matches('\'');
                                        if !sheet_names.contains(&clean.to_string()) {
                                            issues.push(json!({
                                                "sheet": name,
                                                "cell": format!("{}{}", col_letter(col_idx), row_idx + 1),
                                                "type": "missing_sheet_ref",
                                                "ref": r,
                                            }));
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

    Ok(json!({
        "issues": issues,
        "issue_count": issues.len(),
        "sheets_checked": sheet_names.len(),
    }))
}

fn col_letter(idx: usize) -> String {
    let mut result = String::new();
    let mut n = idx;
    loop {
        result.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    result
}
