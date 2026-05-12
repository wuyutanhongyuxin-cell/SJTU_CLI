//! 课表 grid 渲染（comfy-table 包装）。render_grid_day / render_grid_week 输出字符串，
//! 终端宽度自适应（ContentArrangement::Dynamic）；非 TTY 走 YAML/JSON 输出。

use comfy_table::{ContentArrangement, Table};

/// 单日 grid 的一格内容。
pub struct DayCell {
    pub jc_list: Vec<u8>,
    pub kcmc: String,
    pub cdmc: String,
    pub xm: String,
}

/// 整周 grid 的一格内容（含周几）。
pub struct WeekCell {
    pub xqj: u8,
    pub jc_list: Vec<u8>,
    pub kcmc: String,
    pub cdmc: String,
    pub xm: String,
}

/// 渲染单日表格，列：节次 | 课程 | 教室 | 教师。
pub fn render_grid_day(items: &[DayCell]) -> String {
    let mut t = Table::new();
    t.set_content_arrangement(ContentArrangement::Dynamic);
    t.set_header(vec!["节次", "课程", "教室", "教师"]);
    for c in items {
        t.add_row(vec![
            jc_range(&c.jc_list),
            c.kcmc.clone(),
            c.cdmc.clone(),
            c.xm.clone(),
        ]);
    }
    t.to_string()
}

/// 渲染整周表格。
///
/// - `week_dates`: `[(周几文本, ISO 日期), ...]`，长度通常为 7。
/// - `items`: 课程列表，`xqj` 为 1（周一）..7（周日）。
pub fn render_grid_week(week_dates: &[(String, String)], items: &[WeekCell]) -> String {
    let mut t = Table::new();
    t.set_content_arrangement(ContentArrangement::Dynamic);

    let mut header: Vec<String> = vec!["节次".to_string()];
    for (label, date) in week_dates {
        header.push(format!("{label}\n{date}"));
    }
    t.set_header(header);

    // 收集所有出现的节次，去重后升序
    let mut all_jc: Vec<u8> = items
        .iter()
        .flat_map(|c| c.jc_list.iter().copied())
        .collect();
    all_jc.sort_unstable();
    all_jc.dedup();

    for jc in all_jc {
        let mut row: Vec<String> = vec![jc.to_string()];
        for xqj in 1u8..=7 {
            let cell = items
                .iter()
                .find(|c| c.xqj == xqj && c.jc_list.contains(&jc))
                .map(|c| format!("{}\n{}\n{}", c.kcmc, c.cdmc, c.xm))
                .unwrap_or_default();
            row.push(cell);
        }
        t.add_row(row);
    }
    t.to_string()
}

/// 将连续节次列表压缩为 "1-3" 形式；不连续保持逗号分隔。
fn jc_range(jcs: &[u8]) -> String {
    if jcs.is_empty() {
        return String::new();
    }
    let mut s: Vec<u8> = jcs.to_vec();
    s.sort_unstable();
    let consecutive = s.windows(2).all(|w| w[1] == w[0] + 1);
    if consecutive && s.len() > 1 {
        format!("{}-{}", s.first().unwrap(), s.last().unwrap())
    } else {
        s.iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_grid_day_with_two_courses_outputs_non_empty_string() {
        let items = vec![
            DayCell {
                jc_list: vec![1, 2],
                kcmc: "高数".into(),
                cdmc: "东上 101".into(),
                xm: "张".into(),
            },
            DayCell {
                jc_list: vec![3, 4],
                kcmc: "英语".into(),
                cdmc: "西中 202".into(),
                xm: "李".into(),
            },
        ];
        let out = render_grid_day(&items);
        assert!(out.contains("高数"));
        assert!(out.contains("英语"));
        assert!(out.contains("1-2") || out.contains("1, 2"));
    }

    #[test]
    fn render_grid_week_with_empty_items_outputs_header_only() {
        let dates = vec![
            ("周一".into(), "2026-05-11".into()),
            ("周二".into(), "2026-05-12".into()),
        ];
        let out = render_grid_week(&dates, &[]);
        assert!(out.contains("周一"));
        assert!(out.contains("周二"));
    }
}
