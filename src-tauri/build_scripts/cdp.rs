// build/cdp.rs — CDP Protocol Codegen
//
// 读取 cdp-protocol/{browser,js}_protocol.json
// → 生成 $OUT_DIR/cdp_generated.rs
// → engine/cdp/types.rs 中 include! 引用

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::Path;

pub fn generate() {
    let protocol_dir = Path::new("cdp-protocol");
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("cdp_generated.rs");

    let browser_path = protocol_dir.join("browser_protocol.json");
    let js_path = protocol_dir.join("js_protocol.json");

    if !browser_path.exists() && !js_path.exists() {
        fs::write(&out_path, "// No protocol JSON files found\n").unwrap();
        return;
    }

    let mut all_domains: Vec<Domain> = Vec::new();

    for path in [&browser_path, &js_path] {
        if !path.exists() {
            continue;
        }
        println!("cargo:rerun-if-changed={}", path.display());
        let content = fs::read_to_string(path).unwrap();
        let protocol: ProtocolSpec = match serde_json::from_str(&content) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("cargo:warning=Failed to parse {}: {}", path.display(), e);
                continue;
            }
        };
        all_domains.extend(protocol.domains);
    }

    // 收集每个 domain 的类型 ID (跨 domain $ref 解析)
    let mut domain_types: HashMap<String, HashSet<String>> = HashMap::new();
    for domain in &all_domains {
        let mut types = HashSet::new();
        for td in &domain.types {
            types.insert(td.id.clone());
        }
        domain_types.insert(domain.domain.clone(), types);
    }

    // 递归结构体字段需 Box 包装
    let recursive_fields: HashSet<(&str, &str, &str)> = [
        ("DOM", "Node", "contentDocument"),
        ("DOM", "Node", "templateContent"),
        ("DOM", "Node", "importedDocument"),
        ("Accessibility", "AXNode", "sources"),
        ("Runtime", "StackTrace", "parent"),
    ]
    .into_iter()
    .collect();

    let mut output = String::new();
    output.push_str("use serde::{Deserialize, Serialize};\n\n");

    for domain in &all_domains {
        emit_domain(domain, &domain_types, &recursive_fields, &mut output);
    }

    fs::write(&out_path, &output).unwrap();
}

// ── JSON schema ──────────────────────────────

#[allow(dead_code)]
#[derive(serde::Deserialize)]
struct ProtocolSpec {
    domains: Vec<Domain>,
}

#[allow(dead_code)]
#[derive(serde::Deserialize, Clone)]
struct Domain {
    domain: String,
    #[serde(default)]
    types: Vec<TypeDef>,
    #[serde(default)]
    commands: Vec<Command>,
    #[serde(default)]
    events: Vec<Event>,
}

#[allow(dead_code)]
#[derive(serde::Deserialize, Clone)]
struct TypeDef {
    id: String,
    #[serde(rename = "type", default)]
    type_kind: String,
    #[serde(default)]
    properties: Vec<Property>,
    #[serde(rename = "enum", default)]
    enum_values: Vec<String>,
    #[serde(default)]
    description: Option<String>,
}

#[allow(dead_code)]
#[derive(serde::Deserialize, Clone)]
struct Command {
    name: String,
    #[serde(default)]
    parameters: Vec<Property>,
    #[serde(default)]
    returns: Vec<Property>,
    #[serde(default)]
    description: Option<String>,
}

#[allow(dead_code)]
#[derive(serde::Deserialize, Clone)]
struct Event {
    name: String,
    #[serde(default)]
    parameters: Vec<Property>,
    #[serde(default)]
    description: Option<String>,
}

#[allow(dead_code)]
#[derive(serde::Deserialize, Clone)]
struct Property {
    name: String,
    #[serde(rename = "type", default)]
    type_kind: Option<String>,
    #[serde(rename = "$ref", default)]
    ref_type: Option<String>,
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    items: Option<Box<ItemType>>,
    #[serde(rename = "enum", default)]
    enum_values: Vec<String>,
}

