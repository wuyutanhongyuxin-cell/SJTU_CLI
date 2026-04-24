//! Canvas Personal Access Token 的读 / 写 / 失效判定。
//!
//! PAT 落盘路径：`<config_dir>/sub_sessions/canvas_token.txt`（纯文本单行，无 BOM）。
//!
//! 为什么单独一个文件而不复用 `Session` struct：
//! - Canvas PAT 不是 cookie —— `Session` 的 `cookies: Vec<Cookie>` 字段硬塞不自然
//! - 隔离后 `sjtu logout` 只清主 session，不动 Canvas PAT（语义更直观）
//! - 与 `sub_sessions/<name>.json` 并排；`.gitignore` 的 `sub_sessions/` 规则同样覆盖

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config;
use crate::error::SjtuCliError;

/// PAT 文件名（定在 `sub_sessions/` 下）。
const TOKEN_FILE: &str = "canvas_token.txt";

/// 解析 PAT 文件绝对路径。
pub fn token_path() -> Result<PathBuf> {
    Ok(config::sub_sessions_dir()?.join(TOKEN_FILE))
}

/// 读 PAT。文件不存在 → `NotAuthenticated`；文件为空 / 只空白 → 同上。
pub fn load_pat() -> Result<String> {
    let path = token_path()?;
    if !path.exists() {
        return Err(SjtuCliError::NotAuthenticated.into());
    }
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("读取 {} 失败", path.display()))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(SjtuCliError::NotAuthenticated.into());
    }
    Ok(trimmed.to_string())
}

/// 写 PAT。自动 mkdir 父目录；Unix 下 chmod 600。
///
/// `pat` 先 trim 再落盘，避免误粘带前后空格 / 换行。
pub fn save_pat(pat: &str) -> Result<PathBuf> {
    let pat = pat.trim();
    if pat.is_empty() {
        return Err(SjtuCliError::InvalidInput("PAT 为空（去空格后）".into()).into());
    }
    config::ensure_dirs()?;
    let path = token_path()?;
    std::fs::write(&path, pat).with_context(|| format!("写入 {} 失败", path.display()))?;
    chmod_600(&path)?;
    Ok(path)
}

/// 清除 PAT 文件（给未来 `canvas logout` 用；当前未暴露到 CLI）。幂等。
#[allow(dead_code)]
pub fn clear_pat() -> Result<()> {
    let path = token_path()?;
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("删除 {} 失败", path.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn chmod_600(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(path)?.permissions();
    perm.set_mode(0o600);
    std::fs::set_permissions(path, perm)
        .with_context(|| format!("无法设置 {} 权限为 600", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn chmod_600(_path: &std::path::Path) -> Result<()> {
    // Windows ACL 收紧留 TODO，与 cookies::io 保持一致。
    Ok(())
}
