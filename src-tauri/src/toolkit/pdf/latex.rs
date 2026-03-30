// toolkit/pdf/latex — LaTeX → PDF
//
// 方案: tectonic crate (纯 Rust LaTeX 引擎)
// 需要 Cargo.toml: tectonic = "0.15"
//
// 如 tectonic 未引入, 使用 stub 并提示安装

use std::path::Path;
use serde_json::{json, Value};
use crate::toolkit::client::TabClientError;

/// LaTeX 字符串 → PDF bytes
///
/// 使用 tectonic crate 编译
#[cfg(feature = "tectonic")]
pub fn latex_to_pdf(
    tex_content: &str,
    output: &Path,
) -> Result<Value, TabClientError> {
    let pdf_data = tectonic::latex_to_pdf(tex_content)
        .map_err(|e| TabClientError::Other(format!("LaTeX 编译失败: {e}")))?;

    std::fs::write(output, &pdf_data)
        .map_err(|e| TabClientError::Other(format!("写入 PDF 失败: {e}")))?;

    Ok(json!({
        "output": output.to_string_lossy(),
        "tex_length": tex_content.len(),
        "pdf_size": pdf_data.len(),
        "status": "ok",
    }))
}

/// LaTeX → PDF (fallback: 未启用 tectonic feature)
#[cfg(not(feature = "tectonic"))]
pub fn latex_to_pdf(
    tex_content: &str,
    output: &Path,
) -> Result<Value, TabClientError> {
    // 尝试用 CLI tectonic
    let tectonic_path = find_tectonic();
    if let Some(cmd) = tectonic_path {
        return compile_with_cli(&cmd, tex_content, output);
    }

    Err(TabClientError::Other(
        "LaTeX 编译不可用. 请安装: cargo install tectonic".into()
    ))
}

/// 查找 tectonic CLI
fn find_tectonic() -> Option<String> {
    // which
    if let Ok(out) = std::process::Command::new("which").arg("tectonic").output() {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() { return Some(p); }
        }
    }
    // cargo bin
    if let Ok(home) = std::env::var("HOME") {
        let p = format!("{home}/.cargo/bin/tectonic");
        if Path::new(&p).exists() { return Some(p); }
    }
    None
}

/// 通过 CLI 编译
fn compile_with_cli(
    tectonic_cmd: &str,
    tex_content: &str,
    output: &Path,
) -> Result<Value, TabClientError> {
    let tmp_dir = std::env::temp_dir().join("tab-latex");
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| TabClientError::Other(format!("{e}")))?;

    let tex_path = tmp_dir.join("input.tex");
    std::fs::write(&tex_path, tex_content)
        .map_err(|e| TabClientError::Other(format!("{e}")))?;

    let result = std::process::Command::new(tectonic_cmd)
        .arg(&tex_path)
        .output()
        .map_err(|e| TabClientError::Other(format!("执行 tectonic 失败: {e}")))?;

    let pdf_path = tex_path.with_extension("pdf");
    if pdf_path.exists() {
        std::fs::copy(&pdf_path, output)
            .map_err(|e| TabClientError::Other(format!("{e}")))?;
        let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
        let _ = std::fs::remove_dir_all(&tmp_dir);

        Ok(json!({
            "output": output.to_string_lossy(),
            "tex_length": tex_content.len(),
            "pdf_size": size,
            "status": "ok",
        }))
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr);
        Err(TabClientError::Other(format!("LaTeX 编译失败: {stderr}")))
    }
}

/// LaTeX 文件 → PDF
pub fn latex_file_to_pdf(
    input: &Path,
    output: &Path,
) -> Result<Value, TabClientError> {
    let content = std::fs::read_to_string(input)
        .map_err(|e| TabClientError::Other(format!("读取 .tex 失败: {e}")))?;
    latex_to_pdf(&content, output)
}
