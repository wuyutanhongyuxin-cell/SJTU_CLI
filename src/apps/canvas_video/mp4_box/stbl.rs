//! mp4 box parser — stbl / sample table 层。Task 3 实装 stsd / stsc / stsz / stco / co64。

use super::parser::AudioTrack;

/// stbl body → AudioTrack。Task 3 实装，当前返占位错误。
pub(super) fn parse_stbl(_stbl_body: &[u8]) -> anyhow::Result<AudioTrack> {
    anyhow::bail!("parse_stbl 在 Task 3 实装")
}
