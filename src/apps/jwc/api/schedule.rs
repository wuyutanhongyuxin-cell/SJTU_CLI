//! §2.2 N2151 — 个人课表查询。

use anyhow::Result;

use super::Client;
use crate::apps::jwc::http::post_form_json;
use crate::apps::jwc::models::Schedule;

impl Client {
    /// 拉学年学期完整课表（一周 7 天 7 节铺平）。
    ///
    /// `xnm`/`xqm` 留空 = 当前学年/学期。返回 envelope 是 N2151 专属（**非** `JwcPage`）：
    /// `kbList` 课程数组 + `xqjmcMap` 周几文本映射，其余身份/显示侧字段已抹掉。
    ///
    /// 端点 URL 不带 `doType=query`（§2.2 contract）；无 `queryModel.*` 公共字段。
    pub async fn schedule(&self, xnm: Option<&str>, xqm: Option<&str>) -> Result<Schedule> {
        self.ensure_sp_bound("/kbcx/xskbcx_cxXskbcxIndex.html", "N2151", "N2151 schedule")
            .await?;

        // §2.2 form: 极简 6 字段（无公共 queryModel.*）；kzlx=ck 不能省。
        let form: Vec<(&str, String)> = vec![
            ("xnm", xnm.unwrap_or("").to_string()),
            ("xqm", xqm.unwrap_or("").to_string()),
            ("kzlx", "ck".to_string()),
            ("xsdm", String::new()),
            ("kclbdm", String::new()),
            ("kclxdm", String::new()),
        ];

        post_form_json(
            &self.http,
            &self.throttle,
            "/kbcx/xskbcx_cxXsgrkb.html",
            "N2151",
            None, // §2.2 端点不带 doType
            "/kbcx/xskbcx_cxXskbcxIndex.html",
            &form,
            "N2151 schedule",
        )
        .await
    }
}
