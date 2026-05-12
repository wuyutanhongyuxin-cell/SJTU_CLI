//! §2.2 N2151 个人课表查询响应实体（专属 envelope，不复用 `JwcPage`）。

use serde::{Deserialize, Deserializer, Serialize};

/// 反序列化 string ("65535") → Option<u32>。
/// 兼容 N2154 真机返回（`oldzc` / `oldjc` 是 string，不是 number）。
fn deserialize_opt_u32_from_str<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) if s.is_empty() => Ok(None),
        Some(s) => s.parse::<u32>().map(Some).map_err(serde::de::Error::custom),
    }
}

/// §2.2 N2151 课表 envelope（N2154 周课表复用）。
/// 隐藏字段：`xsxx`（学生身份）/ `qsxqj` / `sjkList` / `sjfwkg` / `xskbsfxstkzt`（ZF 噪音）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    /// 周几文本映射 `{"1":"星期一", ..., "7":"星期日"}`，原样转出。
    #[serde(default)]
    pub xqjmc_map: serde_json::Value,
    /// 课表条目（已按周几+节次铺平）。
    #[serde(default)]
    pub kb_list: Vec<KbItem>,
    /// §2.7 N2154 周课表的"周次→日期"映射；N2151 路径永远空 Vec。
    #[serde(default, rename = "rqazcList")]
    pub rqazc_list: Vec<RqAzc>,
}

/// §2.7 N2154 `rqazcList[*]` 单条：周次对应的日期 + 周几。
///
/// N2151 路径下永远空（`serde(default)` 兼容）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RqAzc {
    /// 日期 ISO 字符串（"2025-09-08"）。
    #[serde(default)]
    pub rq: Option<String>,
    /// 周几（真机是 number 1..7，非 string）。
    #[serde(default)]
    pub xqj: Option<u8>,
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
    /// §2.7 N2154 周次 bitmask（真机 string "65535"，parse→u32；N2151 永远 None）。
    #[serde(
        default,
        rename = "oldzc",
        deserialize_with = "deserialize_opt_u32_from_str"
    )]
    pub old_zc: Option<u32>,
    /// §2.7 N2154 节次 bitmask（真机 string "12"，parse→u32；N2151 永远 None）。
    #[serde(
        default,
        rename = "oldjc",
        deserialize_with = "deserialize_opt_u32_from_str"
    )]
    pub old_jc: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_deserializes_n2154_rqazc_list() {
        // 真机 N2154：xqj 是 number (1..7)，rq 是 ISO 字符串
        let json = r#"{
            "xqjmcMap": {"1":"星期一","2":"星期二"},
            "kbList": [],
            "rqazcList": [
                {"rq": "2025-09-08", "xqj": 1},
                {"rq": "2025-09-09", "xqj": 2}
            ]
        }"#;
        let s: Schedule = serde_json::from_str(json).expect("Schedule 解析失败");
        assert_eq!(s.rqazc_list.len(), 2);
        assert_eq!(s.rqazc_list[0].rq.as_deref(), Some("2025-09-08"));
        assert_eq!(s.rqazc_list[0].xqj, Some(1));
    }

    #[test]
    fn kb_item_deserializes_old_zc_old_jc_from_string() {
        // 真机 N2154：oldzc / oldjc 是 string，需 parse 成 u32
        let json = r#"{"kcmc":"高数","oldzc":"524286","oldjc":"12"}"#;
        let k: KbItem = serde_json::from_str(json).expect("KbItem 解析失败");
        assert_eq!(k.old_zc, Some(524286));
        assert_eq!(k.old_jc, Some(12));
    }

    #[test]
    fn kb_item_old_zc_absent_defaults_to_none() {
        // N2151 路径无 oldzc / oldjc
        let json = r#"{"kcmc":"高数"}"#;
        let k: KbItem = serde_json::from_str(json).expect("KbItem 解析失败");
        assert!(k.old_zc.is_none());
        assert!(k.old_jc.is_none());
    }

    #[test]
    fn n2151_response_without_rqazc_list_defaults_to_empty_vec() {
        let json = r#"{"xqjmcMap":{},"kbList":[]}"#;
        let s: Schedule = serde_json::from_str(json).expect("Schedule 解析失败");
        assert!(
            s.rqazc_list.is_empty(),
            "N2151 路径 rqazc_list 必须为空 Vec"
        );
    }
}
