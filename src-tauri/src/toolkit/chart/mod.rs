// toolkit/chart — 图表生成
//
// 移植自: aily_chart/ (510行, 10种图表)
// 所有图表生成 JSON/HTML spec，前端用 Plotly/ECharts 渲染

pub mod types;
pub mod dispatch;

use serde_json::{json, Value};
use crate::toolkit::client::TabClientError;

/// 图表类型
#[derive(Debug, Clone, Copy)]
pub enum ChartType {
    Bar,
    Line,
    Area,
    Pie,
    Scatter,
    BoxPlot,
    Radar,
    Funnel,
    Heatmap,
    Treemap,
}

impl ChartType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "bar" => Some(Self::Bar),
            "line" => Some(Self::Line),
            "area" => Some(Self::Area),
            "pie" => Some(Self::Pie),
            "scatter" => Some(Self::Scatter),
            "box" | "boxplot" | "box_plot" => Some(Self::BoxPlot),
            "radar" => Some(Self::Radar),
            "funnel" => Some(Self::Funnel),
            "heatmap" => Some(Self::Heatmap),
            "treemap" => Some(Self::Treemap),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Bar => "bar",
            Self::Line => "line",
            Self::Area => "area",
            Self::Pie => "pie",
            Self::Scatter => "scatter",
            Self::BoxPlot => "box",
            Self::Radar => "radar",
            Self::Funnel => "funnel",
            Self::Heatmap => "heatmap",
            Self::Treemap => "treemap",
        }
    }
}

/// 生成图表 spec
pub fn create_chart(
    chart_type: ChartType,
    title: &str,
    data: &Value,
    options: Option<&Value>,
) -> Result<Value, TabClientError> {
    let data_obj = data.as_object()
        .ok_or_else(|| TabClientError::InvalidParam("data must be object".into()))?;

    let x = data_obj.get("x").cloned().unwrap_or(json!([]));
    let y = data_obj.get("y").cloned().unwrap_or(json!([]));
    let labels = data_obj.get("labels").cloned().unwrap_or(json!([]));
    let values_arr = data_obj.get("values").cloned().unwrap_or(json!([]));

    let spec = match chart_type {
        ChartType::Bar => json!({
            "type": "bar",
            "data": { "labels": x, "datasets": [{ "label": title, "data": y }] },
        }),
        ChartType::Line => json!({
            "type": "line",
            "data": { "labels": x, "datasets": [{ "label": title, "data": y }] },
        }),
        ChartType::Area => json!({
            "type": "line",
            "data": { "labels": x, "datasets": [{ "label": title, "data": y, "fill": true }] },
        }),
        ChartType::Pie => json!({
            "type": "pie",
            "data": { "labels": labels, "datasets": [{ "data": values_arr }] },
        }),
        ChartType::Scatter => json!({
            "type": "scatter",
            "data": { "datasets": [{ "label": title, "data": x }] },
        }),
        ChartType::BoxPlot => json!({
            "type": "boxplot",
            "data": { "labels": labels, "datasets": [{ "label": title, "data": y }] },
        }),
        ChartType::Radar => json!({
            "type": "radar",
            "data": { "labels": labels, "datasets": [{ "label": title, "data": values_arr }] },
        }),
        ChartType::Funnel => json!({
            "type": "funnel",
            "data": { "labels": labels, "datasets": [{ "data": values_arr }] },
        }),
        ChartType::Heatmap => json!({
            "type": "heatmap",
            "data": { "x": x, "y": y, "values": values_arr },
        }),
        ChartType::Treemap => json!({
            "type": "treemap",
            "data": { "labels": labels, "parents": data_obj.get("parents").cloned().unwrap_or(json!([])), "values": values_arr },
        }),
    };

    let mut result = json!({
        "chart_type": chart_type.as_str(),
        "title": title,
        "spec": spec,
    });

    if let Some(opts) = options {
        result["options"] = opts.clone();
    }

    Ok(result)
}
