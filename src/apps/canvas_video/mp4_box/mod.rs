//! mp4 box 最小化解析：只解析 audio-only Range 下载需要的 box（ftyp / moov / trak / stbl）。
//!
//! 不引入 mp4 / mp4parse crate（CLAUDE.md 禁自引依赖；只用 ~5% box 类型）。
//! 设计目标：parse moov 字节 → AudioTrack（含 sample 偏移/大小 + stsd 复用字节）。

mod boxes;
mod parser;
mod stbl;
#[cfg(test)]
mod tests;
mod trak;

pub use parser::{parse_moov, AudioTrack};

/// 测试 helper：从整个 mp4 文件字节里切出 moov box 字节（顺序扫第一个 moov）。
/// 仅供 mp4_box / m4a_mux / audio_dl 单测共享。
#[cfg(test)]
pub(crate) fn extract_moov_bytes_for_test(mp4: &[u8]) -> Vec<u8> {
    let mut pos = 0usize;
    while pos + 8 <= mp4.len() {
        let size = u32::from_be_bytes(mp4[pos..pos + 4].try_into().unwrap()) as usize;
        if &mp4[pos + 4..pos + 8] == b"moov" {
            return mp4[pos..pos + size].to_vec();
        }
        if size == 0 {
            break;
        }
        pos += size;
    }
    panic!("没找到 moov box");
}
