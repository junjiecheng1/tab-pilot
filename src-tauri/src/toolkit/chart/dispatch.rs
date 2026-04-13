// toolkit/chart/dispatch — CLI 命令分发
//
// Agent: bash("tab-chart bar --title '销售数据' --data '{...}'")

use crate::core::error::{ServiceError, ServiceResult};
use serde_json::{json, Value};

pub fn dispatch(args: &[String]) -> ServiceResult {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    // 所有图表类型都走 create_chart
    if let Some(chart_type) = super::ChartType::from_str(subcmd) {
        let title = named_arg(args, "--title").unwrap_or_else(|_| "Chart".to_string());
        let data_str = named_arg(args, "--data")?;
        let data: Value = serde_json::from_str(&data_str)
            .map_err(|e| ServiceError::bad_request(format!("JSON 解析失败: {e}")))?;

        let result = super::create_chart(chart_type, &title, &data, None)
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
            format!("tab-chart: 未知图表类型 '{subcmd}'. 支持: bar|line|area|pie|scatter|box|radar|funnel|heatmap|treemap")
        )),
    }
}

const HELP: &str = r#"tab-chart — 图表生成 (输出 JSON spec)

用法: tab-chart <type> --title <标题> --data '<JSON>'

图表类型:
  bar        柱状图
  line       折线图
  area       面积图
  pie        饼图
  scatter    散点图
  box        箱线图
  radar      雷达图
  funnel     漏斗图
  heatmap    热力图
  treemap    矩形树图

示例:
  tab-chart bar --title "销售" --data '{"x":["Q1","Q2"],"y":[100,200]}'
  tab-chart pie --title "占比" --data '{"labels":["A","B"],"values":[60,40]}'
"#;

fn named_arg(args: &[String], flag: &str) -> Result<String, ServiceError> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string())
        .ok_or_else(|| ServiceError::bad_request(format!("缺少参数 {flag}")))
}
