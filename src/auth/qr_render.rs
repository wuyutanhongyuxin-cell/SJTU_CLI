//! 从截屏 PNG 里解码 QR，然后用 `qrcode` 重绘成终端 ANSI 半块。
//!
//! 单独成文件是为了隔离 `image` + `rqrr` + `qrcode` 三个依赖的使用面。

use anyhow::{anyhow, Result};
use qrcode::render::unicode;
use qrcode::QrCode;

/// 从 PNG 字节流里找第一个 QR 码并解码出字符串 payload。
pub fn decode_qr_from_png(png: &[u8]) -> Result<String> {
    let img = image::load_from_memory(png)
        .map_err(|e| anyhow!("解码 PNG 失败: {e}"))?
        .to_luma8();

    let mut prepared = rqrr::PreparedImage::prepare(img);
    let grids = prepared.detect_grids();

    for grid in grids {
        if let Ok((_meta, content)) = grid.decode() {
            if !content.is_empty() {
                return Ok(content);
            }
        }
    }
    Err(anyhow!("截屏中未能识别到 QR"))
}

/// 把字符串 payload 当作 QR 内容重绘到 stdout，使用 Unicode 半块色块。
pub fn render_ansi_to_stdout(payload: &str) -> Result<()> {
    let code = QrCode::new(payload.as_bytes()).map_err(|e| anyhow!("生成 QR 失败: {e}"))?;
    let rendered = code
        .render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Dark)
        .light_color(unicode::Dense1x2::Light)
        .quiet_zone(true)
        .build();
    println!();
    println!("{rendered}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_ascii_qr_does_not_panic_on_simple_payload() {
        render_ansi_to_stdout("https://example.com").unwrap();
    }

    #[test]
    fn decode_rejects_empty_bytes() {
        assert!(decode_qr_from_png(&[]).is_err());
    }
}
