// toolkit/diagram/dispatch — CLI 命令分发
//
// Agent: bash("tab-diagram flowchart --data '{...}'")

use crate::core::error::{ServiceError, ServiceResult};
use serde_json::{json, Value};

pub fn dispatch(args: &[String]) -> ServiceResult {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    if let Some(diagram_type) = super::DiagramType::from_str(subcmd) {
        let title = named_arg(args, "--title").unwrap_or_else(|_| "Diagram".to_string());
        let data_str = named_arg(args, "--data")?;
        let data: Value = serde_json::from_str(&data_str)
            .map_err(|e| ServiceError::bad_request(format!("JSON 解析失败: {e}")))?;

        let result = super::create_diagram(diagram_type, &title, &data)
            .map_err(|e| ServiceError::internal(format!("{e}")))?;
        let output = serde_json::to_string(&result).unwrap_or_default();
        return Ok(json!({"output": output, "exit_code": 0}));
    }

    match subcmd {
        "help" | "--help" | "-h" => Ok(json!({
            "output": HELP,
            "exit_code": 0,
        })),
        _ => Err(ServiceError::bad_request(
            format!("tab-diagram: 未知类型 '{subcmd}'. 支持: flowchart|sequence|class|er|gantt|mindmap|state")
        )),
    }
}

const HELP: &str = r#"tab-diagram — Mermaid 图生成

用法: tab-diagram <type> --title <标题> --data '<JSON>'

类型:
  flowchart  流程图
  sequence   时序图
  class      类图
  er         ER 图
  gantt      甘特图
  mindmap    思维导图
  state      状态图

示例:
  tab-diagram flowchart --data '{"nodes":[{"id":"A","label":"开始"}],"edges":[{"from":"A","to":"B"}]}'
  tab-diagram gantt --title "项目" --data '{"tasks":[{"name":"设计","start":"2024-01-01","duration":"7d"}]}'
"#;

fn named_arg(args: &[String], flag: &str) -> Result<String, ServiceError> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string())
        .ok_or_else(|| ServiceError::bad_request(format!("缺少参数 {flag}")))
}
