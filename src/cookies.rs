//! Cookie 持久化：主 JAccount session 的读 / 写 / TTL / 脱敏。

use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::error::SjtuCliError;

/// 单条 cookie：name / value + 最基本的元信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub expires: Option<DateTime<Utc>>,
}

/// JAccount 主 session：一组 cookie + 抓取时间 + 软性过期时间。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub cookies: Vec<Cookie>,
    pub captured_at: DateTime<Utc>,
    /// 软性 TTL。S0 固定 30 天；S1 拿到真实 `JAAuthCookie` 的 expires 后可覆盖。
    pub soft_expires_at: DateTime<Utc>,
}

impl Session {
    /// 用当前时间 + 30 天 TTL 构造。
    pub fn new(cookies: Vec<Cookie>) -> Self {
        let now = Utc::now();
        Self {
            cookies,
            captured_at: now,
            soft_expires_at: now + Duration::days(30),
        }
    }

    /// 软 TTL 是否已过期。真正失效由上游 401 触发刷新。
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.soft_expires_at
    }

    /// 查指定名字的 cookie 值。
    pub fn get(&self, name: &str) -> Option<&str> {
        self.cookies
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.value.as_str())
    }

    /// 方便日志：把 cookie value 脱敏为"前 8 位 + ***"。
    pub fn redacted(&self) -> HashMap<&str, String> {
        self.cookies
            .iter()
            .map(|c| (c.name.as_str(), redact(&c.value)))
            .collect()
    }
}

fn redact(v: &str) -> String {
    if v.len() <= 8 {
        "***".to_string()
    } else {
        format!("{}***", &v[..8])
    }
}

/// 读取 `~/.sjtu-cli/session.json`。文件不存在时返回 `NotAuthenticated`。
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

/// 保存 session；自动 mkdir -p 父目录；Unix 下 chmod 600。
pub fn save_session(session: &Session) -> Result<()> {
    config::ensure_dirs()?;
    let path = config::session_path()?;
    let raw = serde_json::to_string_pretty(session).context("序列化 session 失败")?;
    std::fs::write(&path, raw).with_context(|| format!("写入 {} 失败", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&path)?.permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(&path, perm).context("无法设置 session.json 权限为 600")?;
    }

    Ok(())
}

/// 清除 session 文件（用于 `sjtu logout`）。幂等。
pub fn clear_session() -> Result<()> {
    let path = config::session_path()?;
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("删除 {} 失败", path.display()))?;
    }
    Ok(())
}

// === 子系统 session 的读写 ===
//
// 每个子系统（教务 jwc / 一卡通 card / Canvas / ...）走完一次 CAS 跳转后，
// 拿到一组属于该子系统域的 cookie，独立缓存到 `~/.sjtu-cli/sub_sessions/<name>.json`。
// 文件结构复用主 `Session`（同一份 30 天软 TTL）；上游真返 401 时调用方负责清缓存重做。

fn sub_session_path(name: &str) -> Result<std::path::PathBuf> {
    if name.is_empty() || name.contains(['/', '\\', '.', ' ']) {
        return Err(SjtuCliError::InvalidInput(format!(
            "非法子系统名 `{name}`：禁止空 / `.` / 路径分隔符 / 空格"
        ))
        .into());
    }
    Ok(config::sub_sessions_dir()?.join(format!("{name}.json")))
}

/// 读 `~/.sjtu-cli/sub_sessions/<name>.json`。文件不存在返回 `NotAuthenticated`
/// （由调用方决定是发起 CAS 还是直接报错）。
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

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&path)?.permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(&path, perm)
            .with_context(|| format!("无法设置 {} 权限为 600", path.display()))?;
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_session_name_rejects_path_traversal() {
        assert!(sub_session_path("").is_err());
        assert!(sub_session_path("..").is_err());
        assert!(sub_session_path("a/b").is_err());
        assert!(sub_session_path("a\\b").is_err());
        assert!(sub_session_path("with space").is_err());
        assert!(sub_session_path("ok_name").is_ok());
        assert!(sub_session_path("jwc").is_ok());
    }
}
