// toolkit/xlsx/formula — 公式工具函数

use regex::Regex;

/// 判断是否公式
pub fn is_formula(s: &str) -> bool {
    let trimmed = s.trim();
    trimmed.starts_with('=') && trimmed.len() > 1
}

/// 提取公式中的引用
pub fn extract_refs(formula: &str) -> Vec<String> {
    // 匹配 A1, $A$1, Sheet1!A1, 'Sheet Name'!A1:B2
    let re = Regex::new(
        r"(?:'[^']+?'!\$?[A-Z]+\$?\d+(?::\$?[A-Z]+\$?\d+)?)|(?:[A-Za-z_]\w*!\$?[A-Z]+\$?\d+(?::\$?[A-Z]+\$?\d+)?)|(?:\$?[A-Z]{1,3}\$?\d+(?::\$?[A-Z]{1,3}\$?\d+)?)"
    ).unwrap();

    re.find_iter(formula)
        .map(|m| m.as_str().to_string())
        .collect()
}

/// 解析范围字符串 (e.g. "A1:B3" → (0,0,1,2))
pub fn parse_range(range_str: &str) -> Option<(usize, usize, usize, usize)> {
    let parts: Vec<&str> = range_str.split(':').collect();
    if parts.is_empty() || parts.len() > 2 {
        return None;
    }

    let (col1, row1) = parse_cell_ref(parts[0])?;
    if parts.len() == 1 {
        return Some((row1, col1, row1, col1));
    }

    let (col2, row2) = parse_cell_ref(parts[1])?;
    Some((row1, col1, row2, col2))
}

fn parse_cell_ref(cell: &str) -> Option<(usize, usize)> {
    let clean = cell.replace('$', "");
    // 可能有 sheet! 前缀
    let part = if clean.contains('!') {
        clean.split('!').last()?
    } else {
        &clean
    };

    let mut col_str = String::new();
    let mut row_str = String::new();

    for c in part.chars() {
        if c.is_ascii_alphabetic() {
            col_str.push(c.to_ascii_uppercase());
        } else if c.is_ascii_digit() {
            row_str.push(c);
        }
    }

    if col_str.is_empty() || row_str.is_empty() {
        return None;
    }

    let col = col_str
        .chars()
        .fold(0usize, |acc, c| acc * 26 + (c as usize - 'A' as usize + 1))
        - 1;
    let row = row_str.parse::<usize>().ok()? - 1;

    Some((col, row))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_formula() {
        assert!(is_formula("=SUM(A1:A10)"));
        assert!(is_formula("=A1+B1"));
        assert!(!is_formula("Hello"));
        assert!(!is_formula("="));
    }

    #[test]
    fn test_extract_refs() {
        let refs = extract_refs("=SUM(A1:A10)+B2");
        assert!(refs.contains(&"A1:A10".to_string()));
        assert!(refs.contains(&"B2".to_string()));
    }

    #[test]
    fn test_parse_range() {
        assert_eq!(parse_range("A1:B3"), Some((0, 0, 2, 1)));
        assert_eq!(parse_range("A1"), Some((0, 0, 0, 0)));
    }
}
