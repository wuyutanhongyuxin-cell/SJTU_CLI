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

    /// §2.7 N2154 周次课表查询。返回的 envelope 含 `rqazcList`（每天 ISO 日期）+
    /// `kbList[*].oldzc/oldjc` 周次/节次 bitmask。
    ///
    /// `zs` 必填（1..=18 教学周）；`xnm`/`xqm` 留空 = 当前学年/学期。
    pub async fn schedule_by_week(
        &self,
        xnm: Option<&str>,
        xqm: Option<&str>,
        zs: u8,
    ) -> Result<Schedule> {
        self.ensure_sp_bound(
            "/kbcx/xskbcxMobile_cxXskbcxMobileIndex.html",
            "N2154",
            "N2154 周课表",
        )
        .await?;

        let form = build_n2154_form(xnm, xqm, zs);
        post_form_json(
            &self.http,
            &self.throttle,
            "/kbcx/xskbcxMobile_cxXsKb.html",
            "N2154",
            None, // §2.7：端点不带 doType（form 里已有）
            "/kbcx/xskbcxMobile_cxXskbcxMobileIndex.html",
            &form,
            "N2154 周课表",
        )
        .await
    }

    /// 反推今天属于第几教学周。优先读 cache（TTL 内）；miss → 调 N2154 zs=1 算 delta。
    ///
    /// 返回 0 = 学期未开始；超 18 = 假期/学期已结束。
    pub async fn infer_current_week(&self, xnm: Option<&str>, xqm: Option<&str>) -> Result<u8> {
        use super::week_cache::{
            cache_key, cache_ttl_seconds, compute_current_week, read_cache_if_fresh, write_cache,
        };
        use chrono::NaiveDate;

        let key = cache_key(xnm, xqm);
        let ttl = cache_ttl_seconds(xnm, xqm);

        if let Some(cw) = read_cache_if_fresh(&key, ttl) {
            return Ok(cw);
        }

        let s = self.schedule_by_week(xnm, xqm, 1).await?;
        // 找 xqj=1（周一）的那条；找不到退到 rqazc_list[0]。RqAzc.xqj 是 Option<u8>。
        let week1_iso = s
            .rqazc_list
            .iter()
            .find_map(|r| r.rq.as_deref().filter(|_| r.xqj == Some(1)))
            .or_else(|| s.rqazc_list.first().and_then(|r| r.rq.as_deref()))
            .ok_or_else(|| {
                anyhow::anyhow!("N2154 zs=1 响应缺少 rqazcList[*].rq，无法反推当前周")
            })?;

        let week1_monday = NaiveDate::parse_from_str(week1_iso, "%Y-%m-%d")
            .map_err(|e| anyhow::anyhow!("rqazcList[0].rq '{week1_iso}' 非 ISO 日期: {e}"))?;
        let today = chrono::Local::now().date_naive();
        let cw = compute_current_week(week1_monday, today);

        let _ = write_cache(&key, cw);
        Ok(cw)
    }
}

/// 拼 N2154 form（pure 函数，便于单测）。
fn build_n2154_form(xnm: Option<&str>, xqm: Option<&str>, zs: u8) -> Vec<(&'static str, String)> {
    vec![
        ("xnm", xnm.unwrap_or("").to_string()),
        ("xqm", xqm.unwrap_or("").to_string()),
        ("zs", zs.to_string()),
        ("kblx", "1".to_string()),
        ("doType", "app".to_string()),
        ("xh", String::new()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 N2154 form 是否拼对（字段顺序 + zs 数值）。
    /// 由于真实调用需要 SJTU session，这里只测 form 拼装是 pure 函数式。
    #[test]
    fn n2154_form_contains_required_fields() {
        let form = build_n2154_form(Some("2025"), Some("12"), 14);
        assert!(form.iter().any(|(k, v)| *k == "xnm" && v == "2025"));
        assert!(form.iter().any(|(k, v)| *k == "xqm" && v == "12"));
        assert!(form.iter().any(|(k, v)| *k == "zs" && v == "14"));
        assert!(form.iter().any(|(k, v)| *k == "kblx" && v == "1"));
        assert!(form.iter().any(|(k, v)| *k == "doType" && v == "app"));
        assert!(form.iter().any(|(k, v)| *k == "xh" && v.is_empty()));
    }

    #[test]
    fn n2154_form_empty_xnm_xqm_default_to_empty_string() {
        let form = build_n2154_form(None, None, 1);
        assert!(form.iter().any(|(k, v)| *k == "xnm" && v.is_empty()));
        assert!(form.iter().any(|(k, v)| *k == "xqm" && v.is_empty()));
    }
}
