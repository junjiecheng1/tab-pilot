// toolkit/drive/dispatch — 云空间操作 CLI 命令分发
//
// Agent: bash("tab-drive upload --files '[\"report.pdf\"]' --parent-node token")
// 底层复用 base::file_ops

use serde_json::{json, Value};
use crate::core::error::{ServiceError, ServiceResult};
use crate::toolkit::client::TabClient;

pub async fn dispatch(args: &[String], client: &TabClient) -> ServiceResult {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    match subcmd {
        "upload" => {
            // upload_files(client, file_paths, parent_type, parent_node)
            let files_str = named_arg(args, "--files")?;
            let files: Vec<String> = serde_json::from_str(&files_str)
                .map_err(|e| ServiceError::bad_request(format!("--files JSON 解析失败: {e}")))?;
            let parent_type = named_arg(args, "--parent-type").unwrap_or_else(|_| "explorer".to_string());
            let parent_node = named_arg(args, "--parent-node")?;

            let result = super::upload_files(client, &files, &parent_type, &parent_node)
                .await.map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "download" => {
            // download_files(client, file_tokens, dir_path)
            let tokens_str = named_arg(args, "--tokens")?;
            let tokens: Vec<String> = serde_json::from_str(&tokens_str)
                .map_err(|e| ServiceError::bad_request(format!("--tokens JSON 解析失败: {e}")))?;
            let dir = named_arg(args, "--output").unwrap_or_else(|_| "./downloads".to_string());

            let result = super::download_files(client, &tokens, &dir)
                .await.map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "help" | "--help" | "-h" => Ok(json!({
            "output": HELP,
            "exit_code": 0,
        })),
        _ => Err(ServiceError::bad_request(format!("tab-drive: 未知命令 '{subcmd}'"))),
    }
}

const HELP: &str = r#"tab-drive — 云空间操作

命令:
  upload     上传文件     --files '<JSON>' --parent-node <token> [--parent-type explorer]
  download   下载文件     --tokens '<JSON>' [--output ./downloads]

示例:
  tab-drive upload --files '["report.pdf","data.xlsx"]' --parent-node fldcnXXX
  tab-drive download --tokens '["boxcnAAA","boxcnBBB"]' --output ./out
"#;

fn named_arg(args: &[String], flag: &str) -> Result<String, ServiceError> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string())
        .ok_or_else(|| ServiceError::bad_request(format!("缺少参数 {flag}")))
}

fn wrap(data: Value) -> ServiceResult {
    let output = serde_json::to_string(&data).unwrap_or_default();
    Ok(json!({"output": output, "exit_code": 0}))
}
