//! T4 一卡通 OAuth2 Authorization Code 通道（RFC6749 标准）。
//!
//! 不用 `oauth2` crate（违 CLAUDE.md 不引入新依赖）。
//! 不用 `keyring`（跨平台行为不一致；JSON+chmod 600 与 cookies::session.json 同制，单一可审计点）。
//! 不用 `axum`（1 endpoint 不值得引入 micro-framework；手卷 60 行 listener 够用）。
//! Refresh 走 failure-driven 不走 timer（同 canvas_video::with_token_refresh 范式，省状态机）。
//!
//! 与现 `src/auth/oauth2/` 完全不同：那个是 shuiyuan 用的 302-chain 跟链，
//! 终点取 Discourse 的 `_t` cookie；本模块走 code-for-token 拿 Bearer access_token。

pub mod authorize;
pub mod callback;
pub mod refresh;
pub mod secret;
pub mod token;

#[cfg(test)]
mod tests_token;

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::error::SjtuCliError;

/// 本地落盘的 OAuth2 session schema (`~/.sjtu-cli/sub_sessions/card_oauth.json`)。
///
/// **不**复用 `cookies::Session`：那个是 cookie 形态（name/value/domain/path 列表），
/// 跟 token 形态完全不同。显式 struct 表达 OAuth2 语义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardOAuthSession {
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<FixedOffset>,
    /// 申请 scope，可读性参考（"card_info card_transactions"）。
    pub scope: String,
    /// 主卡号（首次跑 `/v1/me/card` 时存）；后续 history 查询用。
    /// 可为空（极少情况下首次跑还没拿到）。
    #[serde(default)]
    pub main_card_no: Option<String>,
    pub captured_at: DateTime<FixedOffset>,
}

const SESSION_FILE: &str = "card_oauth.json";
/// access_token 在 `expires_at - 60s` 时即视为过期，提前 refresh。
pub const REFRESH_MARGIN_SECS: i64 = 60;

/// 子 session JSON 路径：`~/.sjtu-cli/sub_sessions/card_oauth.json`。
pub fn session_path() -> Result<PathBuf> {
    Ok(config::sub_sessions_dir()?.join(SESSION_FILE))
}

/// 读 OAuth2 session。文件不存在 → NotAuthenticated。
pub fn load_session() -> Result<CardOAuthSession> {
    let path = session_path()?;
    if !path.exists() {
        return Err(SjtuCliError::NotAuthenticated.into());
    }
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("读取 {} 失败", path.display()))?;
    let s: CardOAuthSession = serde_json::from_str(&raw)
        .with_context(|| format!("解析 {} 失败（card_oauth.json 损坏？）", path.display()))?;
    Ok(s)
}

/// 保存 OAuth2 session。chmod 600 on Unix。
pub fn save_session(sess: &CardOAuthSession) -> Result<()> {
    config::ensure_dirs()?;
    let path = session_path()?;
    let raw = serde_json::to_string_pretty(sess).context("序列化 card_oauth session 失败")?;
    std::fs::write(&path, raw).with_context(|| format!("写入 {} 失败", path.display()))?;
    chmod_600(&path)?;
    Ok(())
}

/// 清除 OAuth2 session（用于 `sjtu card logout` 或异常恢复）。幂等。
pub fn clear_session() -> Result<()> {
    let path = session_path()?;
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("删除 {} 失败", path.display()))?;
    }
    Ok(())
}

/// 检查 token 是否需要 refresh（now ≥ expires_at - 60s）。
pub fn is_token_stale(sess: &CardOAuthSession) -> bool {
    let now = Utc::now().with_timezone(sess.expires_at.offset());
    let margin = chrono::Duration::seconds(REFRESH_MARGIN_SECS);
    now + margin >= sess.expires_at
}

/// 调 refresh_token 续期并落盘。
pub async fn refresh_and_save(http: &reqwest::Client) -> Result<CardOAuthSession> {
    let mut sess = load_session()?;
    let secret = secret::load_secret()?;
    let resp = token::refresh(http, &sess.refresh_token, &sess.client_id, &secret).await?;
    let now = beijing_now();
    sess.access_token = resp.access_token;
    if !resp.refresh_token.is_empty() {
        sess.refresh_token = resp.refresh_token;
    }
    sess.expires_at = now
        + chrono::Duration::seconds(resp.expires_in as i64)
        - chrono::Duration::seconds(REFRESH_MARGIN_SECS);
    sess.captured_at = now;
    save_session(&sess)?;
    Ok(sess)
}

/// 当前北京时间（+08:00）。
pub fn beijing_now() -> DateTime<FixedOffset> {
    let beijing = FixedOffset::east_opt(8 * 3600).expect("+08:00 常量");
    Utc::now().with_timezone(&beijing)
}

/// 把 expires_in（秒）+ now 算成 expires_at。
pub fn compute_expires_at(expires_in: u64) -> DateTime<FixedOffset> {
    beijing_now() + chrono::Duration::seconds(expires_in as i64)
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
    // Windows ACL 暂留 TODO（S0 继承的留白）
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sess(expires_offset_secs: i64) -> CardOAuthSession {
        let now = beijing_now();
        CardOAuthSession {
            client_id: "test_id".into(),
            access_token: "AT".into(),
            refresh_token: "RT".into(),
            expires_at: now + chrono::Duration::seconds(expires_offset_secs),
            scope: "card_info card_transactions".into(),
            main_card_no: None,
            captured_at: now,
        }
    }

    #[test]
    fn fresh_token_not_stale() {
        let s = make_sess(1700); // 28 分钟后过期，仍 fresh
        assert!(!is_token_stale(&s));
    }

    #[test]
    fn token_within_margin_is_stale() {
        let s = make_sess(30); // 30s 后过期，60s margin 内 → stale
        assert!(is_token_stale(&s));
    }

    #[test]
    fn expired_token_is_stale() {
        let s = make_sess(-100); // 100s 前过期
        assert!(is_token_stale(&s));
    }

    #[test]
    fn compute_expires_at_is_about_30min_from_now() {
        let exp = compute_expires_at(1800);
        let diff = (exp - beijing_now()).num_seconds();
        assert!((1790..=1810).contains(&diff), "实际 diff={diff}");
    }

    #[test]
    fn session_roundtrip_json() {
        let s = make_sess(1800);
        let json = serde_json::to_string(&s).unwrap();
        let back: CardOAuthSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.client_id, s.client_id);
        assert_eq!(back.access_token, s.access_token);
    }
}
