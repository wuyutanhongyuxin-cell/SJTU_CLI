//! mp4 box 最小化解析：只解析 audio-only Range 下载需要的 box（ftyp / moov / trak / stbl）。
//!
//! 不引入 mp4 / mp4parse crate（CLAUDE.md 禁自引依赖；只用 ~5% box 类型）。
//! 设计目标：parse moov 字节 → AudioTrack（含 sample 偏移/大小 + stsd 复用字节）。

mod parser;
#[cfg(test)]
mod tests;

pub use parser::{parse_moov, AudioTrack};
