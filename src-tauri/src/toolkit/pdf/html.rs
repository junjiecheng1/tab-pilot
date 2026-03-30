// toolkit/pdf/html — HTML → PDF (via CDP)
//
// 通过 engine/cdp 模块，使用 Page.printToPDF 将 HTML 渲染为 PDF
// 需要浏览器引擎已启动 (Chrome/LightPanda)

use std::path::Path;
use serde_json::{json, Value};
use crate::toolkit::client::TabClientError;
use crate::engine::cdp::client::CdpClient;

/// HTML → PDF (通过 CDP Page.printToPDF)
///
/// html_source: HTML 内容字符串或文件路径
/// output: PDF 输出路径
/// options: 打印选项
pub async fn html_to_pdf(
    html_source: &str,
    output: &Path,
    options: Option<&HtmlToPdfOptions>,
) -> Result<Value, TabClientError> {
    let opts = options.cloned().unwrap_or_default();

    // 1. 写临时 HTML 文件
    let tmp_dir = std::env::temp_dir().join("tab-html2pdf");
    tokio::fs::create_dir_all(&tmp_dir).await
        .map_err(|e| TabClientError::Other(format!("创建临时目录失败: {e}")))?;

    let html_path = if Path::new(html_source).exists() {
        Path::new(html_source).to_path_buf()
    } else {
        let p = tmp_dir.join("input.html");
        tokio::fs::write(&p, html_source.as_bytes()).await
            .map_err(|e| TabClientError::Other(format!("写入 HTML 失败: {e}")))?;
        p
    };

    let file_url = format!("file://{}", html_path.to_string_lossy());

    // 2. 构造 CDP printToPDF 参数
    let print_params = json!({
        "landscape": opts.landscape,
        "printBackground": opts.print_background,
        "scale": opts.scale,
        "paperWidth": opts.paper_width,
        "paperHeight": opts.paper_height,
        "marginTop": opts.margin_top,
        "marginBottom": opts.margin_bottom,
        "marginLeft": opts.margin_left,
        "marginRight": opts.margin_right,
        "transferMode": "ReturnAsBase64",
    });

    // 3. 连接 CDP
    let cdp_url = std::env::var("CDP_WS_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:9222".to_string());

    let client = CdpClient::connect(&cdp_url).await
        .map_err(|e| TabClientError::Other(format!("CDP 连接失败: {e}")))?;

    // 导航到 HTML 页面
    let _nav = client.send_command(
        "Page.navigate",
        Some(json!({"url": file_url})),
        None,
    ).await.map_err(|e| TabClientError::Other(format!("导航失败: {e}")))?;

    // 等待页面加载
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // 打印为 PDF
    let pdf_result: Value = client.send_command(
        "Page.printToPDF",
        Some(print_params),
        None,
    ).await.map_err(|e| TabClientError::Other(format!("打印 PDF 失败: {e}")))?;

    // 4. 解码 base64 并写入文件
    let base64_data = pdf_result.get("data")
        .and_then(|v: &Value| v.as_str())
        .ok_or_else(|| TabClientError::Other("CDP 未返回 PDF 数据".into()))?;

    use base64::Engine;
    let pdf_bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| TabClientError::Other(format!("base64 解码失败: {e}")))?;

    tokio::fs::write(output, &pdf_bytes).await
        .map_err(|e| TabClientError::Other(format!("写入 PDF 失败: {e}")))?;

    // 清理
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    Ok(json!({
        "output": output.to_string_lossy(),
        "html_length": html_source.len(),
        "pdf_size": pdf_bytes.len(),
        "method": "cdp",
        "status": "ok",
    }))
}

/// HTML → PDF 选项
#[derive(Debug, Clone)]
pub struct HtmlToPdfOptions {
    pub landscape: bool,
    pub print_background: bool,
    pub scale: f64,
    pub paper_width: f64,
    pub paper_height: f64,
    pub margin_top: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
    pub margin_right: f64,
}

impl Default for HtmlToPdfOptions {
    fn default() -> Self {
        Self {
            landscape: false,
            print_background: true,
            scale: 1.0,
            paper_width: 8.5,
            paper_height: 11.0,
            margin_top: 0.4,
            margin_bottom: 0.4,
            margin_left: 0.4,
            margin_right: 0.4,
        }
    }
}
