// build/env.rs — 编译时嵌入 env 变量
//
// release 读 .env.production, debug 读 .env
// 确保 PILOT_SERVER 等变量编译进二进制

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

pub fn embed() {
    let profile = env::var("PROFILE").unwrap_or_default();
    let env_file = if profile == "release" {
        "../.env.production"
    } else {
        "../.env"
    };

    let vars = load_env_file(env_file)
        .or_else(|| load_env_file("../.env"))
        .unwrap_or_default();

    for (key, value) in &vars {
        let final_value = env::var(key).unwrap_or_else(|_| value.clone());
        println!("cargo:rustc-env={key}={final_value}");
    }

    println!("cargo:rerun-if-changed=../.env");
    println!("cargo:rerun-if-changed=../.env.production");
    println!("cargo:rerun-if-env-changed=PILOT_SERVER");
    println!("cargo:rerun-if-env-changed=PILOT_DEBUG");
}

fn load_env_file(path: &str) -> Option<HashMap<String, String>> {
    let content = fs::read_to_string(Path::new(path)).ok()?;
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    Some(map)
}
