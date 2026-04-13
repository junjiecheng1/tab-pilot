// toolkit/base/parser — URL 解析 + 数据验证 + CSV 转换
//
// 移植自: aily_base/parser.py (417行)

use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde_json::{json, Value};
use url::Url;

use crate::toolkit::client::Result;

/// 字段格式推断
pub struct FieldFormat {
    pub name: String,
    pub prop: HashMap<String, String>,
}

impl FieldFormat {
    /// 从列数据推断字段类型
    pub fn from_values(name: &str, values: &[&str]) -> Self {
        let prop = Self::infer_property(values);
        Self {
            name: name.to_string(),
            prop,
        }
    }

    fn infer_property(values: &[&str]) -> HashMap<String, String> {
        let clean: Vec<&str> = values
            .iter()
            .filter(|v| !v.trim().is_empty())
            .copied()
            .collect();

        if clean.is_empty() {
            return HashMap::from([("type".into(), "text".into())]);
        }

        // 检查数字
        let num_count = clean.iter().filter(|v| v.parse::<f64>().is_ok()).count();
        if num_count == clean.len() {
            let all_int = clean.iter().all(|v| v.parse::<i64>().is_ok());
            if all_int {
                return HashMap::from([
                    ("type".into(), "number".into()),
                    ("format".into(), "integer".into()),
                ]);
            }
            return HashMap::from([
                ("type".into(), "number".into()),
                ("format".into(), "decimal".into()),
            ]);
        }

        // 检查日期
        let date_re = [
            Regex::new(r"^\d{4}-\d{2}-\d{2}").unwrap(),
            Regex::new(r"^\d{4}/\d{2}/\d{2}").unwrap(),
            Regex::new(r"^\d{2}/\d{2}/\d{4}").unwrap(),
        ];
        let date_count = clean
            .iter()
            .filter(|v| date_re.iter().any(|re| re.is_match(v)))
            .count();
        if date_count as f64 > clean.len() as f64 * 0.8 {
            return HashMap::from([("type".into(), "datetime".into())]);
        }

        // 检查 URL
        let url_count = clean
            .iter()
            .filter(|v| v.starts_with("http://") || v.starts_with("https://"))
            .count();
        if url_count as f64 > clean.len() as f64 * 0.8 {
            return HashMap::from([("type".into(), "url".into())]);
        }

        HashMap::from([("type".into(), "text".into())])
    }
}

/// 数据验证器
pub struct DataValidator {
    fields: Vec<String>,
    require_fields: bool,
}

impl DataValidator {
    pub fn new(fields: Vec<String>, require_fields: bool) -> Self {
        Self {
            fields,
            require_fields,
        }
    }

    pub fn validate(&self, data: &Value) -> Value {
        let mut issues: Vec<String> = Vec::new();

        let obj = match data.as_object() {
            Some(o) => o,
            None => {
                return json!({
                    "valid": false,
                    "issues": ["Data must be an object"],
                })
            }
        };

        // 检查顶层 key
        let required_keys = ["fields", "rows"];
        for key in &required_keys {
            if !obj.contains_key(*key) {
                issues.push(format!("Missing required key: {key}"));
            }
        }

        let expected: HashSet<&str> = ["fields", "rows", "values"].into_iter().collect();
        let unexpected: Vec<&String> = obj
            .keys()
            .filter(|k| !expected.contains(k.as_str()))
            .collect();
        if !unexpected.is_empty() {
            issues.push(format!("Unexpected keys: {unexpected:?}"));
        }

        // 字段
        let fields_arr = obj
            .get("fields")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut field_names: Vec<String> = Vec::new();

        for (i, field) in fields_arr.iter().enumerate() {
            if let Some(fobj) = field.as_object() {
                let name = fobj
                    .get("field_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if name.is_empty() {
                    issues.push(format!("Field {i}: missing field_name"));
                }
                field_names.push(name.to_string());
            } else if let Some(s) = field.as_str() {
                field_names.push(s.to_string());
            } else {
                issues.push(format!("Field {i}: invalid type"));
            }
        }

        if self.require_fields {
            for f in &self.fields {
                if !field_names.contains(f) {
                    issues.push(format!("Missing required field: {f}"));
                }
            }
        }

        // 行
        let rows = obj
            .get("rows")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for (i, row) in rows.iter().enumerate() {
            if let Some(robj) = row.as_object() {
                for fn_ in &field_names {
                    if !robj.contains_key(fn_) {
                        issues.push(format!("Row {i}: missing field {fn_}"));
                    }
                }
            } else if let Some(arr) = row.as_array() {
                if arr.len() != field_names.len() {
                    issues.push(format!(
                        "Row {i}: expected {} values, got {}",
                        field_names.len(),
                        arr.len()
                    ));
                }
            } else {
                issues.push(format!("Row {i}: invalid type"));
            }
        }

        json!({
            "valid": issues.is_empty(),
            "issues": issues,
            "field_count": field_names.len(),
            "row_count": rows.len(),
        })
    }
}

/// 从 Bitable URL 解析 app_token / table_id / view_id
pub fn parse_bitable_url(url: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    result.insert("url".into(), url.to_string());

    let parts: Vec<&str> = url.trim().trim_matches('/').split('/').collect();

    for (i, part) in parts.iter().enumerate() {
        match *part {
            "base" if i + 1 < parts.len() => {
                // base/BascXXXX 可能带 ?table=...
                let token = parts[i + 1].split('?').next().unwrap_or("");
                result.insert("app_token".into(), token.to_string());
            }
            "table" if i + 1 < parts.len() => {
                result.insert("table_id".into(), parts[i + 1].to_string());
            }
            "view" if i + 1 < parts.len() => {
                result.insert("view_id".into(), parts[i + 1].to_string());
            }
            _ => {}
        }
    }

    // 也检查 query string
    if let Ok(parsed) = Url::parse(url.trim()) {
        for (k, v) in parsed.query_pairs() {
            if k == "table" {
                result
                    .entry("table_id".into())
                    .or_insert_with(|| v.to_string());
            }
        }
    }

    result
}

/// CSV 内容转 JSON
pub fn csv_to_json(content: &str) -> Value {
    let mut reader = csv::Reader::from_reader(content.as_bytes());

    let headers: Vec<String> = match reader.headers() {
        Ok(h) => h.iter().map(|s| s.trim().to_string()).collect(),
        Err(_) => return json!({"fields": [], "rows": []}),
    };

    let fields: Vec<Value> = headers.iter().map(|h| json!({"field_name": h})).collect();

    let mut rows: Vec<Value> = Vec::new();
    for result in reader.records() {
        if let Ok(record) = result {
            let mut row = serde_json::Map::new();
            for (i, val) in record.iter().enumerate() {
                if i < headers.len() {
                    row.insert(headers[i].clone(), json!(val.trim()));
                }
            }
            rows.push(Value::Object(row));
        }
    }

    json!({
        "fields": fields,
        "rows": rows,
    })
}
