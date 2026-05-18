//! 统一输出信封（Envelope）+ 格式派发（YAML / JSON / Table）。
//!
//! 设计：
//! - 所有子命令返回同一形状（`ok` / `schema_version` / `data` / `error`），给 AI Agent 消费。
//! - 默认格式按 TTY 检测：TTY → Table（给人），非 TTY → YAML（给脚本 / Agent）。
//! - 可被 `--yaml` / `--json` 显式覆盖。

use anyhow::Result;
use is_terminal::IsTerminal;
use serde::Serialize;

/// 当前 Envelope schema 版本。字段变更时 bump。
pub const SCHEMA_VERSION: &str = "1";

/// 输出格式枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Yaml,
    Json,
    Table,
}

/// 失败信封里的 error 字段。
#[derive(Debug, Clone, Serialize)]
pub struct EnvelopeError {
    pub code: String,
    pub message: String,
}

/// 信封元数据。当前承载本次响应的"路径感知"信息（多路径子系统如 card 双轨）。
/// 字段全 Option + skip_serializing_if，None → JSON 中不出现，后向兼容现有子命令。
#[derive(Debug, Clone, Serialize, Default)]
pub struct EnvelopeMeta {
    /// 实际走的路径名（如 "oauth2" / "weixin"）。Agent / 用户感知用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// 数据源域名提示（debug 用，如 "api.sjtu.edu.cn" / "card.sjtu.edu.cn"）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hint: Option<String>,
}

/// 统一信封。`data` 和 `error` 互斥：成功只填 data，失败只填 error。
#[derive(Debug, Clone, Serialize)]
pub struct Envelope<T: Serialize> {
    pub ok: bool,
    pub schema_version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<EnvelopeError>,
    /// 元数据（如 `via` / `source_hint`）。None 时 JSON 输出不出现，后向兼容。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<EnvelopeMeta>,
}

impl<T: Serialize> Envelope<T> {
    /// 成功信封。
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            schema_version: SCHEMA_VERSION,
            data: Some(data),
            error: None,
            meta: None,
        }
    }

    /// 成功信封 + 元数据。card 子命令双轨切换时用。
    pub fn ok_with_meta(data: T, meta: EnvelopeMeta) -> Self {
        Self {
            ok: true,
            schema_version: SCHEMA_VERSION,
            data: Some(data),
            error: None,
            meta: Some(meta),
        }
    }

    /// 失败信封。用法示例：`Envelope::<()>::err("session_expired", "...")`。
    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            schema_version: SCHEMA_VERSION,
            data: None,
            error: Some(EnvelopeError {
                code: code.into(),
                message: message.into(),
            }),
            meta: None,
        }
    }
}

/// 决定最终输出格式：显式指定 > TTY 检测。
pub fn resolve_format(explicit: Option<OutputFormat>) -> OutputFormat {
    if let Some(f) = explicit {
        return f;
    }
    if std::io::stdout().is_terminal() {
        OutputFormat::Table
    } else {
        OutputFormat::Yaml
    }
}

/// 渲染 Envelope 到 stdout。
pub fn render<T: Serialize>(env: Envelope<T>, explicit: Option<OutputFormat>) -> Result<()> {
    let fmt = resolve_format(explicit);
    match fmt {
        OutputFormat::Yaml => {
            let s = serde_yml::to_string(&env)?;
            print!("{s}");
        }
        OutputFormat::Json => {
            let s = serde_json::to_string_pretty(&env)?;
            println!("{s}");
        }
        OutputFormat::Table => {
            // S0：表格未接入 comfy-table，先退回 YAML（人眼也能看）。
            // S3 真正有结构化数据时，替换为 comfy-table 渲染。
            let s = serde_yml::to_string(&env)?;
            print!("{s}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// meta=None 时 JSON 输出不包含 meta 键（后向兼容现有子命令）
    #[test]
    fn envelope_no_meta_serializes_without_meta_key() {
        #[derive(serde::Serialize)]
        struct D {
            v: i32,
        }
        let e = Envelope::ok(D { v: 1 });
        let s = serde_json::to_string(&e).unwrap();
        assert!(
            !s.contains("\"meta\""),
            "无 meta 时 JSON 不应出现 meta 键: {s}"
        );
    }

    /// meta=Some 时 JSON 输出含 via + source_hint
    #[test]
    fn envelope_with_meta_serializes_via_and_hint() {
        #[derive(serde::Serialize)]
        struct D {
            v: i32,
        }
        let e = Envelope::ok_with_meta(
            D { v: 1 },
            EnvelopeMeta {
                via: Some("weixin".into()),
                source_hint: Some("card.sjtu.edu.cn".into()),
            },
        );
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains("\"via\":\"weixin\""), "应含 via: {s}");
        assert!(
            s.contains("\"source_hint\":\"card.sjtu.edu.cn\""),
            "应含 source_hint: {s}"
        );
    }

    /// EnvelopeMeta 两字段都 None 时 JSON 输出空对象
    #[test]
    fn envelope_meta_all_none_serializes_empty_object() {
        let m = EnvelopeMeta {
            via: None,
            source_hint: None,
        };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(s, "{}");
    }
}
