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

/// 统一信封。`data` 和 `error` 互斥：成功只填 data，失败只填 error。
#[derive(Debug, Clone, Serialize)]
pub struct Envelope<T: Serialize> {
    pub ok: bool,
    pub schema_version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<EnvelopeError>,
}

impl<T: Serialize> Envelope<T> {
    /// 成功信封。
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            schema_version: SCHEMA_VERSION,
            data: Some(data),
            error: None,
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
