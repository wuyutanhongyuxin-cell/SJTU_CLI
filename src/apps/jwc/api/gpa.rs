//! §2.3 N309131 — GPA / 学积分查询（**两阶段**）。

use anyhow::Result;

use super::Client;
use crate::apps::jwc::http::post_form_json;
use crate::apps::jwc::models::{Gpa, JwcPage};
use crate::error::SjtuCliError;

/// 默认不计平均分 / 不计绩点的成绩类型（§2.3）。
const BJPJF_DEFAULT: &str = "缓考,缓考(重考),尚未修读,暂不记录,中期退课,重考报名";

/// 课程范围：`hxkc` = 核心课程；`qbkc` = 全部课程。
#[derive(Debug, Clone, Copy)]
pub enum GpaScope {
    HxKc,
    QbKc,
}

impl GpaScope {
    fn as_str(self) -> &'static str {
        match self {
            GpaScope::HxKc => "hxkc",
            GpaScope::QbKc => "qbkc",
        }
    }
}

/// 排名范围：`njzy` = 年级专业；`nj` = 年级；`bj` = 班级。
#[derive(Debug, Clone, Copy)]
pub enum GpaRank {
    NjZy,
    Nj,
    Bj,
}

impl GpaRank {
    fn as_str(self) -> &'static str {
        match self {
            GpaRank::NjZy => "njzy",
            GpaRank::Nj => "nj",
            GpaRank::Bj => "bj",
        }
    }
}

impl Client {
    /// 两阶段 GPA：
    /// 1) `tjGpapmtj` 触发 server-side 计算（写临时表），返裸 JSON 字符串 `"统计成功！"`。
    /// 2) `cxGpaxjfcxIndex` 拉计算结果（标准 §1.3 envelope，items[0] 是当前学生）。
    ///
    /// **必须按序**：跳 step 1 直接 step 2 会返空 / 报错。
    ///
    /// `qs_xnxq`/`zz_xnxq` 起止学年学期 6 位编码（如 `"202503"` = 2025-2026 第 1 学期），
    /// 留 `None` 都空 = 全部学期累计。
    pub async fn gpa(
        &self,
        scope: GpaScope,
        rank: GpaRank,
        qs_xnxq: Option<&str>,
        zz_xnxq: Option<&str>,
    ) -> Result<JwcPage<Gpa>> {
        self.ensure_sp_bound(
            "/cjpmtj/gpapmtj_cxGpaxjfcxIndex.html",
            "N309131",
            "N309131 gpa",
        )
        .await?;

        let base_form = build_gpa_form(scope, rank, qs_xnxq, zz_xnxq);

        // step 1：触发统计；响应是裸 JSON 字符串 `"统计成功！"`，不是对象。
        let msg: String = post_form_json(
            &self.http,
            &self.throttle,
            "/cjpmtj/gpapmtj_tjGpapmtj.html",
            "N309131",
            None, // step 1 端点不带 doType
            "/cjpmtj/gpapmtj_cxGpaxjfcxIndex.html",
            &base_form,
            "N309131 gpa step1",
        )
        .await?;
        if !msg.contains("统计成功") {
            return Err(SjtuCliError::UpstreamError(format!(
                "N309131 gpa step1 未返 \"统计成功！\"：{msg}"
            ))
            .into());
        }

        // step 2：拉结果。复用同一组 form 字段 + 公共 queryModel.* 等。
        let mut form = self.build_common_form(1, 50);
        form.extend(base_form);

        post_form_json(
            &self.http,
            &self.throttle,
            "/cjpmtj/gpapmtj_cxGpaxjfcxIndex.html",
            "N309131",
            Some("query"),
            "/cjpmtj/gpapmtj_cxGpaxjfcxIndex.html",
            &form,
            "N309131 gpa step2",
        )
        .await
    }
}

/// 拼 N309131 两阶段共享的 form 字段（§2.3 默认值）。
fn build_gpa_form(
    scope: GpaScope,
    rank: GpaRank,
    qs_xnxq: Option<&str>,
    zz_xnxq: Option<&str>,
) -> Vec<(&'static str, String)> {
    vec![
        ("qsXnxq", qs_xnxq.unwrap_or("").to_string()),
        ("zzXnxq", zz_xnxq.unwrap_or("").to_string()),
        ("tjgx", "0".to_string()),
        ("alsfj", String::new()),
        ("sspjfblws", "9".to_string()),
        ("pjjdblws", "9".to_string()),
        ("bjpjf", BJPJF_DEFAULT.to_string()),
        ("bjjd", BJPJF_DEFAULT.to_string()),
        ("kch_ids", String::new()),
        ("bcjkc_id", String::new()),
        ("bcjkz_id", String::new()),
        ("cjkz_id", String::new()),
        ("cjxzm", "zhyccj".to_string()),
        ("kcfw", scope.as_str().to_string()),
        ("tjfw", rank.as_str().to_string()),
        ("xjzt", "1".to_string()),
    ]
}
