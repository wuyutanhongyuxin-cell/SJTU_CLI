//! `sjtu login` / `sjtu logout` / `sjtu status` 三个子命令的实现。

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::auth::{self, Backend};
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
