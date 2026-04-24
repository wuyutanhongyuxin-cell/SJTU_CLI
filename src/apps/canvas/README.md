# apps/canvas

## 用途
Canvas LMS（`oc.sjtu.edu.cn`）只读客户端。封装 MVP 3 个 API 端点，支撑 `sjtu canvas today` / `upcoming` / `whoami` 三条命令。

## 认证模型
Personal Access Token（PAT）—— 用户手动在 Canvas `设置 → 访问许可证` 生成，`sjtu canvas setup` 交互式粘贴后落盘到 `<config_dir>/sub_sessions/canvas_token.txt`（600 权限）。不走 SSO / cookie。

## 文件清单
- `mod.rs` — 模块入口，re-export `Client` / `PlannerItem` / `Profile` / `Submissions` / `Plannable`
- `api.rs` — `Client` struct + `connect()` + `whoami()` + `planner_items()`
- `http.rs` — reqwest Client 构造（注入 Bearer + 固定 header）+ `fetch_json`（节流 + 重试 + 401 映射）
- `auth.rs` — PAT 文件 I/O（`load_pat` / `save_pat` / `clear_pat`）
- `models.rs` — `UserSelf` / `UserProfile` / `Profile` / `PlannerItem` / `Plannable` / `Submissions`
- `throttle.rs` — 300 ms 固定间隔节流器（与 shuiyuan / jwbmessage 同策略）
- `tests_parse.rs` — serde 解析单测 + throttle 行为单测

## 依赖关系
- 依赖：`crate::config`（路径解析）、`crate::error`（`SjtuCliError::{NotAuthenticated, SessionExpired, CanvasApi, ...}`）、`reqwest` / `tokio` / `serde`
- 被依赖：`crate::commands::canvas`（所有 handler）、`crate::cli::canvas`（dispatch）

## 端点契约（详见 `tasks/s3c-canvas-planner.md §2`）
| 用途 | 方法 | 路径 | 关键参数 |
|---|---|---|---|
| whoami | GET | `/api/v1/users/self` + `/api/v1/users/self/profile` | — |
| today / upcoming | GET | `/api/v1/planner/items` | `start_date=<UTC>` + `end_date=<UTC>` + `order=asc` + `per_page=100` |

固定请求头：`Authorization: Bearer <PAT>` + `Accept: application/json+canvas-string-ids, application/json` + `X-Requested-With: XMLHttpRequest`。
