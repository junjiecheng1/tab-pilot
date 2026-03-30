// toolkit/image/dispatch — CLI 命令分发
//
// Agent: bash("tab-image watermark --input photo.jpg --output out.jpg --text '版权' --opacity 0.3")

use std::path::Path;
use serde_json::json;
use crate::core::error::{ServiceError, ServiceResult};

pub fn dispatch(args: &[String]) -> ServiceResult {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    match subcmd {
        "watermark" => {
            let input = named_arg(args, "--input")?;
            let output = named_arg(args, "--output")?;
            let text = named_arg(args, "--text").unwrap_or_else(|_| "WATERMARK".to_string());
            let opacity: f32 = named_arg(args, "--opacity")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.3);
            let font_size: f32 = named_arg(args, "--font-size")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(24.0);
            let angle: f32 = named_arg(args, "--angle")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(45.0);

            let result = super::add_watermark(
                Path::new(&input),
                Path::new(&output),
                &text,
                opacity,
                font_size,
                angle,
            ).map_err(|e| ServiceError::internal(format!("{e}")))?;

            let out = serde_json::to_string(&result).unwrap_or_default();
            Ok(json!({"output": out, "exit_code": 0}))
        }
        "help" | "--help" | "-h" => Ok(json!({
            "output": HELP,
            "exit_code": 0,
        })),
        _ => Err(ServiceError::bad_request(
            format!("tab-image: 未知命令 '{subcmd}'")
        )),
    }
}

const HELP: &str = r#"tab-image — 图片处理

用法: tab-image <command> [options]

命令:
  watermark  添加水印  --input <in> --output <out> --text <文字> [--opacity 0.3] [--font-size 24] [--angle 45]

示例:
  tab-image watermark --input photo.jpg --output out.jpg --text "版权所有" --opacity 0.3
"#;

fn named_arg(args: &[String], flag: &str) -> Result<String, ServiceError> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string())
        .ok_or_else(|| ServiceError::bad_request(format!("缺少参数 {flag}")))
}
