// toolkit/skillhub/dispatch — CLI 命令分发
//
// Agent: bash("tab-skillhub explore")

use serde_json::json;
use crate::core::error::{ServiceError, ServiceResult};
use crate::toolkit::client::TabClient;

pub async fn dispatch(args: &[String], client: &TabClient) -> ServiceResult {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    match subcmd {
        "explore" => {
            let result = super::explore(client).await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "search" => {
            let query = named_arg(args, "--query")?;
            let page_size: i32 = named_arg(args, "--limit")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(20);
            let result = super::search(client, &query, page_size).await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "list" | "installed" => {
            let result = super::list_installed(client).await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "inspect" => {
            let skill_id = named_arg(args, "--id")?;
            let result = super::inspect(client, &skill_id).await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "install" => {
            let skill_id = named_arg(args, "--id")?;
            let result = super::install(client, &skill_id).await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "uninstall" => {
            let skill_id = named_arg(args, "--id")?;
            let result = super::uninstall(client, &skill_id).await
                .map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "help" | "--help" | "-h" => Ok(json!({
            "output": HELP,
            "exit_code": 0,
        })),
        _ => Err(ServiceError::bad_request(format!("tab-skillhub: 未知命令 '{subcmd}'"))),
    }
}

const HELP: &str = r#"tab-skillhub — 技能市场

命令:
  explore     浏览技能市场
  search      搜索技能       --query <关键词> [--limit 20]
  list        已安装技能
  inspect     技能详情       --id <skill_id>
  install     安装技能       --id <skill_id>
  uninstall   卸载技能       --id <skill_id>

示例:
  tab-skillhub explore
  tab-skillhub search --query "数据分析"
  tab-skillhub install --id skill_xxx
"#;

fn named_arg(args: &[String], flag: &str) -> Result<String, ServiceError> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string())
        .ok_or_else(|| ServiceError::bad_request(format!("缺少参数 {flag}")))
}

fn wrap(data: serde_json::Value) -> ServiceResult {
    let output = serde_json::to_string(&data).unwrap_or_default();
    Ok(json!({"output": output, "exit_code": 0}))
}
