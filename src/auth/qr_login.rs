//! 扫码登录主流程：启动可见 Chrome → 访问交我办（CAS 自动跳到 JAccount 登录页）→ 截 QR → 终端重绘 → 轮询 URL → 抽 cookie。

use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use headless_chrome::protocol::cdp::Network::GetAllCookies;
use headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption;
use headless_chrome::{Browser, LaunchOptionsBuilder};
use tracing::{debug, info, warn};

use crate::auth::qr_render;
use crate::cookies::{save_session, Cookie, Session};
use crate::error::SjtuCliError;

/// 入口：交我办门户。未登录时 CAS 会把我们重定向到 JAccount 真正的登录页（带 QR）。
const ENTRY_URL: &str = "https://my.sjtu.edu.cn/ui/app/";
/// 成功标志：又被重定向回交我办即视为登录完成。
const SUCCESS_URL_PREFIX: &str = "https://my.sjtu.edu.cn/ui/app";
/// QR 轮询最长时长（秒）。xhs-cli 用的 240s，同步。
const POLL_TIMEOUT_SECS: u64 = 240;
/// 轮询间隔。
const POLL_INTERVAL_MS: u64 = 500;
/// 给 JAccount 登录页里的 QR canvas 多一点 JS 绘制时间。
const QR_RENDER_DELAY_MS: u64 = 800;

/// 主流程入口。成功则 session 已 `save_session()`。
pub fn login_with_chrome() -> Result<Session> {
    let browser = launch_browser()?;
    let tab = browser
        .new_tab()
        .map_err(|e| SjtuCliError::UpstreamError(format!("创建 tab 失败: {e}")))?;

    tab.navigate_to(ENTRY_URL)
        .map_err(|e| SjtuCliError::UpstreamError(format!("访问交我办失败: {e}")))?;
    tab.wait_until_navigated()
        .map_err(|e| SjtuCliError::UpstreamError(format!("等待重定向失败: {e}")))?;

    let landed = tab.get_url();
    info!(landed_url = %landed, "导航完成");

    if landed.starts_with(SUCCESS_URL_PREFIX) {
        // 极少见：浏览器已带有有效会话（如 user_data_dir 复用）。直接走抽 cookie 分支。
        info!("浏览器已具备 my.sjtu 会话，跳过扫码");
    } else {
        // 当前应在 jaccount.sjtu.edu.cn 的登录页。等 QR canvas 出现并画完。
        let _ = tab
            .wait_for_element("canvas")
            .or_else(|_| tab.wait_for_element("img.qr-img"))
            .or_else(|_| tab.wait_for_element("#qr-img"))
            .or_else(|_| tab.wait_for_element(".qr"));
        thread::sleep(Duration::from_millis(QR_RENDER_DELAY_MS));

        render_terminal_qr(&tab);

        println!("\n请用 JAccount App 扫码（浏览器窗口或上方终端 QR 均可）。");
        println!("等待扫码确认……超时 {POLL_TIMEOUT_SECS}s。\n");

        wait_for_success(&tab)?;
        info!("检测到跳转回 my.sjtu.edu.cn，准备抓 cookie");
        // 给浏览器一点时间把所有 Set-Cookie 落到 cookie jar。
        thread::sleep(Duration::from_millis(500));
    }

    // 用 Network.getAllCookies 跨所有域抓（默认的 tab.get_cookies() 只看当前 URL 域，
    // 拿不到 jaccount.sjtu.edu.cn 设的 JAAuthCookie）。
    let raw = tab
        .call_method(GetAllCookies(None))
        .map_err(|e| SjtuCliError::UpstreamError(format!("读浏览器 cookie 失败: {e}")))?
        .cookies;

    let cookies: Vec<Cookie> = raw
        .iter()
        .filter(|c| is_sjtu_domain(&c.domain))
        .map(cookie_from_cdp)
        .collect();

    info!(count = cookies.len(), "抓到 SJTU 域 cookie");

    if cookies.is_empty() {
        return Err(SjtuCliError::UpstreamError("未抓到任何 SJTU cookie，登录失败".into()).into());
    }

    if !cookies.iter().any(|c| c.name == "JAAuthCookie") {
        let names: Vec<String> = cookies
            .iter()
            .map(|c| format!("{}@{}", c.name, c.domain.as_deref().unwrap_or("?")))
            .collect();
        eprintln!(
            "⚠ 未在抓取的 cookie 里找到 `JAAuthCookie`。本次共抓到 {} 条 SJTU cookie：[{}]",
            cookies.len(),
            names.join(", ")
        );
        eprintln!("仍然保存了 session.json，可用 `sjtu status` 复核；若后续 CAS 跳转失败，可能 SJTU 改了 cookie 命名。");
    }

    let session = Session::new(cookies);
    save_session(&session)?;
    Ok(session)
}

