//! 本地 OAuth2 callback server：bind 127.0.0.1:45123 接 GET /callback?code=...&state=...
//!
//! 设计：
//! - 单连接 listener：accept 一个 connection → 解析第一行 GET → 返 200 OK + HTML → 关闭
//! - 5 分钟超时（用户没在浏览器同意 / 浏览器没弹出）
//! - state 校验：传入期望 state，请求 state 不匹配 → 拒绝并报错
//! - **不**用 axum / warp / hyper-server，纯 tokio::net::TcpListener + 手写 1 个 GET 解析

use std::time::Duration;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;

use crate::error::SjtuCliError;

const BIND_ADDR: &str = "127.0.0.1:45123";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

const SUCCESS_HTML: &str = "HTTP/1.1 200 OK\r\n\
Content-Type: text/html; charset=utf-8\r\n\
Connection: close\r\n\
\r\n\
<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>授权成功</title></head>\
<body><h2>sjtu-cli 授权成功</h2><p>可关闭本窗口，回到终端查看结果。</p></body></html>";

/// 在 127.0.0.1:45123 启 listener 等浏览器 callback。
///
/// 返回 `Ok((code, state))` 当解析成功；超时返 `CardOAuthTimeout`；
/// IO/解析错返 `CardOAuth(...)`。
///
/// **注意**：返回前会先给浏览器写 200 OK + HTML，保证用户看到"授权成功"。
pub async fn wait_for_callback() -> Result<(String, String)> {
    let listener = TcpListener::bind(BIND_ADDR).await.map_err(|e| {
        SjtuCliError::CardOAuth(format!(
            "无法 bind {BIND_ADDR}（端口被占用？）: {e}"
        ))
    })?;
    let (mut sock, _addr) = timeout(CALLBACK_TIMEOUT, listener.accept())
        .await
        .map_err(|_| SjtuCliError::CardOAuthTimeout)?
        .map_err(|e| SjtuCliError::CardOAuth(format!("accept 失败: {e}")))?;
    // 读至多 4 KiB 拿到 GET 第一行（OAuth2 callback URL 不可能更长）
    let mut buf = vec![0u8; 4096];
    let n = sock
        .read(&mut buf)
        .await
        .map_err(|e| SjtuCliError::CardOAuth(format!("读 socket: {e}")))?;
    let request = String::from_utf8_lossy(&buf[..n]).into_owned();
    let result = parse_callback_request(&request);
    // 不管成功失败都写一个响应回浏览器（失败也避免浏览器一直转圈）
    let _ = sock.write_all(SUCCESS_HTML.as_bytes()).await;
    let _ = sock.shutdown().await;
    result
}

/// 从 raw HTTP request 第一行 `GET /callback?code=X&state=Y HTTP/1.1` 解析 code/state。
pub(crate) fn parse_callback_request(req: &str) -> Result<(String, String)> {
    let first_line = req.lines().next().ok_or_else(|| {
        SjtuCliError::CardOAuth("callback 请求为空".to_string())
    })?;
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    if method != "GET" {
        return Err(
            SjtuCliError::CardOAuth(format!("期望 GET，实际 {method}")).into(),
        );
    }
    // target = "/callback?code=X&state=Y"
    let query = target
        .split_once('?')
        .map(|(_, q)| q)
        .ok_or_else(|| SjtuCliError::CardOAuth("callback 缺 query string".to_string()))?;
    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut err_param: Option<String> = None;
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v_dec = percent_decode(v);
        match k {
            "code" => code = Some(v_dec),
            "state" => state = Some(v_dec),
            "error" => err_param = Some(v_dec),
            _ => {} // 忽略 error_description / scope 等
        }
    }
    if let Some(e) = err_param {
        return Err(SjtuCliError::CardOAuth(format!("callback 返回 error={e}")).into());
    }
    let code = code.ok_or_else(|| SjtuCliError::CardOAuth("callback 缺 code 参数".to_string()))?;
    let state =
        state.ok_or_else(|| SjtuCliError::CardOAuth("callback 缺 state 参数".to_string()))?;
    Ok((code, state))
}

/// 简单 percent-decode：把 %xx 还原为字节。仅 ASCII；中文等多字节 OAuth2 callback 不会出现。
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        // 把 + 也当作空格（form-encoded 兼容；OAuth2 query 实际不出现）
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 校验 state 一致（CSRF 防御）。不匹配返 CardOAuth("state_mismatch")。
pub fn check_state(got: &str, expected: &str) -> Result<()> {
    if got != expected {
        return Err(SjtuCliError::CardOAuth(format!(
            "state_mismatch: 期望 {expected} 实际 {got}"
        ))
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_callback_happy() {
        let r = "GET /callback?code=ABC&state=XYZ HTTP/1.1\r\nHost: 127.0.0.1:45123\r\n\r\n";
        let (c, s) = parse_callback_request(r).unwrap();
        assert_eq!(c, "ABC");
        assert_eq!(s, "XYZ");
    }

    #[test]
    fn parse_callback_with_error_param() {
        let r = "GET /callback?error=access_denied&state=XYZ HTTP/1.1\r\n\r\n";
        let e = parse_callback_request(r).expect_err("error 参数应抛错");
        assert!(format!("{e}").contains("access_denied"));
    }

    #[test]
    fn parse_callback_missing_code() {
        let r = "GET /callback?state=XYZ HTTP/1.1\r\n\r\n";
        let e = parse_callback_request(r).expect_err("缺 code 应抛错");
        assert!(format!("{e}").contains("缺 code"));
    }

    #[test]
    fn parse_callback_percent_decode() {
        let r = "GET /callback?code=A%2BB%2FC&state=XYZ HTTP/1.1\r\n\r\n";
        let (c, _) = parse_callback_request(r).unwrap();
        assert_eq!(c, "A+B/C");
    }

    #[test]
    fn state_mismatch_returns_err() {
        let e = check_state("got", "expected").expect_err("不匹配应抛错");
        assert!(format!("{e}").contains("state_mismatch"));
    }

    #[test]
    fn state_match_ok() {
        check_state("xyz", "xyz").expect("匹配应通过");
    }
}
