//! 教务 Client：CAS 跳转 + 各 SP 数据查询入口（按 SP 分文件）。
//!
//! 认证链路：复用 S2 `cas::cas_login("jwc", LOGIN_URL)`。
//! CAS 302 链最终落到 i.sjtu 首页时种 JSESSIONID + keepalive，本模块只负责拿 cookie，
//! ZF 后端的 csrftoken 在 page HTML hidden input 里（CLI 暂不实现写操作，无需解析）。
//!
//! 各 SP 实现规则：
//! - 一律走 `super::http::post_form_json`（统一 header + 节流 + 错误诊断）
//! - 字段命名沿用 ZF 拼音缩写（与 `tasks/isjtu_investigation.md` §2 字段表一致）
//! - 公共 form 字段（queryModel.* / _search / nd / time / pkey）由 `build_common_form` 拼
//! - 每个 SP 一文件，`impl Client { ... }` 分散在各子模块；公共 helper 在本文件。

mod exams;
mod gpa;
mod grades;
mod schedule;

pub use gpa::{GpaRank, GpaScope};

use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use reqwest::Client as HttpClient;
use tokio::sync::Mutex;

use super::bind::visit_sp_page;
use super::http::build_http_client;
use super::throttle::Throttle;
use crate::auth::cas::cas_login;

/// CAS 跳转目标：i.sjtu 的 jAccount 登录入口。
///
/// 直接给 `/xtgl/index_initMenu.html` 等深页 ZF 不会自动触发 jaccount CAS dance ——
/// 它只会把请求 302 到自家内部 `/xtgl/login_slogin.html`（用户名密码登录页），
/// 拿到的是 anonymous JSESSIONID，后续 API POST 一律 status=901。
///
/// 正确入口从 ZF login 页 HTML 解出：`<a href="/jaccountlogin" id="authJwglxtLoginURL">`。
/// 该 URL 触发：i.sjtu → jaccount?sid=...&service=... → JAAuthCookie 验证 → 302 回 i.sjtu?ticket=ST-XXX
/// → ZF 校验 ticket → 绑定 user_id 到 JSESSIONID → 302 到 nav 主页。
/// 实测 2026-04-26（详 tasks/lessons.md）。
pub(super) const LOGIN_URL: &str = "https://i.sjtu.edu.cn/jaccountlogin";

/// 教务 Client。
pub struct Client {
    pub(super) http: HttpClient,
    pub(super) throttle: Arc<Throttle>,
    /// 同一会话内 query 累计计数（ZF 表单字段 `time` 自增防缓存戳，从 0 起）。
    pub(super) time_counter: AtomicU32,
    /// 已绑定到 Tomcat session 的 SP 页面集合（避免重复 pre-GET）。
    visited_sp: Mutex<HashSet<&'static str>>,
    /// CAS 返回的元数据，供上层 Envelope 展示。
    pub login: LoginMeta,
}

/// 登录元数据，暴露给 CLI 构造 Envelope。
#[derive(Debug, Clone)]
pub struct LoginMeta {
    pub from_cache: bool,
    pub elapsed_ms: u128,
    pub final_url: String,
}

impl Client {
    /// CAS 跳转 → 注入 cookie 构造 reqwest Client。
    pub async fn connect() -> Result<Self> {
        let r = cas_login("jwc", LOGIN_URL).await?;
        let http = build_http_client(&r.session)?;
        Ok(Self {
            http,
            throttle: Arc::new(Throttle::new()),
            time_counter: AtomicU32::new(0),
            visited_sp: Mutex::new(HashSet::new()),
            login: LoginMeta {
                from_cache: r.from_cache,
                elapsed_ms: r.elapsed_ms,
                final_url: r.final_url,
            },
        })
    }

    /// 确保 SP 页面已绑到 session（同一 Client 生命周期内每个 page_path 只 GET 一次）。
    ///
    /// ZF 后端在 GET `<sp-page>?gnmkdm=<gnmkdm>&layout=default` 时把功能模块登记到
    /// server-side session；未绑则后续数据 POST 一律 status=901（实测 2026-04-26）。
    pub(super) async fn ensure_sp_bound(
        &self,
        page_path: &'static str,
        gnmkdm: &str,
        label: &str,
    ) -> Result<()> {
        {
            let visited = self.visited_sp.lock().await;
            if visited.contains(page_path) {
                return Ok(());
            }
        }
        visit_sp_page(&self.http, &self.throttle, page_path, gnmkdm, label).await?;
        self.visited_sp.lock().await.insert(page_path);
        Ok(())
    }

    /// 拼公共 form 字段（§1.5）：`queryModel.*` + `_search` + `nd` + `time` + `pkey`。
    /// 调用方在此基础上 push SP 专属字段。
    pub(super) fn build_common_form(
        &self,
        page: u32,
        page_size: u32,
    ) -> Vec<(&'static str, String)> {
        let nd = Utc::now().timestamp_millis().to_string();
        let time = self
            .time_counter
            .fetch_add(1, Ordering::Relaxed)
            .to_string();
        vec![
            ("queryModel.showCount", page_size.to_string()),
            ("queryModel.currentPage", page.to_string()),
            ("queryModel.sortName", String::new()),
            ("queryModel.sortOrder", "asc".to_string()),
            ("_search", "false".to_string()),
            ("nd", nd),
            ("time", time),
            ("pkey", String::new()),
        ]
    }
}
