// toolkit/diagram — Mermaid 图生成
//
// 移植自: aily_diagram/ (182行, 7种图)
// 纯字符串操作，零依赖

pub mod dispatch;

use crate::toolkit::client::TabClientError;
use serde_json::{json, Value};

/// 图类型
#[derive(Debug, Clone, Copy)]
pub enum DiagramType {
    Flowchart,
    Sequence,
    ClassDiagram,
    Er,
    Gantt,
    Mindmap,
    State,
}

impl DiagramType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "flowchart" | "flow" => Some(Self::Flowchart),
            "sequence" | "seq" => Some(Self::Sequence),
            "class" | "classdiagram" => Some(Self::ClassDiagram),
            "er" | "erdiagram" => Some(Self::Er),
            "gantt" => Some(Self::Gantt),
            "mindmap" | "mind" => Some(Self::Mindmap),
            "state" | "statediagram" => Some(Self::State),
            _ => None,
        }
    }
}

/// 生成 Mermaid 图
pub fn create_diagram(
    diagram_type: DiagramType,
    title: &str,
    data: &Value,
) -> Result<Value, TabClientError> {
    let mermaid = match diagram_type {
        DiagramType::Flowchart => generate_flowchart(title, data)?,
        DiagramType::Sequence => generate_sequence(title, data)?,
        DiagramType::ClassDiagram => generate_class(title, data)?,
        DiagramType::Er => generate_er(title, data)?,
        DiagramType::Gantt => generate_gantt(title, data)?,
        DiagramType::Mindmap => generate_mindmap(title, data)?,
        DiagramType::State => generate_state(title, data)?,
    };

    Ok(json!({
        "mermaid": mermaid,
        "title": title,
    }))
}

fn generate_flowchart(_title: &str, data: &Value) -> Result<String, TabClientError> {
    let nodes = data
        .get("nodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| TabClientError::InvalidParam("需要 nodes".into()))?;
    let edges = data
        .get("edges")
        .and_then(|v| v.as_array())
        .unwrap_or(&Vec::new())
        .clone();
    let direction = data
        .get("direction")
        .and_then(|v| v.as_str())
        .unwrap_or("TD");

    let mut lines = vec![format!("flowchart {direction}")];
    for node in nodes {
        let id = node.get("id").and_then(|v| v.as_str()).unwrap_or("n");
        let label = node.get("label").and_then(|v| v.as_str()).unwrap_or(id);
        lines.push(format!("    {id}[\"{label}\"]"));
    }
    for edge in &edges {
        let from = edge.get("from").and_then(|v| v.as_str()).unwrap_or("");
        let to = edge.get("to").and_then(|v| v.as_str()).unwrap_or("");
        let label = edge.get("label").and_then(|v| v.as_str());
        match label {
            Some(l) => lines.push(format!("    {from} -->|{l}| {to}")),
            None => lines.push(format!("    {from} --> {to}")),
        }
    }
    Ok(lines.join("\n"))
}

fn generate_sequence(_title: &str, data: &Value) -> Result<String, TabClientError> {
    let steps = data
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or_else(|| TabClientError::InvalidParam("需要 steps".into()))?;
    let mut lines = vec!["sequenceDiagram".to_string()];
    for step in steps {
        let from = step.get("from").and_then(|v| v.as_str()).unwrap_or("A");
        let to = step.get("to").and_then(|v| v.as_str()).unwrap_or("B");
        let msg = step.get("message").and_then(|v| v.as_str()).unwrap_or("");
        lines.push(format!("    {from}->>+{to}: {msg}"));
    }
    Ok(lines.join("\n"))
}

fn generate_class(_title: &str, data: &Value) -> Result<String, TabClientError> {
    let classes = data
        .get("classes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| TabClientError::InvalidParam("需要 classes".into()))?;
    let mut lines = vec!["classDiagram".to_string()];
    for cls in classes {
        let name = cls.get("name").and_then(|v| v.as_str()).unwrap_or("Class");
        lines.push(format!("    class {name}"));
        if let Some(methods) = cls.get("methods").and_then(|v| v.as_array()) {
            for m in methods {
                let ms = m.as_str().unwrap_or("");
                lines.push(format!("    {name} : {ms}"));
            }
        }
    }
    Ok(lines.join("\n"))
}

fn generate_er(_title: &str, data: &Value) -> Result<String, TabClientError> {
    let entities = data
        .get("entities")
        .and_then(|v| v.as_array())
        .ok_or_else(|| TabClientError::InvalidParam("需要 entities".into()))?;
    let relations = data
        .get("relations")
        .and_then(|v| v.as_array())
        .unwrap_or(&Vec::new())
        .clone();
    let mut lines = vec!["erDiagram".to_string()];
    for rel in &relations {
        let from = rel.get("from").and_then(|v| v.as_str()).unwrap_or("");
        let to = rel.get("to").and_then(|v| v.as_str()).unwrap_or("");
        let label = rel.get("label").and_then(|v| v.as_str()).unwrap_or("");
        lines.push(format!("    {from} ||--o{{ {to} : \"{label}\""));
    }
    let _ = entities; // entities 已在 relations 中引用
    Ok(lines.join("\n"))
}

fn generate_gantt(title: &str, data: &Value) -> Result<String, TabClientError> {
    let tasks = data
        .get("tasks")
        .and_then(|v| v.as_array())
        .ok_or_else(|| TabClientError::InvalidParam("需要 tasks".into()))?;
    let mut lines = vec![
        "gantt".to_string(),
        format!("    title {title}"),
        "    dateFormat YYYY-MM-DD".to_string(),
    ];
    for task in tasks {
        let name = task.get("name").and_then(|v| v.as_str()).unwrap_or("Task");
        let start = task
            .get("start")
            .and_then(|v| v.as_str())
            .unwrap_or("2024-01-01");
        let duration = task
            .get("duration")
            .and_then(|v| v.as_str())
            .unwrap_or("7d");
        lines.push(format!("    {name} : {start}, {duration}"));
    }
    Ok(lines.join("\n"))
}

fn generate_mindmap(title: &str, data: &Value) -> Result<String, TabClientError> {
    let root = data.get("root").and_then(|v| v.as_str()).unwrap_or(title);
    let children = data.get("children").and_then(|v| v.as_array());
    let mut lines = vec!["mindmap".to_string(), format!("  root(({root}))")];
    if let Some(kids) = children {
        for child in kids {
            let label = child.as_str().unwrap_or("item");
            lines.push(format!("    {label}"));
        }
    }
    Ok(lines.join("\n"))
}

fn generate_state(_title: &str, data: &Value) -> Result<String, TabClientError> {
    let transitions = data
        .get("transitions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| TabClientError::InvalidParam("需要 transitions".into()))?;
    let mut lines = vec!["stateDiagram-v2".to_string()];
    for t in transitions {
        let from = t.get("from").and_then(|v| v.as_str()).unwrap_or("[*]");
        let to = t.get("to").and_then(|v| v.as_str()).unwrap_or("[*]");
        let label = t.get("label").and_then(|v| v.as_str());
        match label {
            Some(l) => lines.push(format!("    {from} --> {to} : {l}")),
            None => lines.push(format!("    {from} --> {to}")),
        }
    }
    Ok(lines.join("\n"))
}
