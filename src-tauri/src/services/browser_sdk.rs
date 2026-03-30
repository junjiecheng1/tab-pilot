// BrowserSDK 服务 — CDP 页面级自动化
//
// 对应 Python app/services/browser_sdk.py
// 通过 WS JSON-RPC 分发: app/browser_sdk.<action>
//
// 设计: 委托给 service::browser::BrowserService (→ engine)

use std::net::IpAddr;

use serde_json::{json, Value};

use super::browser::BrowserService;
use crate::core::error::{ServiceError, ServiceResult};

/// BrowserSDK 服务 — 高级页面自动化 API
pub struct BrowserSdkService;

impl BrowserSdkService {
    pub fn new() -> Self {
        Self
    }

    /// 统一 WS handler — 将 SDK action 映射为 engine command
    pub async fn handle(
        &self,
        action: &str,
        params: Value,
        browser: &BrowserService,
    ) -> ServiceResult {
        let engine_cmd = self.map_action(action, &params)?;
        browser.execute(engine_cmd).await
    }

    /// SDK action → engine command 映射
    fn map_action(&self, action: &str, params: &Value) -> Result<Value, ServiceError> {
        let cmd = match action {
            // ── 导航 ────────────────────────────────
            "navigate" => {
                let url = params["url"].as_str().unwrap_or_default();
                let url = Self::normalize_url(url);
                json!({"action": "navigate", "url": url})
            }
            "go_back" => json!({"action": "back"}),
            "go_forward" => json!({"action": "forward"}),
            "reload" => json!({"action": "reload"}),

            // ── 交互 ────────────────────────────────
            "click" => {
                let mut cmd = json!({"action": "click"});
                if let Some(s) = params["selector"].as_str() {
                    cmd["selector"] = json!(s);
                } else if let Some(idx) = params["index"].as_u64() {
                    cmd["selector"] = json!(format!("[ref={}]", idx));
                }
                if let Some(btn) = params["button"].as_str() {
                    cmd["button"] = json!(btn);
                }
                if let Some(count) = params["click_count"].as_i64() {
                    cmd["clickCount"] = json!(count);
                }
                if let (Some(x), Some(y)) = (params["x"].as_f64(), params["y"].as_f64()) {
                    cmd["x"] = json!(x);
                    cmd["y"] = json!(y);
                }
                cmd
            }
            "fill" => {
                let mut cmd = json!({"action": "fill"});
                if let Some(s) = params["selector"].as_str() {
                    cmd["selector"] = json!(s);
                } else if let Some(idx) = params["index"].as_u64() {
                    cmd["selector"] = json!(format!("[ref={}]", idx));
                }
                cmd["value"] = params.get("text").cloned().unwrap_or(json!(""));
                cmd
            }
            "type_text" => {
                let mut cmd = json!({"action": "type"});
                cmd["text"] = params.get("text").cloned().unwrap_or(json!(""));
                cmd["selector"] = params.get("selector").cloned().unwrap_or(json!("body"));
                if let Some(delay) = params["delay"].as_f64() {
                    cmd["delay"] = json!(delay as u64);
                }
                cmd
            }
            "press_key" => {
                let key = params["key"].as_str().unwrap_or_default();
                json!({"action": "press", "key": key})
            }
            "hot_key" => {
                let keys: Vec<&str> = params["keys"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                json!({"action": "press", "key": keys.join("+")})
            }
            "hover" => {
                let mut cmd = json!({"action": "hover"});
                if let Some(s) = params["selector"].as_str() {
                    cmd["selector"] = json!(s);
                }
                cmd
            }
            "select_option" => {
                let selector = req_str(params, "selector")?;
                let value = params["value"].as_str()
                    .or(params["label"].as_str())
                    .unwrap_or_default();
                json!({"action": "select", "selector": selector, "value": value})
            }
            "check" => {
                let selector = req_str(params, "selector")?;
                json!({"action": "check", "selector": selector})
            }
            "uncheck" => {
                let selector = req_str(params, "selector")?;
                json!({"action": "uncheck", "selector": selector})
            }

            // ── 滚动 ────────────────────────────────
            "scroll" => {
                let direction = params["direction"].as_str().unwrap_or("down");
                let amount = params["amount"].as_i64().unwrap_or(300);
                let (dx, dy) = match direction {
                    "up" => (0.0, -(amount as f64)),
                    "down" => (0.0, amount as f64),
                    "left" => (-(amount as f64), 0.0),
                    "right" => (amount as f64, 0.0),
                    _ => (0.0, amount as f64),
                };
                json!({"action": "scroll", "delta_x": dx, "delta_y": dy})
            }
            "scroll_to" => {
                let x = params["x"].as_i64().unwrap_or(0);
                let y = params["y"].as_i64().unwrap_or(0);
                json!({"action": "evaluate", "expression": format!("window.scrollTo({x},{y})")})
            }
            "scroll_to_element" => {
                let selector = req_str(params, "selector")?;
                json!({"action": "evaluate", "expression":
                    format!("document.querySelector('{}')?.scrollIntoView({{behavior:'smooth',block:'center'}})",
                        selector.replace('\'', "\\'"))
                })
            }

            // ── 内容 ────────────────────────────────
            "screenshot" => json!({"action": "screenshot"}),
            "get_html" => {
                let outer = params["outer"].as_bool().unwrap_or(false);
                let expr = if outer { "document.documentElement.outerHTML" } else { "document.documentElement.innerHTML" };
                json!({"action": "evaluate", "expression": expr})
            }
            "get_text" => json!({"action": "gettext"}),
            "get_markdown" => {
                json!({"action": "evaluate", "expression":
                    "(function(){return JSON.stringify({title:document.title,markdown:document.body?.innerText||''})})()"
                })
            }
            "evaluate" => {
                let expr = req_str(params, "expression")?;
                json!({"action": "evaluate", "expression": expr})
            }
            "get_interactive_elements" => json!({"action": "snapshot"}),
            "find_text" => {
                let kw = req_str(params, "keyword")?;
                json!({"action": "evaluate", "expression":
                    format!("(function(){{const t=document.body?.innerText||'';return JSON.stringify({{found:t.includes('{kw}'),count:(t.match(new RegExp('{kw}','gi'))||[]).length}})}})()",
                        kw=kw.replace('\'', "\\'"))
                })
            }
            "get_console_logs" => {
                json!({"action": "evaluate", "expression":
                    "JSON.stringify(window.__tabpilot_console_logs||[])"
                })
            }

            // ── 等待 ────────────────────────────────
            "wait_for_selector" => {
                let sel = req_str(params, "selector")?;
                let state = params["state"].as_str().unwrap_or("visible");
                let tms = timeout_ms(params);
                json!({"action": "evaluate", "expression":
                    format!("(async()=>{{const s=Date.now();while(Date.now()-s<{tms}){{const e=document.querySelector('{sel}');if(e&&('{state}'==='attached'||e.offsetParent!==null))return JSON.stringify({{found:true}});await new Promise(r=>setTimeout(r,200))}}return JSON.stringify({{found:false,timeout:true}})}})()",
                        sel=sel.replace('\'', "\\'"), state=state)
                })
            }
            "wait_for_load" => {
                let tms = timeout_ms(params);
                json!({"action": "evaluate", "expression":
                    format!("(async()=>{{if(document.readyState==='complete')return JSON.stringify({{loaded:true}});await new Promise(r=>{{if(document.readyState==='complete')r();else window.addEventListener('load',r,{{once:true}});setTimeout(r,{tms})}});return JSON.stringify({{loaded:document.readyState==='complete'}})}})()")
                })
            }
            "wait_for_url" => {
                let url = req_str(params, "url")?;
                let tms = timeout_ms(params);
                json!({"action": "evaluate", "expression":
                    format!("(async()=>{{const s=Date.now();while(Date.now()-s<{tms}){{if(location.href.includes('{url}'))return JSON.stringify({{matched:true,url:location.href}});await new Promise(r=>setTimeout(r,200))}}return JSON.stringify({{matched:false,timeout:true}})}})()",
                        url=url.replace('\'', "\\'"))
                })
            }
            "wait_for_network_idle" | "wait_for_download"
            | "wait_for_response" | "wait_for_request" => {
                let tms = timeout_ms(params);
                json!({"action": "wait", "ms": tms})
            }
            "wait_for_function" => {
                let expr = req_str(params, "expression")?;
                let tms = timeout_ms(params);
                json!({"action": "evaluate", "expression":
                    format!("(async()=>{{const s=Date.now();while(Date.now()-s<{tms}){{try{{if(eval('{}'))return JSON.stringify({{result:true}})}}catch(e){{}}await new Promise(r=>setTimeout(r,200))}}return JSON.stringify({{result:false,timeout:true}})}})()",
                        expr.replace('\'', "\\'"))
                })
            }

            // ── 页面管理 ────────────────────────────
            "list_pages" => json!({"action": "tab_list"}),
            "create_page" => {
                let mut cmd = json!({"action": "tab_new"});
                if let Some(u) = params["url"].as_str() { cmd["url"] = json!(u); }
                cmd
            }
            "close_page" => {
                let mut cmd = json!({"action": "tab_close"});
                if let Some(idx) = params["index"].as_u64() { cmd["index"] = json!(idx); }
                cmd
            }
            "activate_tab" => {
                let idx = params["index"].as_u64()
                    .ok_or_else(|| ServiceError::bad_request("缺少 index"))?;
                json!({"action": "tab_switch", "index": idx})
            }

            // ── Cookies ─────────────────────────────
            "get_cookies" => {
                json!({"action": "evaluate", "expression":
                    "JSON.stringify(document.cookie.split('; ').map(c=>{const[k,...v]=c.split('=');return{name:k,value:v.join('=')}}))"
                })
            }
            "set_cookies" => {
                let cookies = params.get("cookies").and_then(|v| v.as_array());
                let exprs: Vec<String> = cookies
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|c| {
                        let n = c["name"].as_str()?;
                        let v = c["value"].as_str().unwrap_or("");
                        Some(format!("document.cookie='{n}={v}'"))
                    })
                    .collect();
                json!({"action": "evaluate", "expression": exprs.join(";")})
            }
            "clear_cookies" => {
                json!({"action": "evaluate", "expression":
                    "document.cookie.split(';').forEach(c=>{document.cookie=c.trim().split('=')[0]+'=;expires=Thu,01 Jan 1970 00:00:00 GMT;path=/'})"
                })
            }

            // ── 网络 ────────────────────────────────
            "set_extra_headers" | "set_scoped_headers" | "add_route" | "remove_route" => {
                json!({"action": "evaluate", "expression": "'not_implemented'"})
            }
            "get_requests" => {
                let limit = params["limit"].as_u64().unwrap_or(100);
                json!({"action": "evaluate", "expression":
                    format!("JSON.stringify((window.__tabpilot_requests||[]).slice(-{limit}))")
                })
            }

            // ── CAPTCHA ─────────────────────────────
            "detect_captcha" => {
                json!({"action": "evaluate", "expression":
                    "(function(){const s=['iframe[src*=\"captcha\"]','iframe[src*=\"recaptcha\"]','.g-recaptcha','#captcha','[class*=\"captcha\"]'];return JSON.stringify({detected:s.some(x=>document.querySelector(x)!==null)})})()"
                })
            }
            "wait_for_captcha_resolution" => {
                let tms = (params["timeout"].as_f64().unwrap_or(60.0) * 1000.0) as u64;
                let pms = (params["poll_interval"].as_f64().unwrap_or(2.0) * 1000.0) as u64;
                json!({"action": "evaluate", "expression":
                    format!("(async()=>{{const s=Date.now();while(Date.now()-s<{tms}){{const found=['iframe[src*=\"captcha\"]','.g-recaptcha','#captcha'].some(x=>document.querySelector(x)!==null);if(!found)return JSON.stringify({{resolved:true}});await new Promise(r=>setTimeout(r,{pms}))}}return JSON.stringify({{resolved:false,timeout:true}})}})()")
                })
            }

            // ── 导出 ────────────────────────────────
            "export_har" | "export_console_logs" => {
                json!({"action": "evaluate", "expression": "'export_not_implemented'"})
            }

            // ── 状态 ────────────────────────────────
            "save_state" | "load_state" => {
                json!({"action": "evaluate", "expression": "'state_not_implemented'"})
            }

            // ── 生命周期 ─────────────────────────────
            "restart" => json!({"action": "launch"}),
            "close" => json!({"action": "close"}),

            // ── Agent 状态 ──────────────────────────
            "agent_start" => json!({"action": "agent_start"}),
            "agent_stop" => json!({"action": "agent_stop"}),

            // ── 信息 ────────────────────────────────
            "info" => json!({"action": "info"}),
            "get_url" => json!({"action": "url"}),
            "get_title" => json!({"action": "title"}),

            _ => return Err(ServiceError::bad_request(format!("未知 browser_sdk 操作: {action}"))),
        };
        Ok(cmd)
    }