#[allow(dead_code)]
#[derive(serde::Deserialize, Clone)]
struct ItemType {
    #[serde(rename = "type", default)]
    type_kind: Option<String>,
    #[serde(rename = "$ref", default)]
    ref_type: Option<String>,
}

// ── 代码生成辅助 ──────────────────────────────

fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize = true;
    for c in s.chars() {
        if c == '_' || c == '-' || c == '.' {
            capitalize = true;
        } else if capitalize {
            result.push(c.to_ascii_uppercase());
            capitalize = false;
        } else {
            result.push(c);
        }
    }
    result
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() && i > 0 {
            let prev_upper = chars[i - 1].is_uppercase();
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if !prev_upper || next_lower {
                result.push('_');
            }
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}

fn resolve_ref(
    r: &str,
    current_domain: &str,
    domain_types: &HashMap<String, HashSet<String>>,
) -> String {
    let parts: Vec<&str> = r.split('.').collect();
    if parts.len() == 2 {
        let (ref_domain, ref_type) = (parts[0], parts[1]);
        if ref_domain == current_domain {
            to_pascal_case(ref_type)
        } else if domain_types
            .get(ref_domain)
            .is_some_and(|t| t.contains(ref_type))
        {
            format!(
                "super::cdp_{}::{}",
                to_snake_case(ref_domain),
                to_pascal_case(ref_type)
            )
        } else {
            "serde_json::Value".to_string()
        }
    } else {
        to_pascal_case(r)
    }
}

fn map_type(
    prop: &Property,
    domain: &str,
    domain_types: &HashMap<String, HashSet<String>>,
) -> String {
    if let Some(ref r) = prop.ref_type {
        let t = resolve_ref(r, domain, domain_types);
        return if prop.optional {
            format!("Option<{t}>")
        } else {
            t
        };
    }
    if let Some(ref t) = prop.type_kind {
        let base = match t.as_str() {
            "string" => "String".to_string(),
            "integer" => "i64".to_string(),
            "number" => "f64".to_string(),
            "boolean" => "bool".to_string(),
            "object" | "any" => "serde_json::Value".to_string(),
            "array" => {
                let inner = prop
                    .items
                    .as_ref()
                    .map_or("serde_json::Value".to_string(), |items| {
                        if let Some(ref r) = items.ref_type {
                            resolve_ref(r, domain, domain_types)
                        } else {
                            match items.type_kind.as_deref().unwrap_or("any") {
                                "string" => "String".to_string(),
                                "integer" => "i64".to_string(),
                                "number" => "f64".to_string(),
                                "boolean" => "bool".to_string(),
                                _ => "serde_json::Value".to_string(),
                            }
                        }
                    });
                format!("Vec<{inner}>")
            }
            _ => "serde_json::Value".to_string(),
        };
        return if prop.optional {
            format!("Option<{base}>")
        } else {
            base
        };
    }
    if prop.optional {
        "Option<serde_json::Value>".to_string()
    } else {
        "serde_json::Value".to_string()
    }
}

fn is_rust_keyword(s: &str) -> bool {
    matches!(
        s,
        "type"
            | "self"
            | "Self"
            | "super"
            | "move"
            | "ref"
            | "fn"
            | "mod"
            | "use"
            | "pub"
            | "let"
            | "mut"
            | "const"
            | "static"
            | "if"
            | "else"
            | "for"
            | "while"
            | "loop"
            | "match"
            | "return"
            | "break"
            | "continue"
            | "as"
            | "in"
            | "impl"
            | "trait"
            | "struct"
            | "enum"
            | "where"
            | "async"
            | "await"
            | "dyn"
            | "box"
            | "yield"
            | "override"
            | "crate"
            | "extern"
    )
}

fn field_name(name: &str) -> String {
    let snake = to_snake_case(name);
    if is_rust_keyword(&snake) {
        format!("r#{snake}")
    } else {
        snake
    }
}

// ── Domain codegen ──────────────────────────────

