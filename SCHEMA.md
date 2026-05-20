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

## library —— 图书馆借阅（weijieyue.lib.sjtu.edu.cn:8080）

**子命令**：`loans` / `history` / `fines`，均只读。

**Envelope.meta**：
```yaml
meta:
  via: weijieyue
  source_hint: weijieyue.lib.sjtu.edu.cn:8080
```

### library loans

```yaml
data:
  count: 2
  items:
    - title: "Rust 编程之道"
      isbn: "9787121327971"
      barcode: "B1234567"
      borrow_date: "2026-04-15"
      due_date: "2026-06-15"
      renew_times: 0
      location: "包玉刚图书馆"
```

### library history

```yaml
data:
  count: 1
  items:
    - title: "算法导论"
      isbn: "9787111407010"
      borrow_date: "2025-09-01"
      return_date: "2025-11-01"
      location: "主馆"
```

### library fines

```yaml
data:
  count: 1
  pending_count: 1
  items:
    - title: "数据结构"
      isbn: "9787302464710"
      fine_sum: "5.00"
      status: "待缴纳"
      fine_date: "2026-04-20"
      sequence: "F20260420001"
```

**红线**：永不实装续借 / 缴费 / 取消等写端点（参见 plan 文档 §红线契约）。
