//! session 文件读 / 写 / 清除：主 session 在 `~/.sjtu-cli/session.json`，
//! 子系统 session 在 `~/.sjtu-cli/sub_sessions/<name>.json`。
//! Unix 下落盘后 chmod 600；Windows ACL 暂留 TODO。

use anyhow::{Context, Result};

use super::Session;
use crate::config;
use crate::error::SjtuCliError;

/// 读取主 session。文件不存在时返回 `NotAuthenticated`。
pub fn load_session() -> Result<Session> {
    let path = config::session_path()?;
    if !path.exists() {
        return Err(SjtuCliError::NotAuthenticated.into());
    }
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("读取 {} 失败", path.display()))?;
    let sess: Session = serde_json::from_str(&raw)
        .with_context(|| format!("解析 {} 失败（文件已损坏？）", path.display()))?;
    Ok(sess)
}

/// 保存主 session；自动 mkdir -p 父目录；Unix 下 chmod 600。
pub fn save_session(session: &Session) -> Result<()> {
    config::ensure_dirs()?;
    let path = config::session_path()?;
    let raw = serde_json::to_string_pretty(session).context("序列化 session 失败")?;
    std::fs::write(&path, raw).with_context(|| format!("写入 {} 失败", path.display()))?;
    chmod_600(&path)?;
    Ok(())
}

/// 清除主 session 文件（用于 `sjtu logout`）。幂等。
pub fn clear_session() -> Result<()> {
    let path = config::session_path()?;
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("删除 {} 失败", path.display()))?;
    }
    Ok(())
}

/// 子系统 session 路径，带路径注入防御。
pub(super) fn sub_session_path(name: &str) -> Result<std::path::PathBuf> {
    if name.is_empty() || name.contains(['/', '\\', '.', ' ']) {
        return Err(SjtuCliError::InvalidInput(format!(
            "非法子系统名 `{name}`：禁止空 / `.` / 路径分隔符 / 空格"
        ))
        .into());
    }
    Ok(config::sub_sessions_dir()?.join(format!("{name}.json")))
}

/// 读子系统 session。文件不存在返回 `NotAuthenticated`。
pub fn load_sub_session(name: &str) -> Result<Session> {
    let path = sub_session_path(name)?;
    if !path.exists() {
        return Err(SjtuCliError::NotAuthenticated.into());
    }
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("读取 {} 失败", path.display()))?;
    let sess: Session = serde_json::from_str(&raw)
        .with_context(|| format!("解析 {} 失败（文件已损坏？）", path.display()))?;
    Ok(sess)
}

/// 保存子系统 session。自动 mkdir -p 父目录；Unix 下 chmod 600。
pub fn save_sub_session(name: &str, session: &Session) -> Result<()> {
    config::ensure_dirs()?;
    let path = sub_session_path(name)?;
    let raw = serde_json::to_string_pretty(session).context("序列化 sub session 失败")?;
    std::fs::write(&path, raw).with_context(|| format!("写入 {} 失败", path.display()))?;
    chmod_600(&path)?;
    Ok(())
}

/// 清除单个子系统 session。幂等。
pub fn clear_sub_session(name: &str) -> Result<()> {
    let path = sub_session_path(name)?;
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
    // Windows 下权限收紧走 ACL，暂留 TODO；这里无操作。
    Ok(())
}
