// toolkit/doc/dispatch — 文档操作 CLI 命令分发
//
// Agent: bash("tab-doc info --doc doccnXXX")

use crate::core::error::{ServiceError, ServiceResult};
use crate::toolkit::client::TabClient;
use serde_json::{json, Value};

pub async fn dispatch(args: &[String], client: &TabClient) -> ServiceResult {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    match subcmd {
        "list" => {
            let doc_id = named_arg(args, "--doc")?;
            let page_size: i32 = named_arg(args, "--limit")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50);
            let result = super::info::list_doc(client, &doc_id, page_size)
                .await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "info" => {
            // get_doc_info(client, doc_id)
            let doc_id = named_arg(args, "--doc")?;
            let result = super::info::get_doc_info(client, &doc_id)
                .await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "search" => {
            // search_docs(client, query, owner_ids)
            let query = named_arg(args, "--query")?;
            let owner_str = named_arg(args, "--owners").ok();
            let owners: Option<Vec<String>> = owner_str.and_then(|s| serde_json::from_str(&s).ok());
            let result = super::info::search_docs(client, &query, owners.as_deref())
                .await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "batch-info" => {
            // batch_get_info(client, urls)
            let urls_str = named_arg(args, "--urls")?;
            let urls: Vec<String> = serde_json::from_str(&urls_str)
                .map_err(|e| ServiceError::bad_request(format!("--urls JSON 解析失败: {e}")))?;
            let result = super::info::batch_get_info(client, &urls)
                .await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "parse-urls" => {
            // parse_urls(client, urls)
            let urls_str = named_arg(args, "--urls")?;
            let urls: Vec<String> = serde_json::from_str(&urls_str)
                .map_err(|e| ServiceError::bad_request(format!("--urls JSON 解析失败: {e}")))?;
            let result = super::info::parse_urls(client, &urls)
                .await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "comments" => {
            // get_comments(client, file_token, file_type, resolve_users)
            let token = named_arg(args, "--token")?;
            let file_type = named_arg(args, "--type").unwrap_or_else(|_| "doc".to_string());
            let resolve = has_flag(args, "--resolve-users");
            let result = super::comments::get_comments(client, &token, &file_type, resolve)
                .await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "add-comment" => {
            // add_comment(client, file_token, file_type, content)
            let token = named_arg(args, "--token")?;
            let file_type = named_arg(args, "--type").unwrap_or_else(|_| "doc".to_string());
            let content = named_arg(args, "--content")?;
            let result = super::comments::add_comment(client, &token, &file_type, &content)
                .await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "help" | "--help" | "-h" => Ok(json!({
            "output": HELP,
            "exit_code": 0,
        })),
        _ => Err(ServiceError::bad_request(format!(
            "tab-doc: 未知命令 '{subcmd}'"
        ))),
    }
}

const HELP: &str = r#"tab-doc — 文档操作

命令:
  info          获取文档内容     --doc <doc_id>
  search        搜索文档         --query <关键词> [--owners '<JSON>']
  batch-info    批量获取信息     --urls '<JSON>'
  parse-urls    解析文档链接     --urls '<JSON>'
  comments      获取评论         --token <file_token> [--type doc] [--resolve-users]
  add-comment   添加评论         --token <file_token> [--type doc] --content <文字>

示例:
  tab-doc info --doc doccnXXX
  tab-doc search --query "周报"
  tab-doc comments --token doccnXXX --resolve-users
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
