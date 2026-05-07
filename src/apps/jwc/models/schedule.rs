//! §2.2 N2151 个人课表查询响应实体（专属 envelope，不复用 `JwcPage`）。

use serde::{Deserialize, Serialize};

/// §2.2 N2151 课表 envelope。
///
/// **不暴露的字段**（默认抹掉）：
/// - `xsxx` —— 学生身份信息（XH/XM/YWXM/NJDM_ID/ZYMC/BJMC 全身份）
/// - `qsxqj` / `sjkList` / `sjfwkg` / `rqazcList` / `xskbsfxstkzt` —— ZF 显示侧噪音
///   （`rqazcList` 在 N2154 周课表里有用，但 N2151 此处永远空）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    /// 周几文本映射 `{"1":"星期一", ..., "7":"星期日"}`，原样转出。
    #[serde(default)]
    pub xqjmc_map: serde_json::Value,
    /// 课表条目（已按周几+节次铺平）。
    #[serde(default)]
    pub kb_list: Vec<KbItem>,
}

/// §2.2 N2151 `kbList[*]` 单条课程。
///
/// 冗余字段（CLI 不暴露）：`bklxdjmc / cxbj / cxbjmc / oldjc / oldzc(疑位 mask) /
/// jgh_id / jxb_id / kch_id / xkbz / cd_id / cdbh / xqh_id / xqh1 / date / dateDigit /
/// day / month / year / queryModel / userModel / pageTotal / pageable / rangeable /
/// row_id / listnav / localeKey / px / sxbj / sfjf / sfkckkb / kkzt / kklxdm / pkbj /
/// xkrs / zzrl / zzmm / zyfxmc / xsdm / xslxbj / zyhxkcbj / qqqh / rk / rsdzjs /
/// njxh / zxs / zxxx / zhxs / kcxszc / kczxs / totalResult`（每条 item 嵌一份 = "0"）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KbItem {
    #[serde(default)]
    pub xnm: Option<String>,
    #[serde(default)]
    pub xqm: Option<String>,
    /// 课程号。
    #[serde(default)]
    pub kch: Option<String>,
    /// 课程名。
    #[serde(default)]
    pub kcmc: Option<String>,
    /// 学分（字符串，不强转）。
    #[serde(default)]
    pub xf: Option<String>,
    /// 课程性质（"必修" / "限选" / "任选"）。
    #[serde(default)]
    pub kcxz: Option<String>,
    /// 课程类别（"专业类教育课程" 等）。
    #[serde(default)]
    pub kclb: Option<String>,
    /// 考核方式（"考试" / "考核"）。
    #[serde(default)]
    pub khfsmc: Option<String>,
    /// 考试方式（"笔试" / "大作业"）。
    #[serde(default)]
    pub ksfsmc: Option<String>,
    /// 教学班名，如 `"(2025-2026-1)-FL1405-01"`。
    #[serde(default)]
    pub jxbmc: Option<String>,
    /// 教学班组成（"2023日语" / 多专业 ";" 分隔）。
    #[serde(default)]
    pub jxbzc: Option<String>,
    /// 教师姓名。
    #[serde(default)]
    pub xm: Option<String>,
    /// 教师职称。
    #[serde(default)]
    pub zcmc: Option<String>,
    /// 主讲身份（如 "主讲"）。
    #[serde(default)]
    pub zfjmc: Option<String>,
    /// 周几（"1".."7"，配 `xqjmc_map` 转中文）。
    #[serde(default)]
    pub xqj: Option<String>,
    /// 周几文本（"星期一" 等）。
    #[serde(default)]
    pub xqjmc: Option<String>,
    /// 节次显示（"3-4" / "1-2节"）。
    #[serde(default)]
    pub jc: Option<String>,
    /// 节次范围数字（"03-04"）。
    #[serde(default)]
    pub jcs: Option<String>,
    /// 节次起止（"3-4"）。
    #[serde(default)]
    pub jcor: Option<String>,
    /// 周次描述（"1-16周" / "1-4周,6-16周" / "2-16周双"）。**需 parser**。
    #[serde(default)]
    pub zcd: Option<String>,
    /// 教室名（"上院 412"）。
    #[serde(default)]
    pub cdmc: Option<String>,
    /// 楼。
    #[serde(default)]
    pub lh: Option<String>,
    /// 教室类别（"多媒体教室" 等）。
    #[serde(default)]
    pub cdlbmc: Option<String>,
    /// 校区名。
    #[serde(default)]
    pub xqmc: Option<String>,
    /// 授课语言（"中文" / "英文"）。
    #[serde(default)]
    pub skfsmc: Option<String>,
}