/// 构造非 headless 的 Browser。Chrome 找不到时给用户一条清晰的兜底路径。
fn launch_browser() -> Result<Browser> {
    let opts = LaunchOptionsBuilder::default()
        .headless(false)
        .idle_browser_timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| anyhow!("构造 LaunchOptions 失败: {e}"))?;

    Browser::new(opts).map_err(|e| {
        SjtuCliError::UpstreamError(format!(
            "Chrome 启动失败: {e}。若本机已用浏览器登过 JAccount，可试 `sjtu login --browser rookie`"
        ))
        .into()
    })
}

/// best-effort：截屏 → 解码 → 终端 ANSI QR。失败都只提示，不中断主流程。
fn render_terminal_qr(tab: &headless_chrome::Tab) {
    match tab.capture_screenshot(CaptureScreenshotFormatOption::Png, None, None, true) {
        Ok(png) => match qr_render::decode_qr_from_png(&png) {
            Ok(payload) => {
                if let Err(e) = qr_render::render_ansi_to_stdout(&payload) {
                    warn!(error = %e, "终端 QR 渲染失败");
                    eprintln!("(终端 QR 渲染失败，请到浏览器窗口扫)");
                }
            }
            Err(e) => {
                debug!(error = %e, "解码页面 QR 失败");
                eprintln!("(未能自动解码 QR，请到浏览器窗口扫)");
            }
        },
        Err(e) => {
            warn!(error = %e, "截屏失败");
            eprintln!("(截屏失败，请到浏览器窗口扫)");
        }
    }
}

/// 轮询 URL，直到匹配 my.sjtu.edu.cn 或超时。
fn wait_for_success(tab: &headless_chrome::Tab) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(POLL_TIMEOUT_SECS);
    while Instant::now() < deadline {
        let url = tab.get_url();
        if url.starts_with(SUCCESS_URL_PREFIX) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    }
    Err(SjtuCliError::UpstreamError(format!("QR 登录超时（{POLL_TIMEOUT_SECS}s 未扫）")).into())
}

fn is_sjtu_domain(domain: &str) -> bool {
    let d = domain.trim_start_matches('.');
    d == "sjtu.edu.cn" || d.ends_with(".sjtu.edu.cn")
}

fn cookie_from_cdp(c: &headless_chrome::protocol::cdp::Network::Cookie) -> Cookie {
    Cookie {
        name: c.name.clone(),
        value: c.value.clone(),
        domain: Some(c.domain.clone()),
        path: if c.path.is_empty() {
            None
        } else {
            Some(c.path.clone())
        },
        expires: expires_to_datetime(c.expires),
    }
}

fn expires_to_datetime(expires: f64) -> Option<DateTime<Utc>> {
    if expires <= 0.0 {
        return None; // 会话 cookie
    }
    let secs = expires as i64;
    let nanos = ((expires - secs as f64) * 1e9) as u32;
    DateTime::<Utc>::from_timestamp(secs, nanos)
}
