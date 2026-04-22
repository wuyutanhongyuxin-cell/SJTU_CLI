//! 配置与路径管理：`~/.sjtu-cli/` 相关目录的解析与创建。
//!
//! 平台差异（由 `directories` crate 决定）：
//! - Linux:   `~/.config/sjtu-cli/`
//! - macOS:   `~/Library/Application Support/sjtu-cli/`
//! - Windows: `%APPDATA%\sjtu-cli\`
//!
//! 为了文档统一，README / SCHEMA 里仍用 `~/.sjtu-cli/` 这种简写。

use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;

/// 解析平台配置目录。
pub fn config_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("edu", "sjtu", "sjtu-cli")
        .context("无法解析平台配置目录（HOME / APPDATA 未设置？）")?;
    Ok(dirs.config_dir().to_path_buf())
}

/// 主 JAccount session 文件路径：`<config_dir>/session.json`。
pub fn session_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("session.json"))
}

/// 子系统 session 目录：`<config_dir>/sub_sessions/`。
pub fn sub_sessions_dir() -> Result<PathBuf> {
    Ok(config_dir()?.join("sub_sessions"))
}

/// 用户配置文件路径：`<config_dir>/config.toml`。
pub fn user_config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

/// 幂等创建所有必要目录。
pub fn ensure_dirs() -> Result<()> {
    let cfg = config_dir()?;
    std::fs::create_dir_all(&cfg).with_context(|| format!("无法创建配置目录 {}", cfg.display()))?;
    let sub = sub_sessions_dir()?;
    std::fs::create_dir_all(&sub)
        .with_context(|| format!("无法创建子系统 session 目录 {}", sub.display()))?;

    #[cfg(unix)]
    set_unix_dir_perm(&cfg)?;

    Ok(())
}

/// 把目录权限改成 700（仅当前用户可读可写可执行）。
#[cfg(unix)]
fn set_unix_dir_perm(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(path)?.permissions();
    perm.set_mode(0o700);
    std::fs::set_permissions(path, perm).context("无法设置 ~/.sjtu-cli/ 权限为 700")?;
    Ok(())
}
