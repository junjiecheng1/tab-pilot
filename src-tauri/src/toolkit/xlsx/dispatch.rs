// toolkit/xlsx/dispatch — CLI 命令分发
//
// Agent: bash("tab-xlsx inspect file.xlsx --pretty")
//        bash("tab-xlsx inspect https://example.com/data.xlsx")

use serde_json::{json, Value};
use crate::core::error::{ServiceError, ServiceResult};
use crate::toolkit::download::resolve_input;

/// 分发 tab-xlsx 子命令 (async: 支持 URL 下载)
pub async fn dispatch(args: &[String]) -> ServiceResult {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    match subcmd {
        "inspect" => {
            let input = file_arg(args, "inspect")?;
            let local = resolve_input(&input, "xlsx").await?;
            let result = super::inspect::inspect_file(local.path())
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            local.cleanup();
            wrap(result, has_flag(args, "--pretty"))
        }
        "recheck" => {
            let input = file_arg(args, "recheck")?;
            let local = resolve_input(&input, "xlsx").await?;
            let result = super::recheck::recheck_file(local.path())
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            local.cleanup();
            wrap(result, false)
        }
        "reference-check" | "refcheck" => {
            let input = file_arg(args, "reference-check")?;
            let local = resolve_input(&input, "xlsx").await?;
            let result = super::reference::check_references(local.path())
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            local.cleanup();
            wrap(result, false)
        }
        "validate" => {
            let input = file_arg(args, "validate")?;
            let local = resolve_input(&input, "xlsx").await?;
            let result = super::validate::validate_file(local.path())
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            local.cleanup();
            wrap(result, false)
        }
        "pivot" => {
            let input = file_arg(args, "pivot")?;
            let local = resolve_input(&input, "xlsx").await?;
            let sheet = named_opt(args, "--sheet");
            let row_field = named_arg(args, "--row")?;
            let col_field = named_arg(args, "--col")?;
            let value_field = named_arg(args, "--value")?;
            let agg = named_opt(args, "--agg").unwrap_or_else(|| "sum".to_string());

            let result = super::pivot::create_pivot(
                local.path(), sheet.as_deref(),
                &row_field, &col_field, &value_field, &agg,
            ).map_err(|e| ServiceError::internal(format!("{e}")))?;
            local.cleanup();
            wrap(result, has_flag(args, "--pretty"))
        }
        "help" | "--help" | "-h" => Ok(json!({
            "output": HELP,
            "exit_code": 0,
        })),
        _ => Err(ServiceError::bad_request(
            format!("tab-xlsx: 未知命令 '{subcmd}'. 运行 'tab-xlsx help' 查看帮助.")
        )),
    }
}

const HELP: &str = r#"tab-xlsx — Excel 分析验证工具

用法: tab-xlsx <command> <file|url> [options]

命令:
  inspect           分析 Excel 文件结构 (支持本地路径或 URL)
  recheck           检测公式错误
  reference-check   检测引用错误和模式异常
  validate          OpenXML 结构验证
  pivot             创建数据透视表

选项:
  --pretty          格式化 JSON 输出

pivot 选项:
  --row <field>     行分组字段 (必需)
  --col <field>     列分组字段 (必需)
  --value <field>   值字段 (必需)
  --agg <method>    聚合方式: sum|avg|count|min|max (默认 sum)
  --sheet <name>    指定工作表

示例:
  tab-xlsx inspect data.xlsx --pretty
  tab-xlsx inspect https://example.com/report.xlsx
  tab-xlsx recheck output.xlsx
  tab-xlsx pivot data.xlsx --row 部门 --col 月份 --value 销售额 --agg sum
"#;

fn file_arg(args: &[String], cmd: &str) -> Result<String, ServiceError> {
    args.iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(|s| s.to_string())
        .ok_or_else(|| ServiceError::bad_request(format!("tab-xlsx {cmd}: 缺少文件路径或 URL")))
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn named_arg(args: &[String], flag: &str) -> Result<String, ServiceError> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string())
        .ok_or_else(|| ServiceError::bad_request(format!("缺少参数 {flag}")))
}

fn named_opt(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string())
}

fn wrap(data: Value, pretty: bool) -> ServiceResult {
    let output = if pretty {
        serde_json::to_string_pretty(&data)
    } else {
        serde_json::to_string(&data)
    }.unwrap_or_default();
    Ok(json!({"output": output, "exit_code": 0}))
}
