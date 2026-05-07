//! §2.1 N305005 — 学生成绩查询。

use anyhow::Result;

use super::Client;
use crate::apps::jwc::http::post_form_json;
use crate::apps::jwc::models::{Grade, JwcPage};

impl Client {
    /// `xnm` 学年 4 位（如 `2025`），`None` = 全部；
    /// `xqm` 学期编码（`3` 秋 / `12` 春 / `16` 夏），`None` = 全部；
    /// `page` 从 1 起；`page_size` 范围 15..500。
    pub async fn grades(
        &self,
        xnm: Option<&str>,
        xqm: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<JwcPage<Grade>> {
        // 必须先 GET SP 页面把 gnmkdm 绑到 Tomcat session（否则 POST 一律 901）。
        self.ensure_sp_bound("/cjcx/cjcx_cxDgXscj.html", "N305005", "N305005 grades")
            .await?;

        let mut form = self.build_common_form(page, page_size);
        form.push(("xnm", xnm.unwrap_or("").to_string()));
        form.push(("xqm", xqm.unwrap_or("").to_string()));
        form.push(("sfzgcj", String::new())); // 是否仅最高成绩：空 = 否
        form.push(("kcbj", String::new())); // 主辅修筛选：空 = 全部

        post_form_json(
            &self.http,
            &self.throttle,
            "/cjcx/cjcx_cxXsgrcj.html",
            "N305005",
            Some("query"),
            "/cjcx/cjcx_cxDgXscj.html",
            &form,
            "N305005 grades",
        )
        .await
    }
}