fn emit_domain(
    domain: &Domain,
    domain_types: &HashMap<String, HashSet<String>>,
    recursive_fields: &HashSet<(&str, &str, &str)>,
    out: &mut String,
) {
    let mod_name = to_snake_case(&domain.domain);
    out.push_str(&format!(
        "#[allow(dead_code, non_snake_case, non_camel_case_types, clippy::enum_variant_names)]\npub mod cdp_{mod_name} {{\n    use super::*;\n\n"
    ));

    for td in &domain.types {
        emit_type_def(td, &domain.domain, domain_types, recursive_fields, out);
    }
    for cmd in &domain.commands {
        let pascal = to_pascal_case(&cmd.name);
        if !cmd.parameters.is_empty() {
            emit_struct(
                &format!("{pascal}Params"),
                &cmd.parameters,
                &domain.domain,
                domain_types,
                None,
                out,
            );
        }
        if !cmd.returns.is_empty() {
            emit_struct(
                &format!("{pascal}Result"),
                &cmd.returns,
                &domain.domain,
                domain_types,
                None,
                out,
            );
        }
    }
    for ev in &domain.events {
        if !ev.parameters.is_empty() {
            let pascal = to_pascal_case(&ev.name);
            emit_struct(
                &format!("{pascal}Event"),
                &ev.parameters,
                &domain.domain,
                domain_types,
                None,
                out,
            );
        }
    }

    out.push_str("}\n\n");
}

fn emit_type_def(
    td: &TypeDef,
    domain: &str,
    domain_types: &HashMap<String, HashSet<String>>,
    recursive_fields: &HashSet<(&str, &str, &str)>,
    out: &mut String,
) {
    if !td.enum_values.is_empty() {
        let mut seen = HashSet::new();
        out.push_str("    #[derive(Debug, Clone, Serialize, Deserialize)]\n");
        out.push_str(&format!("    pub enum {} {{\n", td.id));
        for val in &td.enum_values {
            let mut variant = to_pascal_case(val);
            if variant == "Self" {
                variant = "SelfValue".to_string();
            }
            if variant.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                variant = format!("V{variant}");
            }
            if seen.insert(variant.clone()) {
                out.push_str(&format!(
                    "        #[serde(rename = \"{val}\")]\n        {variant},\n"
                ));
            }
        }
        out.push_str("    }\n\n");
    } else if td.type_kind == "object" && !td.properties.is_empty() {
        emit_struct(
            &td.id,
            &td.properties,
            domain,
            domain_types,
            Some(recursive_fields),
            out,
        );
    } else if td.type_kind == "object" {
        out.push_str(&format!("    pub type {} = serde_json::Value;\n\n", td.id));
    } else if td.type_kind == "array" {
        out.push_str(&format!(
            "    pub type {} = Vec<serde_json::Value>;\n\n",
            td.id
        ));
    } else if td.type_kind == "string" {
        out.push_str(&format!("    pub type {} = String;\n\n", td.id));
    } else if td.type_kind == "integer" {
        out.push_str(&format!("    pub type {} = i64;\n\n", td.id));
    } else if td.type_kind == "number" {
        out.push_str(&format!("    pub type {} = f64;\n\n", td.id));
    }
}

fn emit_struct(
    name: &str,
    props: &[Property],
    domain: &str,
    domain_types: &HashMap<String, HashSet<String>>,
    recursive_fields: Option<&HashSet<(&str, &str, &str)>>,
    out: &mut String,
) {
    out.push_str("    #[derive(Debug, Clone, Serialize, Deserialize)]\n    #[serde(rename_all = \"camelCase\")]\n");
    out.push_str(&format!("    pub struct {name} {{\n"));
    for prop in props {
        let fname = field_name(&prop.name);
        let mut rust_type = map_type(prop, domain, domain_types);

        if let Some(rf) = recursive_fields {
            if rf.contains(&(domain, name, prop.name.as_str())) {
                if rust_type.starts_with("Option<") {
                    let inner = &rust_type[7..rust_type.len() - 1];
                    rust_type = format!("Option<Box<{inner}>>");
                } else {
                    rust_type = format!("Box<{rust_type}>");
                }
            }
        }

        if prop.optional {
            out.push_str("        #[serde(skip_serializing_if = \"Option::is_none\")]\n");
        }
        out.push_str(&format!("        pub {fname}: {rust_type},\n"));
    }
    out.push_str("    }\n\n");
}
