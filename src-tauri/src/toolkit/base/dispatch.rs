// toolkit/base/dispatch — Bitable CLI 命令分发
//
// Agent: bash("tab-base export --app bascnXXX --table tblYYY")

use crate::core::error::{ServiceError, ServiceResult};
use crate::toolkit::client::TabClient;
use serde_json::{json, Value};

pub async fn dispatch(args: &[String], client: &TabClient) -> ServiceResult {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    match subcmd {
        "create" => {
            // create_table(client, fields, rows, app_token, app_name, table_name, folder_token)
            let fields_str = named_arg(args, "--fields")?;
            let fields: Vec<Value> = serde_json::from_str(&fields_str)
                .map_err(|e| ServiceError::bad_request(format!("--fields JSON 解析失败: {e}")))?;
            let rows_str = named_arg(args, "--rows").ok();
            let rows: Option<Vec<Value>> = rows_str.and_then(|s| serde_json::from_str(&s).ok());
            let app_token = named_arg(args, "--app").ok();
            let app_name = named_arg(args, "--app-name").ok();
            let table_name = named_arg(args, "--table-name").ok();
            let folder_token = named_arg(args, "--folder").ok();

            let result = super::create::create_table(
                client,
                &fields,
                rows.as_deref(),
                app_token.as_deref(),
                app_name.as_deref(),
                table_name.as_deref(),
                folder_token.as_deref(),
            )
            .await
            .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "delete-rows" => {
            let app = named_arg(args, "--app")?;
            let table = named_arg(args, "--table")?;
            let key_field = named_arg(args, "--key")?;
            let values_str = named_arg(args, "--values")?;
            let values: Vec<String> = serde_json::from_str(&values_str)
                .map_err(|e| ServiceError::bad_request(format!("--values JSON 解析失败: {e}")))?;

            let result = super::delete::delete_rows(client, &app, &table, &key_field, &values)
                .await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "delete-field" => {
            let app = named_arg(args, "--app")?;
            let table = named_arg(args, "--table")?;
            let field = named_arg(args, "--field")?;

            let result = super::delete::delete_field(client, &app, &table, &field)
                .await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "export" => {
            let app = named_arg(args, "--app")?;
            let table = named_arg(args, "--table")?;
            let include_id = has_flag(args, "--include-id");

            let result = super::export::export_table(client, &app, &table, include_id)
                .await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "export-all" => {
            let app = named_arg(args, "--app")?;
            let tables_str = named_arg(args, "--tables").ok();
            let tables: Option<Vec<String>> =
                tables_str.and_then(|s| serde_json::from_str(&s).ok());
            let include_id = has_flag(args, "--include-id");

            let result = super::export::export_tables(client, &app, tables.as_deref(), include_id)
                .await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "upload" => {
            let files_str = named_arg(args, "--files")?;
            let files: Vec<String> = serde_json::from_str(&files_str)
                .map_err(|e| ServiceError::bad_request(format!("--files JSON 解析失败: {e}")))?;
            let parent_type =
                named_arg(args, "--parent-type").unwrap_or_else(|_| "explorer".to_string());
            let parent_node = named_arg(args, "--parent-node")?;

            let result = super::file_ops::upload_files(client, &files, &parent_type, &parent_node)
                .await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "download" => {
            let tokens_str = named_arg(args, "--tokens")?;
            let tokens: Vec<String> = serde_json::from_str(&tokens_str)
                .map_err(|e| ServiceError::bad_request(format!("--tokens JSON 解析失败: {e}")))?;
            let dir = named_arg(args, "--output").unwrap_or_else(|_| "./downloads".to_string());

            let result = super::file_ops::download_files(client, &tokens, &dir)
                .await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "parse-url" => {
            let url = args
                .get(1)
                .ok_or_else(|| ServiceError::bad_request("tab-base parse-url: 缺少 URL"))?;
            let parsed = super::parser::parse_bitable_url(url);
            wrap(serde_json::to_value(parsed).unwrap_or_default())
        }
        "csv-to-json" => {
            let content = named_arg(args, "--input")?;
            let csv_content = std::fs::read_to_string(&content)
                .map_err(|e| ServiceError::internal(format!("读取文件失败: {e}")))?;
            let result = super::parser::csv_to_json(&csv_content);
            wrap(result)
        }
        "help" | "--help" | "-h" => Ok(json!({
            "output": HELP,
            "exit_code": 0,
        })),
        _ => Err(ServiceError::bad_request(format!(
            "tab-base: 未知命令 '{subcmd}'"
        ))),
    }
}

const HELP: &str = r#"tab-base — Bitable 数据表操作

命令:
  create       创建数据表       --fields '<JSON>' [--rows '<JSON>'] [--app token] [--app-name name]
  delete-rows  删除行           --app <token> --table <id> --key <field> --values '<JSON>'
  delete-field 删除字段         --app <token> --table <id> --field <name>
  export       导出单表         --app <token> --table <id> [--include-id]
  export-all   导出全部表       --app <token> [--tables '<JSON>'] [--include-id]
  upload       上传文件         --files '<JSON>' --parent-node <token> [--parent-type explorer]
  download     下载文件         --tokens '<JSON>' [--output ./downloads]
  parse-url    解析飞书链接     <url>
  csv-to-json  CSV 转 JSON     --input <file>

示例:
  tab-base export --app bascnXXX --table tblYYY
  tab-base parse-url "https://xxx.feishu.cn/base/bascnXXX"
"#;

fn named_arg(args: &[String], flag: &str) -> Result<String, ServiceError> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string())
        .ok_or_else(|| ServiceError::bad_request(format!("缺少参数 {flag}")))
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn wrap(data: Value) -> ServiceResult {
    let output = serde_json::to_string(&data).unwrap_or_default();
    Ok(json!({"output": output, "exit_code": 0}))
}
