// toolkit/chart/types — 图表数据类型

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub label: String,
    pub data: Vec<f64>,
    #[serde(default)]
    pub color: Option<String>,
}
