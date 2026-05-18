# SCHEMA.md — SJTU-CLI 输出信封契约

> Envelope schema 版本：v1。变更字段时 bump `SCHEMA_VERSION`（src/output.rs）。

## Envelope 顶层结构

```yaml
ok: true | false                          # 必出
schema_version: "1"                       # 必出，字符串
data: <子命令 payload>                    # ok=true 时出
error:                                    # ok=false 时出
  code: <kebab-case 错误码>
  message: <人读消息，不含 PII>
meta:                                     # 可选，仅多路径子系统填
  via: <实际走的鉴权路径>
  source_hint: <数据源域名>
```

字段语义：

- `ok: bool` — 成功/失败。Agent 解析必读。
- `schema_version: "1"` — 字符串，方便 schema 演进。
- `data` / `error` 互斥：成功只填 data，失败只填 error。
- `meta` 仅多路径子系统消费（当前：`card`）。

## `meta` 字段（v1+，可选）

`meta` 是 `Option<EnvelopeMeta>`，**仅多路径子系统使用**（当前：card 双轨 OAuth2/weixin）。

```yaml
meta:
  via: "oauth2" | "weixin"               # 实际走的鉴权路径
  source_hint: "api.sjtu.edu.cn" | "card.sjtu.edu.cn"   # 数据源域
```

**后向兼容**：现有子命令（elec / shuiyuan / canvas / jwc / services / jwbmessage）不构造 `meta`，JSON 输出**不出现** `meta` 键。Agent 解析时 `meta` 是 optional 字段。
