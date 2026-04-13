// toolkit/image — 图片水印
//
// 移植自: aily_generate_image/watermark.py (204行)
// 依赖: image crate

pub mod dispatch;

use crate::toolkit::client::TabClientError;
use serde_json::{json, Value};
use std::path::Path;

/// 添加文字水印（简单像素绘制，不依赖外部字体）
pub fn add_watermark(
    input: &Path,
    output: &Path,
    text: &str,
    opacity: f32,
    _font_size: f32,
    _angle: f32,
) -> Result<Value, TabClientError> {
    use image::{GenericImageView, Rgba};

    let img =
        image::open(input).map_err(|e| TabClientError::Other(format!("无法打开图片: {e}")))?;

    let (width, height) = img.dimensions();
    let mut overlay = img.to_rgba8();

    let alpha = (opacity * 255.0) as u8;
    let color = Rgba([128, 128, 128, alpha]);

    // 简单水印: 在固定间距绘制小标记线
    // 完整文字渲染需要字体文件，这里用pattern标记
    let text_bytes = text.as_bytes();
    let step_x = (width as f32 * 0.25) as u32;
    let step_y = (height as f32 * 0.2) as u32;

    let mut y = 10u32;
    while y < height.saturating_sub(20) {
        let mut x = 10u32;
        while x < width.saturating_sub(200) {
            // 绘制一行点阵标记（每个字符用3x5像素块表示）
            for (ci, _ch) in text_bytes.iter().enumerate().take(20) {
                let cx = x + (ci as u32) * 5;
                if cx + 3 < width {
                    for dy in 0..3u32 {
                        for dx in 0..2u32 {
                            let px = cx + dx;
                            let py = y + dy;
                            if px < width && py < height {
                                overlay.put_pixel(px, py, color);
                            }
                        }
                    }
                }
            }
            x += step_x;
        }
        y += step_y;
    }

    overlay
        .save(output)
        .map_err(|e| TabClientError::Other(format!("保存失败: {e}")))?;

    Ok(json!({
        "output": output.to_string_lossy(),
        "width": width,
        "height": height,
        "watermark_text": text,
        "opacity": opacity,
    }))
}
