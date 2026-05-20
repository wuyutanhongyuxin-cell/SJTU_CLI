# Session 恢复快照 — 2026-05-19 重启前

> **目的**：用户重启电脑前完整记录"现在在做什么、做到哪一步、下一步具体动作"。
> 重启后任意一轮 Claude 直接读这一份就能无缝继续。

---

## 1. 最后一次代码状态（已 git push 到 origin/main）

| commit | 内容 |
|---|---|
| `3306c05` | **P1 truncate UTF-8 修** — 6 个子系统 (`elec` / `services` / `jwbmessage` ×2 / `canvas` / `shuiyuan` / `card` 已早修过) 的错误页截断由 `&s[..max]` 字节切片改为 `chars().take(max)`，避中文 panic；+6 边界单测 |
| `dd604c7` | **P2 parse 文件拆 sibling tests** — `card/weixin/balance_parse.rs` 217 → 100 行，`history_parse.rs` 262 → 139 行；测试搬到 `balance_parse_tests.rs` / `history_parse_tests.rs` 兄弟文件；`weixin/mod.rs` 加 `#[cfg(test)] mod *_tests;` 挂载 |

**验证**：`cargo test --lib` 327 passed / 0 failed；`cargo clippy --all-targets -- -D warnings` 零警告；`cargo fmt --check` 零 diff。

**未完待办（下次"清理一下"再说）**：
- `src/apps/card/weixin/mod.rs` 现 242 行，又超 200 硬限（拆 `weixin_follow` 到独立 `redirect.rs` 是建议方向）
- `elec/services/jwbmessage` 接入 CAS retry 层（spec NG1 暂留）

---

## 2. 当前阶段：S3 Phase 2 续 — 图书馆借阅子系统（library）

**用户决策**（2026-05-19 本轮对话）：
- 跳过 CP-WX-真机 5 项（D12 weixin path 上线，离线 fixture 全过；真机校验之后用户自跑）
- 跳过 T4-OAuth2 真机 CP（client_id 仍审批阻塞）
- **新方向**：开图书馆借阅 → 接着开 read-only 邮箱（两个独立 phase，library 先）

### 2.1 进度：L0/8 步刚启动，未跑任何命令

8 步计划（已建 TaskCreate #9-#14，重启后丢失要重建）：

| # | 状态 | 内容 | 谁跑 |
|---|---|---|---|
| **L0** | ⏳ **下一步要跑** | 真机侦察 4 unknowns | 用户校园网内跑 |
| L1 | 待开 | 写 plan 文档 `docs/superpowers/plans/2026-05-19-t7-library-loans.md` | Claude |
| L2 | 待开 | `src/apps/library/` 模块骨架（6 文件，mockito 单测） | Claude |
| L3 | 待开 | `src/commands/library/` + `src/cli/library.rs` | Claude |
| L4 | 待开 | lib.rs / cli.rs 接线 + fmt/clippy/test 全绿 | Claude |
| L5 | 待开 | 文档同步 + 真机 CP-L1/L2/L3 | 用户校园网内跑 |

---

## 3. L0 真机侦察具体动作（重启后第一件事）

### 3.1 — 10 秒 curl 探测 CAS 入口（解 B1 入口域 + B2 进入方式）

PowerShell 贴这 4 条，把输出回贴给 Claude：

```powershell
# 1. 主域是不是直接 OAuth2 入口
curl.exe -s -i --max-redirs 0 "https://www.lib.sjtu.edu.cn/" | Select-Object -First 20

# 2. 抓主域 HTML 里的 jaccount/oauth 锚点（10 秒定 entry）
curl.exe -s "https://www.lib.sjtu.edu.cn/" | Select-String -Pattern "jaccount|oauth|openid|saml|login" -CaseSensitive:$false

# 3. mylib 子域试探（预研提到的候选）
curl.exe -s -i --max-redirs 0 "https://mylib.lib.sjtu.edu.cn/" | Select-Object -First 20

# 4. ilink 候选
curl.exe -s -i --max-redirs 0 "https://ilink.lib.sjtu.edu.cn/" | Select-Object -First 20
```