    // ── URL 规范化 ──────────────────────────────

    pub fn normalize_url(url: &str) -> String {
        let normalized = url.trim();
        if normalized.is_empty() { return normalized.to_string(); }
        let non_hier = ["about:", "data:", "file:", "mailto:", "javascript:", "blob:"];
        if normalized.contains("://") || non_hier.iter().any(|s| normalized.starts_with(s)) {
            return normalized.to_string();
        }
        if normalized.starts_with("//") {
            return format!("https:{normalized}");
        }
        let scheme = if Self::is_local(normalized) { "http://" } else { "https://" };
        format!("{scheme}{normalized}")
    }

    fn is_local(host: &str) -> bool {
        let hostname = host.split(':').next().unwrap_or(host)
            .split('/').next().unwrap_or(host).to_lowercase();
        if hostname == "localhost" { return true; }
        hostname.parse::<IpAddr>().map_or(false, |ip| ip.is_loopback())
    }
}

fn req_str(params: &Value, key: &str) -> Result<String, ServiceError> {
    params[key].as_str().map(String::from)
        .ok_or_else(|| ServiceError::bad_request(format!("缺少 {key} 参数")))
}

fn timeout_ms(params: &Value) -> u64 {
    (params["timeout"].as_f64().unwrap_or(30.0) * 1000.0) as u64
}
