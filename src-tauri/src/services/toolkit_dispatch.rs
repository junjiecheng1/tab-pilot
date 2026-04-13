// Toolkit Builtin Dispatcher — 纯路由层
//
// Shell 内置命令拦截: 当 Agent 通过 bash 调用 tab-* 命令时,
// 路由到对应 toolkit 模块的 dispatch 函数。
//
// 全部 10 个模块:
//   本地: tab-xlsx, tab-pdf, tab-chart, tab-diagram, tab-image
//   飞书: tab-base, tab-doc, tab-im, tab-drive, tab-skillhub
//
// 日志由 shell.rs exec 入口统一记录, 此处纯路由不记日志。

use crate::core::error::{ServiceError, ServiceResult};
use crate::toolkit::client::auth::TokenProvider;

/// 尝试作为 toolkit 内置命令执行
///
/// 返回 Some(result) → 命令已拦截处理
/// 返回 None → 不是 toolkit 命令, 走 PTY
pub async fn try_dispatch(
    command: &str,
    token_provider: Option<&TokenProvider>,
) -> Option<ServiceResult> {
    let args = parse_args(command);
    if args.is_empty() {
        return None;
    }

    let program = args[0].as_str();
    let sub_args = &args[1..];

    match program {
        // ── 本地工具 (纯 Rust, 不需要网络) ──────────
        "tab-xlsx" => Some(crate::toolkit::xlsx::dispatch::dispatch(sub_args).await),
        "tab-pdf" => Some(crate::toolkit::pdf::dispatch::dispatch(sub_args).await),
        "tab-chart" => Some(crate::toolkit::chart::dispatch::dispatch(sub_args)),
        "tab-diagram" => Some(crate::toolkit::diagram::dispatch::dispatch(sub_args)),
        "tab-image" => Some(crate::toolkit::image::dispatch::dispatch(sub_args)),

        // ── 飞书 API 工具 (需要 Token + 网络) ───────
        "tab-base" | "tab-doc" | "tab-im" | "tab-drive" | "tab-skillhub" => {
            let client = match get_client(token_provider).await {
                Ok(c) => c,
                Err(e) => return Some(Err(e)),
            };
            let result = match program {
                "tab-base" => crate::toolkit::base::dispatch::dispatch(sub_args, &client).await,
                "tab-doc" => crate::toolkit::doc::dispatch::dispatch(sub_args, &client).await,
                "tab-im" => crate::toolkit::im::dispatch::dispatch(sub_args, &client).await,
                "tab-drive" => crate::toolkit::drive::dispatch::dispatch(sub_args, &client).await,
                "tab-skillhub" => {
                    crate::toolkit::skillhub::dispatch::dispatch(sub_args, &client).await
                }
                _ => unreachable!(),
            };
            Some(result)
        }

        _ => None, // 不是 toolkit 命令, 走 PTY
    }
}

/// 从 TokenProvider 获取 TabClient
async fn get_client(
    token_provider: Option<&TokenProvider>,
) -> Result<crate::toolkit::client::TabClient, ServiceError> {
    let provider = token_provider
        .ok_or_else(|| ServiceError::internal("飞书功能不可用: 未登录或 TokenProvider 未初始化"))?;
    provider
        .create_client()
        .await
        .map_err(|e| ServiceError::internal(format!("获取飞书 Token 失败: {e}")))
}

/// 简易参数解析 (shell-like, 支持引号)
fn parse_args(command: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = ' ';

    for c in command.chars() {
        match c {
            '"' | '\'' if !in_quote => {
                in_quote = true;
                quote_char = c;
            }
            c if c == quote_char && in_quote => {
                in_quote = false;
            }
            ' ' | '\t' if !in_quote => {
                if !current.is_empty() {
                    args.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}