**lessons 引用**：`tasks/lessons.md` 2026-05-13 那条"新 SP 调研第一步：curl + grep jaccount/oauth/openid 锚点"。

### 3.2 — chrome-devtools 抓借阅端点（解 B3 端点路径 + B4 cookie 名）

仅在 L0.1 定位到 entry 后做。**严格只读：不点续借、不点预约、不动个人信息任何字段**（CLAUDE.md 硬红线）。

SOP：
1. 打开 DevTools Network 面板，勾「Preserve log」+ filter「Fetch/XHR」
2. 浏览器登录 library，进「我的图书馆 → 借阅列表」
3. 把借阅列表那条 XHR 的 **URL / method / response body** 截图或复制给 Claude
4. 如果有「借阅历史」独立 tab，同样抓一条
5. 用 `document.cookie` 看 library 域下 session cookie 名

---

## 4. 关键参考文件（重启后 Claude 必读）

| 文件 | 用途 |
|---|---|
| `docs/superpowers/research/2026-05-15-t6-library-prerequisites.md` | **预研主文档**，含目录骨架 / 4 unknowns / 真机 SOP / 200 行预算 / T7-T8 耦合 |
| `src/apps/elec/mod.rs` | 最近最像 library 的子系统，模板复用 |
| `tasks/lessons.md` 2026-05-13 条 | "新 SP curl + grep 锚点"规则 |
| `tasks/lessons.md` 2026-04-26 条 | ZF 类 SP 入口必须用 `/jaccountlogin` 而非深页；library 是否走 ZF 模式 L0 时验证 |

---

## 5. 4 个 unknowns（L0 之后此节回填）

| # | Unknown | 当前状态 |
|---|---------|----------|
| B1 | 入口域名（www.lib / mylib / ilink / 其它） | ⏳ 未知 |
| B2 | 进入方式（CAS / OAuth2 / 独立 / ZF 内 jAccount 锚点） | ⏳ 未知 |
| B3 | 借阅列表端点路径 + 分页参数 + 响应格式（JSON/HTML） | ⏳ 未知 |
| B4 | Session cookie 名（JSESSIONID / SESSION / 其它） | ⏳ 未知 |

---

## 6. 复用约束（已 locked，不动）

- ✅ CAS retry layer（T8，5-16 已合并）— `cas_login_with_retry("library", LOGIN_URL)` 一行得到 staleness-aware session
- ✅ Envelope 泛型 — `output::Envelope<T>` 同 elec
- ✅ Decimal 金额 — 罚款 / 押金字段一律 `rust_decimal::Decimal` + `serialize_str`
- ✅ NaiveDate 日期 — `LoanRecord.due_date: Option<chrono::NaiveDate>` (T7 通知聚合预留)
- ✅ 200 行硬限 — 预估总 ~545 行 / 11 文件，每文件 < 200
- ✅ 红线 — 不实装 续借 / 预约 / 取消预约（写端点）

---

## 7. 重启后 Claude 启动指令模板

把这段贴给 Claude 即可恢复：

> 我重启完了。读 `tasks/session-2026-05-19-restart-snapshot.md` 接着上次的活——library 子系统 L0 真机侦察。我现在 PowerShell 贴 §3.1 的 4 条 curl 命令，输出贴回给你后我们解 B1/B2 再决定 L0.2 chrome-devtools 怎么抓。

---

## 8. 邮箱子系统（library 完成后再开，本快照不展开）

用户要求"严格准确安全 / 不暴露信息"。开工前需独立 brainstorm：
- 认证选型：IMAP / Microsoft Graph / Web scrape 三选一
- 隐私边界：标题脱敏？发件人脱敏？正文落本地？unread count only？
- client_id 审批是否阻塞（同 T14）

不在本快照范围。
