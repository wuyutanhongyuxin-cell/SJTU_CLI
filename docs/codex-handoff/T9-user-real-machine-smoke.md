# T9 · 用户亲跑 4 端真机 smoke

> **codex 不执行此 task**。
> codex 的工作：把本文件作为 checklist 整理后**发给用户**，让用户亲手跑 4 个日历客户端的 import + 重复 import 测试。
> codex 等用户反馈，结果全绿后才**收尾 T5**。

---

## codex 给用户的提示模板

> T7 + T8 完成。最后一步 T9 需要**你**在 4 个日历客户端真机测一遍 import：
>
> 1. 先在 SJTU 网络环境下登录 + 跑：`sjtu jwc calendar --to ~/sjtu_smoke.ics`（或 `~/Desktop/sjtu_smoke.ics`）
> 2. 同时跑 `sjtu jwc calendar --to ~/sjtu_smoke2.ics -o json` 拿 envelope，确认 `event_count > 0` 且 `warnings: []`
> 3. 然后按下面 4 端清单各导一次
> 4. 最后再导一次同一份 `sjtu_smoke.ics`（幂等测试），确认不会有重复事件
>
> 把每端的结果（成功 / 哪个事件显示异常 / 重复 import 是否产生重复事件）回我，我再帮你标完 T5 收尾。

---

## 4 端 import + 验证清单

### 端 1 · Google Calendar（浏览器）

**Import 路径**：
- 打开 `https://calendar.google.com/`
- 左侧 "其他日历" → "+" → "从文件导入"
- 选 `sjtu_smoke.ics` → 选"目标日历"（建议新建一个测试日历，方便回滚）
- 点 "导入" → 看提示 "已导入 N 个事件"

**验证项**：
- [ ] 提示导入的 N 是否等于 envelope 里的 `event_count`
- [ ] 课表事件标题是中文（如 "操作系统"），无乱码
- [ ] 课表事件**时区显示对**（应是 GMT+8 / "上海"）
- [ ] 周重复课显示重复规则（点开事件看 "每周" 或 "每两周"）
- [ ] 考试事件标题前缀 `[考]`
- [ ] 校历整天事件（如 "[校历] 劳动节"）显示为整天，**不是 23:59 那种细长条**
- [ ] **重复 import**：再 import 同一份 `sjtu_smoke.ics`，提示应是 "N 个事件已存在，0 个新增"

**已知 quirk**：Google Calendar 对整天事件的 dtend 处理：dtend `T23:59:00` 可能显示为"晚上 23:59 单点"而非整天。如果遇到，回报给 codex / Claude，可能要把 dtend 改成下一天 `T00:00:00` 或加 `VALUE=DATE` 整天类型。

### 端 2 · Apple Calendar（macOS）

**Import 路径**：
- 打开 Calendar.app
- 菜单：File → Import → 选 `sjtu_smoke.ics`
- 选目标日历 → 点 "OK"

**验证项**：同 Google（[ ] 6 条）

**已知 quirk**：Apple Calendar 在 VTIMEZONE 块格式上比较严格。如果 import 后事件全部偏移 8 小时，说明 VTIMEZONE Asia/Shanghai 块出问题 —— 回报。

### 端 3 · Outlook（Windows / Mac / web）

**Import 路径**（Outlook web）：
- 打开 `https://outlook.live.com/calendar/`
- 设置 → 日历 → 导入日历 → 从文件 → 上传 `sjtu_smoke.ics`

**Import 路径**（Outlook 桌面版）：
- File → Open & Export → Import/Export → 从 iCalendar (.ics) 文件导入

**验证项**：同 Google（[ ] 6 条）

**已知 quirk**：Outlook 对 RRULE COUNT 大于 100 可能截断 —— 但本项目最大 18 周课，应不触发。

### 端 4 · 手机本地日历（iOS / Android）

**Import 路径**（iOS）：
- 把 `sjtu_smoke.ics` 通过 AirDrop / 邮件附件 / 文件 App 打开
- 系统弹出"添加 N 个事件"对话框 → 选目标日历 → "全部添加"

**Import 路径**（Android）：
- 取决于厂商。原生 Google Calendar app 可走 Google Calendar 网页 import 然后同步下来
- 部分国行 ROM（小米 / 华为）有本地导入入口在 文件管理 → 打开 .ics

**验证项**：同 Google（[ ] 6 条）+ 手机推送是否正常（课表事件应在上课前 N 分钟弹通知）

---

## 用户反馈模板

请用户照这个填回 codex：

```
T9 真机 smoke 报告

环境：
- sjtu jwc calendar envelope:
  - event_count: NN
  - by_kind: { class: NN, exam: N, academic: N }
  - warnings: [...]
  - bytes: NNNN

端 1 Google Calendar：[全过 / 异常：xxx]
端 2 Apple Calendar：[全过 / 异常：xxx / 跳过（无设备）]
端 3 Outlook：[全过 / 异常：xxx / 跳过]
端 4 手机本地：[全过 / 异常：xxx / 跳过]

重复 import 幂等：[过 / 失败：xxx]
```

---

## codex 收到反馈后的动作

### Case A · 用户报告"全 4 端过 + 幂等过"

执行最终收尾：

1. 改 `tasks/todo.md`：T5 Plan T9 / T5 整体 改为 `[x]`，加日期与简短结论
2. 改 `tasks/lessons.md`：如果用户报告任何 quirk（端 1/2/3 quirk 段提到的），加进教训段
3. 不 commit 也 OK（小幅）—— 或 commit 一个 docs commit：

```powershell
git add tasks/todo.md tasks/lessons.md
git commit -m @'
docs(jwc): T5 真机 smoke 全 4 端通过，T5 整体完成（T5 Task 9）

用户报告 Google/Apple/Outlook/手机本地 4 端 import 正常，
重复 import 幂等验证通过（0 重复事件）。

T5 校历 iCal 导出 MVP 完工。

Co-Authored-By: codex
'@
```

4. 给用户最终一行：

```
STATUS: T5 ALL DONE
8 个 commit 累计于本地 main 分支，等你 git push。
下一步建议：S3 Phase 2（一卡通明细 / 通知聚合 / 图书馆借阅）或继续 jwc（培养方案查询 / 选课结果只读）。
```

### Case B · 用户报告某端异常

**不要自己改代码**。把异常分类：

- **VTIMEZONE 偏移问题** → 主对话报告，让 Claude 介入设计修复
- **RRULE 不识别** → 主对话报告，可能是端 N 的 quirk
- **整天事件细长条** → 主对话报告，提议 dtend 改为 `T00:00:00 + 1day`
- **重复 import 出现重复事件** → **严重**，UID 算法 bug，立刻报告主对话停手

报告格式：

```
STATUS: BLOCKED
T9: 用户报告 N 端异常
异常详情：<复制用户原话>
建议：<给主对话留判断空间，不擅自下手>
```

### Case C · 用户跳过部分端（如无 Mac）

允许。只要 Google + Outlook + 至少一个移动端通过，就视为 T9 部分通过，按 Case A 收尾，但 lessons.md 标注"未覆盖：Apple Calendar，待后续真机补测"。

---

## 红线提醒（再说一次）

- codex **永远**不真跑 `sjtu jwc calendar`（除非用户主动让你跑）。codex 没 JAccount session，硬跑会失败 + 可能触发 i.sjtu 风控。
- codex **永远**不替用户做 import 操作（codex 没浏览器 / 没手机）。
- codex **永远**不写"为了通过 T9 我先 mock 一下" —— mock 跑通了 ≠ 真机过。T9 必须真机。
