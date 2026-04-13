// toolkit/xlsx/pivot — 透视表生成
//
// 移植自: cmd_pivot (200行)

use crate::toolkit::client::TabClientError;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

/// 生成透视表
pub fn create_pivot(
    path: &Path,
    sheet_name: Option<&str>,
    row_field: &str,
    col_field: &str,
    value_field: &str,
    agg: &str, // sum, avg, count, min, max
) -> Result<Value, TabClientError> {
    use calamine::{open_workbook_auto, Data, Reader};

    let mut workbook =
        open_workbook_auto(path).map_err(|e| TabClientError::Other(format!("无法打开: {e}")))?;

    let name = sheet_name
        .map(String::from)
        .or_else(|| workbook.sheet_names().first().cloned())
        .ok_or_else(|| TabClientError::Other("No sheets".into()))?;

    let range = workbook
        .worksheet_range(&name)
        .map_err(|e| TabClientError::Other(format!("无法读取sheet: {e}")))?;

    let rows: Vec<Vec<Data>> = range.rows().map(|r| r.to_vec()).collect();
    if rows.is_empty() {
        return Err(TabClientError::Other("Empty sheet".into()));
    }

    // 找到列索引
    let headers: Vec<String> = rows[0].iter().map(|c| cell_str(c)).collect();
    let row_idx = headers
        .iter()
        .position(|h| h == row_field)
        .ok_or_else(|| TabClientError::InvalidParam(format!("Row field not found: {row_field}")))?;
    let col_idx = headers
        .iter()
        .position(|h| h == col_field)
        .ok_or_else(|| TabClientError::InvalidParam(format!("Col field not found: {col_field}")))?;
    let val_idx = headers
        .iter()
        .position(|h| h == value_field)
        .ok_or_else(|| {
            TabClientError::InvalidParam(format!("Value field not found: {value_field}"))
        })?;

    // 聚合
    let mut pivot: HashMap<String, HashMap<String, Vec<f64>>> = HashMap::new();

    for row in rows.iter().skip(1) {
        let rk = cell_str(&row[row_idx]);
        let ck = cell_str(&row[col_idx]);
        let val = cell_f64(&row[val_idx]);

        pivot
            .entry(rk)
            .or_default()
            .entry(ck)
            .or_default()
            .push(val);
    }

    // 计算聚合结果
    let mut result_rows: Vec<Value> = Vec::new();
    let mut col_keys: Vec<String> = Vec::new();

    for row_entries in pivot.values() {
        for ck in row_entries.keys() {
            if !col_keys.contains(ck) {
                col_keys.push(ck.clone());
            }
        }
    }
    col_keys.sort();

    for (rk, row_entries) in &pivot {
        let mut row_obj = serde_json::Map::new();
        row_obj.insert(row_field.into(), json!(rk));

        for ck in &col_keys {
            let vals = row_entries.get(ck).map(|v| v.as_slice()).unwrap_or(&[]);
            let agg_val = aggregate(vals, agg);
            row_obj.insert(ck.clone(), json!(agg_val));
        }

        result_rows.push(Value::Object(row_obj));
    }

    Ok(json!({
        "pivot": result_rows,
        "row_field": row_field,
        "col_field": col_field,
        "value_field": value_field,
        "aggregation": agg,
        "col_keys": col_keys,
    }))
}

fn aggregate(vals: &[f64], agg: &str) -> f64 {
    if vals.is_empty() {
        return 0.0;
    }
    match agg {
        "sum" => vals.iter().sum(),
        "avg" => vals.iter().sum::<f64>() / vals.len() as f64,
        "count" => vals.len() as f64,
        "min" => vals.iter().cloned().fold(f64::INFINITY, f64::min),
        "max" => vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        _ => vals.iter().sum(),
    }
}

fn cell_str(cell: &calamine::Data) -> String {
    use calamine::Data;
    match cell {
        Data::String(s) => s.clone(),
        Data::Float(f) => f.to_string(),
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn cell_f64(cell: &calamine::Data) -> f64 {
    use calamine::Data;
    match cell {
        Data::Float(f) => *f,
        Data::Int(i) => *i as f64,
        Data::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}
