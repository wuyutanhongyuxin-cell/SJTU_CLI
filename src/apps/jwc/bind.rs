//! SP 页面预 GET：把 ZF 功能模块绑到 Tomcat session。
//!
//! ZF（正方）后端在 GET `<sp-page>?gnmkdm=<gnmkdm>&layout=default` 时，把对应
//! 功能模块（gnmkdm）登记到 server-side session。**未绑** → 后续数据 POST 一律
//! `status=901` + 空 body（2026-04-26 实测，详 tasks/lessons.md）。
//!
//! 浏览器里"点 nav → 进 SP 页 → 按查询"流程隐含了这个 GET，CLI 必须显式补一次。
//! 同一 Client 生命周期内每个 page_path 只 GET 一次（`api::Client::ensure_sp_bound`）。

use anyhow::Result;
use reqwest::header::{ACCEPT, USER_AGENT};
use reqwest::Client;
use tracing::debug;

use super::http::{BASE, UA};
use super::throttle::Throttle;
use crate::error::SjtuCliError;

/// GET SP 页面，确认 final URL == 请求 path（否则说明被打回 login 页）。
pub(super) async fn visit_sp_page(
    http: &Client,
    throttle: &Throttle,
    page_path: &str,
    gnmkdm: &str,
    label: &str,
) -> Result<()> {
    throttle.wait().await;
    let url = format!("{BASE}{page_path}?gnmkdm={gnmkdm}&layout=default");
    let resp = http
        .get(&url)
        .header(
            ACCEPT,
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .header(USER_AGENT, UA)
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("GET {url}: {e}")))?;
    let status = resp.status();
    let final_url = resp.url().to_string();

    if final_url.contains("jaccount.sjtu.edu.cn") {
        return Err(SjtuCliError::UpstreamError(format!(
            "{label} pre-GET 被打回 jaccount 登录页（{final_url}）；JAAuthCookie 已失效，请 `sjtu logout` 后重新 `sjtu login`"
        )).into());
    }
    if !status.is_success() {
        return Err(SjtuCliError::UpstreamError(format!(
            "{label} pre-GET 失败 status={status} url={final_url}"
        ))
        .into());
    }
    // ZF 的内部 login 页（/xtgl/login_slogin.html）落在 i.sjtu 同域，单看 host 防不住，
    // 必须比对 final_url 的 path 与请求 page_path 是否一致。
    let final_path = url::Url::parse(&final_url)
        .ok()
        .map(|u| u.path().to_string())
        .unwrap_or_default();
    if !final_path.eq_ignore_ascii_case(page_path) {
        tracing::debug!(
            label, %final_url, %final_path, expected = page_path,
            "ZF 服务端 stale（pre-GET 被重定向到 login_slogin），触发 SubSessionStale"
        );
        return Err(SjtuCliError::SubSessionStale("jwc").into());
    }
    debug!(target: "jwc::bind", page_path, gnmkdm, "SP page bound to ZF session");
    Ok(())
}
