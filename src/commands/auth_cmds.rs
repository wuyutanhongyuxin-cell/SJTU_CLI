//! `sjtu login` / `sjtu logout` / `sjtu status` / `sjtu test-cas` 四个子命令的实现。

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::auth::{self, cas, Backend};
use crate::cookies::{clear_session, load_session};
use crate::error::SjtuCliError;
use crate::output::{render, Envelope, OutputFormat};

/// `sjtu login` 的成功 data 形状。
#[derive(Debug, Serialize)]
struct LoginData {
    authenticated: bool,
    cookie_count: usize,
    captured_at: DateTime<Utc>,
    soft_expires_at: DateTime<Utc>,
    jaauth_cookie_redacted: String,
}

/// `sjtu logout` 的 data。
#[derive(Debug, Serialize)]
struct LogoutData {
    cleared: bool,
}

/// `sjtu status` 的 data（已登录分支）。
#[derive(Debug, Serialize)]
struct StatusData {
    authenticated: bool,
    is_expired: bool,
    captured_at: DateTime<Utc>,
    soft_expires_at: DateTime<Utc>,
    cookies: HashMap<String, String>,
}

/// `sjtu login [--browser chrome|rookie]`
pub fn cmd_login(backend: Backend, fmt: Option<OutputFormat>) -> Result<()> {
    let session = auth::login(backend)?;
    let jaauth = session
        .get("JAAuthCookie")
        .map(redact)
        .unwrap_or_else(|| "(missing)".to_string());
    let data = LoginData {
        authenticated: true,
        cookie_count: session.cookies.len(),
        captured_at: session.captured_at,
        soft_expires_at: session.soft_expires_at,
        jaauth_cookie_redacted: jaauth,
    };
    render(Envelope::ok(data), fmt)
}

/// `sjtu logout`：幂等。
pub fn cmd_logout(fmt: Option<OutputFormat>) -> Result<()> {
    use crate::config::session_path;
    let existed = session_path().map(|p| p.exists()).unwrap_or(false);
    clear_session()?;
    render(Envelope::ok(LogoutData { cleared: existed }), fmt)
}

/// `sjtu status`：未登录也返回 exit 码 0，只是 Envelope 填 error。
pub fn cmd_status(fmt: Option<OutputFormat>) -> Result<()> {
    match load_session() {
        Ok(session) => {
            let data = StatusData {
                authenticated: true,
                is_expired: session.is_expired(),
                captured_at: session.captured_at,
                soft_expires_at: session.soft_expires_at,
                cookies: session.redacted(),
            };
            render(Envelope::ok(data), fmt)
        }
        Err(e) => {
            // 只有 NotAuthenticated 走"软失败"；其它错误（如 JSON 解析失败）仍然冒泡。
            if let Some(sjtu_err) = e.downcast_ref::<SjtuCliError>() {
                if matches!(sjtu_err, SjtuCliError::NotAuthenticated) {
                    let env = Envelope::<()>::err(sjtu_err.code(), sjtu_err.to_string());
                    return render(env, fmt);
                }
            }
            Err(e)
        }
    }
}

fn redact(v: &str) -> String {
    if v.len() <= 8 {
        "***".to_string()
    } else {
        format!("{}***", &v[..8])
    }
}

/// `sjtu test-cas <url> --name <n>` 的 data：调试用，给 S2 checkpoint 看缓存命中。
#[derive(Debug, Serialize)]
struct TestCasData {
    name: String,
    target_url: String,
    final_url: String,
    from_cache: bool,
    elapsed_ms: u128,
    cookie_count: usize,
    cookies: HashMap<String, String>,
}

/// `sjtu test-cas`：跑一次 CAS 通用通道，打印耗时与缓存命中。
pub async fn cmd_test_cas(url: String, name: String, fmt: Option<OutputFormat>) -> Result<()> {
    let result = cas::cas_login(&name, &url).await?;
    let data = TestCasData {
        name,
        target_url: url,
        final_url: result.final_url,
        from_cache: result.from_cache,
        elapsed_ms: result.elapsed_ms,
        cookie_count: result.session.cookies.len(),
        cookies: result.session.redacted(),
    };
    render(Envelope::ok(data), fmt)
}
