// toolkit/pdf/dispatch — CLI 命令分发
//
// Agent: bash("tab-pdf extract document.pdf")
//        bash("tab-pdf extract https://example.com/file.pdf")

use crate::core::error::{ServiceError, ServiceResult};
use crate::toolkit::download::resolve_input;
use serde_json::{json, Value};

/// 分发 tab-pdf 子命令 (async: extract 支持 URL 下载)
pub async fn dispatch(args: &[String]) -> ServiceResult {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    match subcmd {
        "extract" => {
            let input = file_arg(args, "extract")?;
            let local = resolve_input(&input, "pdf").await?;
            let result = super::extract::extract_text(local.path())
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            local.cleanup();
            wrap(result)
        }
        "meta" => {
            let input = file_arg(args, "meta")?;
            let local = resolve_input(&input, "pdf").await?;
            let result = super::meta::read_metadata(local.path())
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            local.cleanup();
            wrap(result)
        }
        "form" => {
            let input = file_arg(args, "form")?;
            let local = resolve_input(&input, "pdf").await?;
            let result = super::form::read_form(local.path())
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            local.cleanup();
            wrap(result)
        }
        "pages" => Err(ServiceError::bad_request("tab-pdf pages: 参数解析待实现")),
        "convert" => Err(ServiceError::bad_request("tab-pdf convert: 参数解析待实现")),
        "latex" => Err(ServiceError::bad_request("tab-pdf latex: 参数解析待实现")),
        "html" => Err(ServiceError::bad_request("tab-pdf html: 参数解析待实现")),
        "help" | "--help" | "-h" => Ok(json!({
            "output": HELP,
            "exit_code": 0,
        })),
        _ => Err(ServiceError::bad_request(format!(
            "tab-pdf: 未知命令 '{subcmd}'"
        ))),
    }
}

const HELP: &str = r#"tab-pdf — PDF 处理工具

用法: tab-pdf <command> <file|url> [options]

命令:
  extract    提取文本内容 (支持本地路径或 URL)
  meta       获取/设置元数据
  form       读取/填写表单
  pages      拆分/合并/旋转页面
  convert    格式转换
  latex      LaTeX → PDF
  html       HTML → PDF

示例:
  tab-pdf extract document.pdf
  tab-pdf extract https://example.com/report.pdf
  tab-pdf meta document.pdf
"#;

fn file_arg(args: &[String], cmd: &str) -> Result<String, ServiceError> {
    args.iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(|s| s.to_string())
        .ok_or_else(|| ServiceError::bad_request(format!("tab-pdf {cmd}: 缺少文件路径或 URL")))
}

fn wrap(data: Value) -> ServiceResult {
    let output = serde_json::to_string(&data).unwrap_or_default();
    Ok(json!({"output": output, "exit_code": 0}))
}
