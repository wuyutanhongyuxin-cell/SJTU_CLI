//! §2.4 N358105 — 考试信息查询。

use anyhow::Result;

use super::Client;
use crate::apps::jwc::http::post_form_json;
use crate::apps::jwc::models::{Exam, JwcPage};

impl Client {
    /// `xnm`/`xqm` 留空 = 当前学年/学期；当前学期未排考时返 `items:[], totalResult:0` 不报错。
    /// `page` 从 1 起；`page_size` 范围 15..500（§1.5）。
    pub async fn exams(
        &self,
        xnm: Option<&str>,
        xqm: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<JwcPage<Exam>> {
        self.ensure_sp_bound("/kwgl/kscx_cxXsksxxIndex.html", "N358105", "N358105 exams")
            .await?;

        let mut form = self.build_common_form(page, page_size);
        form.push(("xnm", xnm.unwrap_or("").to_string()));
        form.push(("xqm", xqm.unwrap_or("").to_string()));
        form.push(("ksmcdmb_id", String::new()));
        form.push(("kch", String::new()));
        form.push(("kc", String::new()));
        form.push(("ksrq", String::new()));
        form.push(("kkbm_id", String::new()));

        post_form_json(
            &self.http,
            &self.throttle,
            "/kwgl/kscx_cxXsksxxIndex.html",
            "N358105",
            Some("query"),
            "/kwgl/kscx_cxXsksxxIndex.html",
            &form,
            "N358105 exams",
        )
        .await
    }
}
