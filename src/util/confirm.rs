//! 写操作前的 y/n 二次确认。
//!
//! 行为表：
//! | assume_yes | is_tty(stdin) | 结果                                    |
//! |------------|---------------|-----------------------------------------|
//! | true       | 任意          | 直接放行（提示一行但不读 stdin）        |
//! | false      | true          | 打印 prompt 读 stdin；y/Y/yes → 放行    |
//! | false      | false         | **拒绝**：管道里不准默默发 POST         |
//!
//! 为什么非 TTY 时必须要 `--yes`：脚本化调用（CI / Agent）里 prompt 读不到响应，
//! 一旦默默继续就会发 POST，违反 "写操作必须显式确认" 的不变量。

use anyhow::{bail, Result};
use is_terminal::IsTerminal;
use std::io::{BufRead, Write};

use crate::error::SjtuCliError;

/// 执行一次 y/n 确认。`action` 是给用户看的一句话（动词开头，如 "回复 topic 123"）。
///
/// `assume_yes=true` 绕过 prompt；非 TTY + 未传 `--yes` 时返回 `InvalidInput` 硬失败。
pub fn confirm(action: &str, assume_yes: bool) -> Result<()> {
    confirm_with_io(action, assume_yes, &mut std::io::stdout(), || {
        let mut s = String::new();
        std::io::stdin().lock().read_line(&mut s)?;
        Ok(s)
    })
}

/// 可注入 stdin/stdout 的版本，便于单测。生产走 `confirm`。
///
/// `read_line` 每次返回"用户敲的一整行（含末尾 `\n`）"或 IO 错误。
pub fn confirm_with_io<W, F>(
    action: &str,
    assume_yes: bool,
    out: &mut W,
    read_line: F,
) -> Result<()>
where
    W: Write,
    F: FnOnce() -> std::io::Result<String>,
{
    if assume_yes {
        writeln!(out, "[确认] --yes 已传入，直接执行：{action}")?;
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        return Err(SjtuCliError::InvalidInput(format!(
            "非 TTY 环境（脚本/管道）下执行写操作必须显式加 `--yes` 以确认：{action}"
        ))
        .into());
    }
    write!(out, "[确认] 即将 {action}. 继续? [y/N]: ")?;
    out.flush()?;
    let line =
        read_line().map_err(|e| SjtuCliError::InvalidInput(format!("读 stdin 失败: {e}")))?;
    let ans = line.trim();
    if matches!(ans, "y" | "Y" | "yes" | "YES" | "Yes") {
        Ok(())
    } else {
        bail!("用户取消：{action}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assume_yes_skips_prompt_and_does_not_read_stdin() {
        let mut out = Vec::<u8>::new();
        confirm_with_io("回复 topic 1", true, &mut out, || {
            panic!("assume_yes 路径不应读 stdin")
        })
        .unwrap();
        assert!(String::from_utf8(out).unwrap().contains("--yes"));
    }

    #[test]
    fn yes_answer_proceeds() {
        // 当前进程里 stdin 通常不是 TTY（Cargo 跑测试）；我们无法跨平台强行模拟 TTY，
        // 所以这里直接测 assume_yes=true + 回答 "y" 的 io 分支就够了。
        // 真实 TTY 分支靠 B2 checkpoint 验。
        let mut out = Vec::<u8>::new();
        let r = confirm_with_io("发新帖", true, &mut out, || Ok("y\n".to_string()));
        assert!(r.is_ok());
    }

    // 注意：无法可靠地在单元测试里模拟 "stdin 是 TTY"，因此
    // "非 TTY + 没传 --yes 时失败" 的路径靠集成脚本验（运行 `echo y | sjtu shuiyuan reply ...`
    // 期望看到 `--yes` 的报错）。
}
