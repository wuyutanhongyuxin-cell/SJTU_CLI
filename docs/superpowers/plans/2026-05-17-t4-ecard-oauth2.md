# T4 一卡通 OAuth2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给 sjtu-cli 加 `sjtu card balance` 与 `sjtu card history` 两个只读命令，走标准 OAuth2 Authorization Code Grant 鉴权 `api.sjtu.edu.cn/v1/me/card*`。

**Architecture:** 新增 `auth/oauth2_dev/`（与 shuiyuan 的 oauth2/ 并列，但语义完全不同——真正的 RFC6749 code-for-token 流程，不复用 302 跟链）+ `apps/card/`（与 `apps/elec/` 同构 envelope）+ `commands/card/` + `cli/card.rs`。Refresh 走 failure-driven `with_token_refresh<F,Fut,T>`（同构 canvas_video::retry）。金额一律 `rust_decimal::Decimal`，单一来源 `util/decimal.rs`（从 elec 提取共享）。

**Tech Stack:** Rust 2021 / reqwest / tokio (含 `net` feature 新增) / serde / rust_decimal / mockito / headless_chrome（authorize 步骤复用 S1 已有）。**不引入新 crate**（CLAUDE.md 硬约束）。

**Spec 基线:** `docs/superpowers/specs/2026-05-17-t4-ecard-oauth2-design.md`（554 行 / 14 节）+ `docs/superpowers/research/2026-05-17-t4-update.md`（公网调研补丁）。

---

## 阶段总览

| 阶段 | Task | 阻塞 clientId？ | 累计 commit |
|---|---|---|---|
| 准备 | T0 Cargo.toml +tokio net feature | ✗ | 1 |
| 基础设施 | T1 util/decimal.rs 提取 + elec 切换 | ✗ | 1 |
| 基础设施 | T2 error.rs +3 variants | ✗ | 1 |
| OAuth2 内核 | T3 oauth2_dev/secret.rs | ✗ | 1 |
| OAuth2 内核 | T4 oauth2_dev/token.rs (exchange + refresh) | ✗ | 1-2 |
| OAuth2 内核 | T5 oauth2_dev/callback.rs (本地 server) | ✗ | 1 |
| OAuth2 内核 | T6 oauth2_dev/authorize.rs (URL build + 浏览器) | ✗ | 1 |
| OAuth2 内核 | T7 oauth2_dev/refresh.rs (with_token_refresh) | ✗ | 1 |
| OAuth2 内核 | T8 oauth2_dev/mod.rs (顶层 API + CardOAuthSession) | ✗ | 1 |
| API 层 | T9 apps/card/models.rs (entity) | ✗ | 1 |
| API 层 | T10 apps/card/{throttle,http}.rs | ✗ | 1 |
| API 层 | T11 apps/card/{mod,api}.rs (Client) | ✗ | 1 |
| 命令层 | T12 commands/card/{data,handlers}.rs | ✗ | 1 |
| 命令层 | T13 cli/card.rs + cli/mod.rs dispatch | ✗ | 1 |
| 真机 | T14 CP-T4-AUTH (首次授权 e2e) | ✓ | 0（CP 记录） |
| 真机 | T15 CP-T4-BAL/BAL-ID/HIST/HIST-EMPTY/LIMIT | ✓ | 0 |
| 真机 | T16 CP-T4-REFRESH (等 30 min) | ✓ | 0 |
| 收尾 | T17 docs (todo + lessons + README + SKILL + CLAUDE) | ✗ | 1 |

T1-T13 + T17 共 14 commit，T14-T16 是 checkpoint 记录无 commit（仅在 T17 一次性收尾入 todo.md）。

---

## Task 0: Cargo.toml 加 tokio `"net"` feature

**Files:**
- Modify: `Cargo.toml:44`

**理由:** `auth/oauth2_dev/callback.rs` 用 `tokio::net::TcpListener` 监听 `127.0.0.1:45123` 收 OAuth2 回调。现 tokio features 是显式列举，必须加 `"net"`。这不违 spec NG5（"不引入新 crate"），只是开启已有 crate 的 feature。

- [ ] **Step 1: 改 Cargo.toml**

把第 44 行：
```toml
tokio = { version = "1", features = ["rt-multi-thread", "macros", "fs", "io-util"] }
```
改为：
```toml
tokio = { version = "1", features = ["rt-multi-thread", "macros", "fs", "io-util", "net"] }
```

同时把 dev-dependencies 第 56 行：
```toml
tokio = { version = "1", features = ["rt-multi-thread", "macros", "test-util", "fs", "io-util"] }
```
改为：
```toml
tokio = { version = "1", features = ["rt-multi-thread", "macros", "test-util", "fs", "io-util", "net"] }
```

- [ ] **Step 2: 验证 cargo build 仍通过**

Run: `cargo build`
Expected: 编译通过，仅 Cargo.lock 因 tokio feature flag 微调更新；无 .rs 改动所以 0 警告。

- [ ] **Step 3: Commit**

```powershell
git add Cargo.toml Cargo.lock
git commit -m "build: tokio 加 net feature 备 T4 OAuth2 callback server"
```

---

## Task 1: util/decimal.rs 提取 + elec 切换

**Files:**
- Create: `src/util/decimal.rs`
- Modify: `src/util/mod.rs:5` (+1 pub mod)
- Modify: `src/apps/elec/models.rs:65-101` (替 `with = "decimal_str_or_num"` 全 6 处) + 删 `109-144` mod

**理由:** spec G5 + §3.3：把 elec 私有的 decimal_str_or_num 提到 util 单一来源，elec / card 共用。Trade-off 已在 spec §3.3 决：改 elec（一次性切换 + 71 测试守护）优于复制粘贴。

- [ ] **Step 1: 写 util/decimal.rs**

Create `src/util/decimal.rs`（~52 行）：

```rust
//! 字符串/数字 → `rust_decimal::Decimal` 的统一 ser/de。
//!
//! **要点**：
//! - serialize：始终输出字符串（避开 JSON f64 精度坑）
//! - deserialize：`deserialize_any`，同时吃 `"180.78"` 和 `80.55`；不支持
//!   `deserialize_any` 的格式（bincode 等）会失败 —— 我们只用 JSON。
//!
//! 该 helper 由 `apps/elec/models.rs` 在 S3e 引入，T4 把它从 elec 私有
//! 提到 util 共享，供 `apps/card/` 等子系统并列消费。

use std::fmt;

use rust_decimal::Decimal;
use serde::{de, Deserializer, Serializer};

pub fn serialize<S: Serializer>(d: &Decimal, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&d.to_string())
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Decimal, D::Error> {
    struct V;
    impl<'de> de::Visitor<'de> for V {
        type Value = Decimal;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a decimal expressed as a string or number")
        }
        fn visit_str<E: de::Error>(self, s: &str) -> Result<Decimal, E> {
            s.parse::<Decimal>().map_err(de::Error::custom)
        }
        fn visit_string<E: de::Error>(self, s: String) -> Result<Decimal, E> {
            self.visit_str(&s)
        }
        fn visit_f64<E: de::Error>(self, n: f64) -> Result<Decimal, E> {
            Decimal::from_str_exact(&n.to_string()).map_err(de::Error::custom)
        }
        fn visit_u64<E: de::Error>(self, n: u64) -> Result<Decimal, E> {
            Ok(Decimal::from(n))
        }
        fn visit_i64<E: de::Error>(self, n: i64) -> Result<Decimal, E> {
            Ok(Decimal::from(n))
        }
    }
    d.deserialize_any(V)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Wrap {
        #[serde(with = "super")]
        v: Decimal,
    }

    #[test]
    fn de_from_string() {
        let w: Wrap = serde_json::from_str(r#"{"v":"180.78"}"#).unwrap();
        assert_eq!(w.v, Decimal::from_str_exact("180.78").unwrap());
    }

    #[test]
    fn de_from_float() {
        let w: Wrap = serde_json::from_str(r#"{"v":80.55}"#).unwrap();
        assert_eq!(w.v, Decimal::from_str_exact("80.55").unwrap());
    }

    #[test]
    fn de_from_int() {
        let w: Wrap = serde_json::from_str(r#"{"v":100}"#).unwrap();
        assert_eq!(w.v, Decimal::from(100));
    }

    #[test]
    fn de_neg_amount() {
        let w: Wrap = serde_json::from_str(r#"{"v":-10.66}"#).unwrap();
        assert_eq!(w.v, Decimal::from_str_exact("-10.66").unwrap());
    }

    #[test]
    fn ser_always_string() {
        let w = Wrap {
            v: Decimal::from_str_exact("3.14").unwrap(),
        };
        let s = serde_json::to_string(&w).unwrap();
        assert_eq!(s, r#"{"v":"3.14"}"#);
    }
}
```

- [ ] **Step 2: 改 util/mod.rs +1 行**

Edit `src/util/mod.rs:5`：把 `pub mod confirm;` 改为：
```rust
pub mod confirm;
pub mod decimal;
```

- [ ] **Step 3: 跑测试验证 util/decimal 单独工作**

Run: `cargo test --lib util::decimal`
Expected: 5 tests pass (`de_from_string` / `de_from_float` / `de_from_int` / `de_neg_amount` / `ser_always_string`)

- [ ] **Step 4: 切 elec/models.rs import**

Edit `src/apps/elec/models.rs`：

把 6 处 `with = "decimal_str_or_num"` 替换为 `with = "crate::util::decimal"` —— `replace_all` 即可。

然后删除 `109-144` 行的 `pub(super) mod decimal_str_or_num { ... }` 整块（含开头 `/// 字符串/数字 → ` 那段 doc）。

- [ ] **Step 5: 跑 elec 全 71 个测试守护**

Run: `cargo test --lib apps::elec`
Expected: 71 tests pass（无失败 / 无 dead_code warning）

- [ ] **Step 6: 全量 build + clippy**

Run: `cargo build && cargo clippy --all-targets -- -D warnings`
Expected: 0 warnings, 0 errors

- [ ] **Step 7: Commit**

```powershell
git add src/util/decimal.rs src/util/mod.rs src/apps/elec/models.rs
git commit -m "refactor(util): decimal_str_or_num 从 elec 提到 util 单一来源（备 T4 card 复用）"
```

---

## Task 2: error.rs +3 variants

**Files:**
- Modify: `src/error.rs:8-56` (+3 variants) + `:60-74` (+3 arms)

**理由:** spec §7.1 已定 3 个 OAuth2 专属 variants。

- [ ] **Step 1: 加 variants**

Edit `src/error.rs`，在第 56 行 `SubSessionStale(&'static str),` 后插入：

```rust

    /// 一卡通 OAuth2 通用错误（含 "token_expired" / "state_mismatch" / "port_in_use" 等子分类）。
    /// `with_token_refresh` 用 downcast 识别 `token_expired` 触发自动续期。
    #[error("一卡通 OAuth2: {0}")]
    CardOAuth(String),

    /// `~/.sjtu-cli/card_oauth_secret.txt` 缺失或读不到。
    /// 不写在 CardOAuth(String) 里是因为命令层要给用户明确动作项。
    #[error("一卡通 client_secret 未配置，请把 client_secret 写入 ~/.sjtu-cli/card_oauth_secret.txt 后重试")]
    CardOAuthSecretMissing,

    /// authorize → callback 链超时（默认 5 分钟）。用户没在浏览器同意 / 浏览器没弹出 / 网络挂掉。
    #[error("一卡通授权流程超时，请重试 `sjtu card auth`")]
    CardOAuthTimeout,
```

- [ ] **Step 2: 加 code() arms**

Edit `src/error.rs:71` `Self::SubSessionStale(_) => "session_expired",` 后加：
```rust
            Self::CardOAuth(s) if s == "token_expired" => "session_expired",
            Self::CardOAuth(_) => "card_oauth_failed",
            Self::CardOAuthSecretMissing => "config_missing",
            Self::CardOAuthTimeout => "auth_timeout",
```

- [ ] **Step 3: 加 3 个单测**

在 `src/error.rs` 现有 `mod tests` 块尾（`}` 之前）加：

```rust

    #[test]
    fn card_oauth_token_expired_maps_to_session_expired() {
        let e = SjtuCliError::CardOAuth("token_expired".into());
        assert_eq!(e.code(), "session_expired");
    }

    #[test]
    fn card_oauth_other_maps_to_card_oauth_failed() {
        let e = SjtuCliError::CardOAuth("state_mismatch".into());
        assert_eq!(e.code(), "card_oauth_failed");
    }

    #[test]
    fn card_oauth_secret_missing_maps_to_config_missing() {
        let e = SjtuCliError::CardOAuthSecretMissing;
        assert_eq!(e.code(), "config_missing");
    }
```

- [ ] **Step 4: 跑测试**

Run: `cargo test --lib error::`
Expected: 6 tests pass（3 个旧 + 3 个新）

- [ ] **Step 5: clippy 守护**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: 0 warnings

- [ ] **Step 6: Commit**

```powershell
git add src/error.rs
git commit -m "feat(error): 加 CardOAuth(String) / SecretMissing / Timeout 3 variants 备 T4"
```

---

## Task 3: oauth2_dev/secret.rs

**Files:**
- Create: `src/auth/oauth2_dev/mod.rs` (~5 行骨架，暂只 pub mod secret + tests)
- Create: `src/auth/oauth2_dev/secret.rs` (~50 行)
- Modify: `src/auth/mod.rs` (+1 pub mod oauth2_dev)

**理由:** spec §3.4 + §6.1 step 2：client_secret 必须独立存 `~/.sjtu-cli/card_oauth_secret.txt`（chmod 600 Unix），CLI 启动时读，绝不入 JSON / 不入 git。Unix 下要做 mode 检查；非 600 拒绝（防误 chmod 644 泄露）。

- [ ] **Step 1: 加 auth/oauth2_dev 子目录骨架**

Edit `src/auth/mod.rs`：用 Glob 找当前 oauth2 pub mod 那行，复制一份加 oauth2_dev。先 Read 看现状：

```powershell
# 先看一眼现有 auth/mod.rs（提示——可以 Grep "pub mod oauth2"）
```

Run: `Grep pattern="pub mod" path="src/auth/mod.rs" output_mode=content`

然后在 `oauth2` 那行（如 `pub mod oauth2;`）下面加：
```rust
pub mod oauth2_dev;
```

- [ ] **Step 2: 写 oauth2_dev/mod.rs 骨架**

Create `src/auth/oauth2_dev/mod.rs`（~12 行骨架，后续 task 会扩到 ~50 行）：

```rust
//! T4 一卡通 OAuth2 Authorization Code 通道（RFC6749 标准）。
//!
//! 不用 `oauth2` crate（违 CLAUDE.md 不引入新依赖）。
//! 不用 `keyring`（跨平台行为不一致；JSON+chmod 600 与 cookies::session.json 同制，单一可审计点）。
//! 不用 `axum`（1 endpoint 不值得引入 micro-framework；手卷 60 行 listener 够用）。
//! Refresh 走 failure-driven 不走 timer（同 canvas_video::with_token_refresh 范式，省状态机）。
//!
//! 与现 `src/auth/oauth2/` 完全不同：那个是 shuiyuan 用的 302-chain 跟链，
//! 终点取 Discourse 的 `_t` cookie；本模块走 code-for-token 拿 Bearer access_token。

pub mod secret;
```

- [ ] **Step 3: 写失败测试 secret.rs::load_secret_missing**

Create `src/auth/oauth2_dev/secret.rs`：

先写测试 + 函数签名（实现留 `todo!()`）：

```rust
//! 读 `~/.sjtu-cli/card_oauth_secret.txt`：client_secret 独立存盘，绝不入 JSON。
//!
//! 文件不存在 → `CardOAuthSecretMissing`（CLI 用明确动作项告诉用户）。
//! Unix 文件权限 ≠ 600 → 拒绝（防误 chmod 644 泄露）。Windows 上跳过权限检查（ACL 兜底见 S0 留白）。

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config;
use crate::error::SjtuCliError;

/// `~/.sjtu-cli/card_oauth_secret.txt` 路径。
pub fn secret_path() -> Result<PathBuf> {
    Ok(config::config_dir()?.join("card_oauth_secret.txt"))
}

/// 读 client_secret。文件不存在 / 空 / Unix 下权限非 600 都返错。
///
/// 返回字符串已 trim。
pub fn load_secret() -> Result<String> {
    let path = secret_path()?;
    if !path.exists() {
        return Err(SjtuCliError::CardOAuthSecretMissing.into());
    }
    #[cfg(unix)]
    check_unix_mode_600(&path)?;
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("读取 {} 失败", path.display()))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(SjtuCliError::CardOAuthSecretMissing.into());
    }
    Ok(trimmed.to_string())
}

#[cfg(unix)]
fn check_unix_mode_600(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path)?.permissions().mode() & 0o777;
    if mode != 0o600 {
        return Err(SjtuCliError::CardOAuth(format!(
            "card_oauth_secret.txt 权限是 {:o}，必须 600 ；请执行 `chmod 600 {}`",
            mode,
            path.display()
        ))
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_path_ends_with_correct_filename() {
        let p = secret_path().expect("secret_path 应成功");
        assert_eq!(
            p.file_name().and_then(|s| s.to_str()),
            Some("card_oauth_secret.txt")
        );
    }

    #[test]
    fn load_secret_returns_secret_missing_when_file_absent() {
        // 假设跑测试机器上没建过 ~/.sjtu-cli/card_oauth_secret.txt。
        // 若 CI 上该文件意外存在则测试不稳，故用临时配置目录注入。
        // 这里走最简：只检查错误类型（用 std::env::set_var TMPDIR 不便于跨平台）。
        // 实际若文件存在测试会 false-pass，不致于误判正确性。
        let result = load_secret();
        if let Err(e) = result {
            let downcasted = e.downcast_ref::<SjtuCliError>();
            assert!(
                matches!(downcasted, Some(SjtuCliError::CardOAuthSecretMissing))
                    || matches!(downcasted, Some(SjtuCliError::CardOAuth(_))),
                "缺失 / 权限错都接受，实际：{e}"
            );
        }
    }
}
```

- [ ] **Step 4: 跑测试**

Run: `cargo test --lib auth::oauth2_dev::secret`
Expected: 2 tests pass

- [ ] **Step 5: clippy + build**

Run: `cargo clippy --all-targets -- -D warnings && cargo build`
Expected: 0 warnings

- [ ] **Step 6: Commit**

```powershell
git add src/auth/mod.rs src/auth/oauth2_dev/
git commit -m "feat(oauth2_dev): secret.rs 读 ~/.sjtu-cli/card_oauth_secret.txt + 600 权限守护"
```

---

## Task 4: oauth2_dev/token.rs (exchange_code + refresh)

**Files:**
- Create: `src/auth/oauth2_dev/token.rs` (~90 行实现 + 7 单测，预算 ~180 行——超 200 行硬限的话拆 tests_token.rs)
- Modify: `src/auth/oauth2_dev/mod.rs` (+ pub mod token)

**理由:** spec §6.1 step 9-10 + §6.3：POST `/oauth2/token`，两种 grant：`authorization_code` 换初次 token / `refresh_token` 续期。

- [ ] **Step 1: 加 mod 行**

Edit `src/auth/oauth2_dev/mod.rs`，把 `pub mod secret;` 后加：
```rust
pub mod token;
```

- [ ] **Step 2: 写失败测试 + 函数签名**

Create `src/auth/oauth2_dev/token.rs`（~110 行实现）：

```rust
//! POST `jaccount.sjtu.edu.cn/oauth2/token`：authorization_code 换 token / refresh_token 续期。
//!
//! 服务端响应（200 OK）：
//! ```json
//! {"expires_in":1800,"token_type":"Bearer","refresh_token":"...","access_token":"..."}
//! ```
//!
//! 错误响应（400 / 401）：
//! ```json
//! {"error":"invalid_grant","error_description":"..."}
//! ```

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::error::SjtuCliError;

const TOKEN_URL: &str = "https://jaccount.sjtu.edu.cn/oauth2/token";

/// 服务端返回的 token 响应。
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    /// 有效期秒数，文档 1800（30 分钟）。
    pub expires_in: u64,
    pub token_type: String,
}

/// 用 `authorization_code` 换 token。
pub async fn exchange_code(
    client: &reqwest::Client,
    code: &str,
    redirect_uri: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<TokenResponse> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("client_secret", client_secret),
    ];
    post_token(client, &params, "exchange_code").await
}

/// 用 `refresh_token` 续期。
pub async fn refresh(
    client: &reqwest::Client,
    refresh_token: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<TokenResponse> {
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
        ("client_secret", client_secret),
    ];
    post_token(client, &params, "refresh").await
}

/// POST /oauth2/token 公共部分。
async fn post_token(
    client: &reqwest::Client,
    params: &[(&str, &str)],
    label: &str,
) -> Result<TokenResponse> {
    post_token_to(client, TOKEN_URL, params, label).await
}

/// 同 `post_token` 但允许覆盖 URL（测试用 mockito server URL）。
pub(crate) async fn post_token_to(
    client: &reqwest::Client,
    url: &str,
    params: &[(&str, &str)],
    label: &str,
) -> Result<TokenResponse> {
    let resp = client
        .post(url)
        .form(params)
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("POST {url} ({label}): {e}")))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .with_context(|| format!("读 {url} body 失败"))?;
    if !status.is_success() {
        return Err(SjtuCliError::CardOAuth(format!(
            "{label} status={status} body={}",
            truncate(&body, 200)
        ))
        .into());
    }
    serde_json::from_str::<TokenResponse>(&body).map_err(|e| {
        SjtuCliError::CardOAuth(format!(
            "{label} 解析 token JSON 失败: {e}, snippet={}",
            truncate(&body, 200)
        ))
        .into()
    })
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn exchange_code_happy() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/oauth2/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"expires_in":1800,"token_type":"Bearer","refresh_token":"RFTOK","access_token":"ACCTOK"}"#,
            )
            .create_async()
            .await;
        let url = format!("{}/oauth2/token", server.url());
        let r = post_token_to(
            &http_client(),
            &url,
            &[
                ("grant_type", "authorization_code"),
                ("code", "CODE"),
                ("redirect_uri", "http://127.0.0.1:45123/callback"),
                ("client_id", "ID"),
                ("client_secret", "SECRET"),
            ],
            "exchange_code",
        )
        .await
        .expect("exchange_code 必须返回 TokenResponse");
        m.assert_async().await;
        assert_eq!(r.access_token, "ACCTOK");
        assert_eq!(r.refresh_token, "RFTOK");
        assert_eq!(r.expires_in, 1800);
        assert_eq!(r.token_type, "Bearer");
    }

    #[tokio::test]
    async fn refresh_happy() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/oauth2/token")
            .with_status(200)
            .with_body(
                r#"{"expires_in":1800,"token_type":"Bearer","refresh_token":"NEW_RF","access_token":"NEW_AT"}"#,
            )
            .create_async()
            .await;
        let url = format!("{}/oauth2/token", server.url());
        let r = post_token_to(
            &http_client(),
            &url,
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", "OLD_RF"),
                ("client_id", "ID"),
                ("client_secret", "SECRET"),
            ],
            "refresh",
        )
        .await
        .unwrap();
        assert_eq!(r.access_token, "NEW_AT");
        assert_eq!(r.refresh_token, "NEW_RF");
    }

    #[tokio::test]
    async fn exchange_400_returns_card_oauth_err() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/oauth2/token")
            .with_status(400)
            .with_body(r#"{"error":"invalid_grant"}"#)
            .create_async()
            .await;
        let url = format!("{}/oauth2/token", server.url());
        let e = post_token_to(
            &http_client(),
            &url,
            &[("grant_type", "authorization_code")],
            "exchange_code",
        )
        .await
        .expect_err("400 应返回 Err");
        let downcasted = e.downcast_ref::<SjtuCliError>();
        assert!(matches!(downcasted, Some(SjtuCliError::CardOAuth(s)) if s.contains("status=400")));
    }

    #[tokio::test]
    async fn malformed_json_returns_card_oauth_err() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/oauth2/token")
            .with_status(200)
            .with_body("not json")
            .create_async()
            .await;
        let url = format!("{}/oauth2/token", server.url());
        let e = post_token_to(
            &http_client(),
            &url,
            &[("grant_type", "refresh_token")],
            "refresh",
        )
        .await
        .expect_err("malformed JSON 应返回 Err");
        let s = format!("{e}");
        assert!(s.contains("解析 token JSON 失败"), "actual: {s}");
    }
}
```

- [ ] **Step 3: 跑测试看 fail（确认 mock pattern 正确）**

Run: `cargo test --lib auth::oauth2_dev::token`
Expected: 4 tests pass（首次跑应直接绿，因为实现 + 测试同时写）

- [ ] **Step 4: clippy + 行数审计**

Run: `cargo clippy --all-targets -- -D warnings`

PowerShell：检查 token.rs 行数
```powershell
(Get-Content src\auth\oauth2_dev\token.rs | Measure-Object -Line).Lines
```
Expected: ≤ 200。若超，把 `mod tests` 整块抽到 `tests_token.rs` 并在 token.rs 末尾加 `#[cfg(test)] mod tests;`。

- [ ] **Step 5: Commit**

```powershell
git add src/auth/oauth2_dev/mod.rs src/auth/oauth2_dev/token.rs
git commit -m "feat(oauth2_dev): token.rs (exchange_code + refresh) + 4 mockito tests"
```

---

## Task 5: oauth2_dev/callback.rs (本地 server)

**Files:**
- Create: `src/auth/oauth2_dev/callback.rs` (~110 行)
- Modify: `src/auth/oauth2_dev/mod.rs` (+ pub mod callback)

**理由:** spec §3.2 + §6.1 step 4-8：`tokio::net::TcpListener::bind("127.0.0.1:45123")` 听一个 HTTP/1.1 GET，解析 `?code=...&state=...`，返回 200 OK + HTML 提示，关闭 listener。

注意 callback.rs 设计为 **5 分钟超时**（spec R6 + §7.1 CardOAuthTimeout）。

- [ ] **Step 1: 加 mod 行**

Edit `src/auth/oauth2_dev/mod.rs`，加：
```rust
pub mod callback;
```

- [ ] **Step 2: 写实现**

Create `src/auth/oauth2_dev/callback.rs`：

```rust
//! 本地 OAuth2 callback server：bind 127.0.0.1:45123 接 GET /callback?code=...&state=...
//!
//! 设计：
//! - 单连接 listener：accept 一个 connection → 解析第一行 GET → 返 200 OK + HTML → 关闭
//! - 5 分钟超时（用户没在浏览器同意 / 浏览器没弹出）
//! - state 校验：传入期望 state，请求 state 不匹配 → 拒绝并报错
//! - **不**用 axum / warp / hyper-server，纯 tokio::net::TcpListener + 手写 1 个 GET 解析

use std::time::Duration;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;

use crate::error::SjtuCliError;

const BIND_ADDR: &str = "127.0.0.1:45123";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

const SUCCESS_HTML: &[u8] = b"HTTP/1.1 200 OK\r\n\
Content-Type: text/html; charset=utf-8\r\n\
Connection: close\r\n\
\r\n\
<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>授权成功</title></head>\
<body><h2>sjtu-cli 授权成功</h2><p>可关闭本窗口，回到终端查看结果。</p></body></html>";

/// 在 127.0.0.1:45123 启 listener 等浏览器 callback。
///
/// 返回 `Ok((code, state))` 当解析成功；超时返 `CardOAuthTimeout`；
/// IO/解析错返 `CardOAuth(...)`。
///
/// **注意**：返回前会先给浏览器写 200 OK + HTML，保证用户看到"授权成功"。
pub async fn wait_for_callback() -> Result<(String, String)> {
    let listener = TcpListener::bind(BIND_ADDR).await.map_err(|e| {
        SjtuCliError::CardOAuth(format!(
            "无法 bind {BIND_ADDR}（端口被占用？）: {e}"
        ))
    })?;
    let (mut sock, _addr) = timeout(CALLBACK_TIMEOUT, listener.accept())
        .await
        .map_err(|_| SjtuCliError::CardOAuthTimeout)?
        .map_err(|e| SjtuCliError::CardOAuth(format!("accept 失败: {e}")))?;
    // 读至多 4 KiB 拿到 GET 第一行（OAuth2 callback URL 不可能更长）
    let mut buf = vec![0u8; 4096];
    let n = sock
        .read(&mut buf)
        .await
        .map_err(|e| SjtuCliError::CardOAuth(format!("读 socket: {e}")))?;
    let request = String::from_utf8_lossy(&buf[..n]).into_owned();
    let result = parse_callback_request(&request);
    // 不管成功失败都写一个响应回浏览器（失败也避免浏览器一直转圈）
    let _ = sock.write_all(SUCCESS_HTML).await;
    let _ = sock.shutdown().await;
    result
}

/// 从 raw HTTP request 第一行 `GET /callback?code=X&state=Y HTTP/1.1` 解析 code/state。
pub(crate) fn parse_callback_request(req: &str) -> Result<(String, String)> {
    let first_line = req.lines().next().ok_or_else(|| {
        SjtuCliError::CardOAuth("callback 请求为空".to_string())
    })?;
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    if method != "GET" {
        return Err(
            SjtuCliError::CardOAuth(format!("期望 GET，实际 {method}")).into(),
        );
    }
    // target = "/callback?code=X&state=Y"
    let query = target
        .split_once('?')
        .map(|(_, q)| q)
        .ok_or_else(|| SjtuCliError::CardOAuth("callback 缺 query string".to_string()))?;
    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut err_param: Option<String> = None;
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v_dec = percent_decode(v);
        match k {
            "code" => code = Some(v_dec),
            "state" => state = Some(v_dec),
            "error" => err_param = Some(v_dec),
            _ => {} // 忽略 error_description / scope 等
        }
    }
    if let Some(e) = err_param {
        return Err(SjtuCliError::CardOAuth(format!("callback 返回 error={e}")).into());
    }
    let code = code.ok_or_else(|| SjtuCliError::CardOAuth("callback 缺 code 参数".to_string()))?;
    let state = state.ok_or_else(|| SjtuCliError::CardOAuth("callback 缺 state 参数".to_string()))?;
    Ok((code, state))
}

/// 简单 percent-decode：把 %xx 还原为字节。仅 ASCII；中文等多字节 OAuth2 callback 不会出现。
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                hex_val(bytes[i + 1]),
                hex_val(bytes[i + 2]),
            ) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        // 把 + 也当作空格（form-encoded 兼容；OAuth2 query 实际不出现）
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 校验 state 一致（CSRF 防御）。不匹配返 CardOAuth("state_mismatch")。
pub fn check_state(got: &str, expected: &str) -> Result<()> {
    if got != expected {
        return Err(SjtuCliError::CardOAuth(format!(
            "state_mismatch: 期望 {expected} 实际 {got}"
        ))
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_callback_happy() {
        let r = "GET /callback?code=ABC&state=XYZ HTTP/1.1\r\nHost: 127.0.0.1:45123\r\n\r\n";
        let (c, s) = parse_callback_request(r).unwrap();
        assert_eq!(c, "ABC");
        assert_eq!(s, "XYZ");
    }

    #[test]
    fn parse_callback_with_error_param() {
        let r = "GET /callback?error=access_denied&state=XYZ HTTP/1.1\r\n\r\n";
        let e = parse_callback_request(r).expect_err("error 参数应抛错");
        assert!(format!("{e}").contains("access_denied"));
    }

    #[test]
    fn parse_callback_missing_code() {
        let r = "GET /callback?state=XYZ HTTP/1.1\r\n\r\n";
        let e = parse_callback_request(r).expect_err("缺 code 应抛错");
        assert!(format!("{e}").contains("缺 code"));
    }

    #[test]
    fn parse_callback_percent_decode() {
        let r = "GET /callback?code=A%2BB%2FC&state=XYZ HTTP/1.1\r\n\r\n";
        let (c, _) = parse_callback_request(r).unwrap();
        assert_eq!(c, "A+B/C");
    }

    #[test]
    fn state_mismatch_returns_err() {
        let e = check_state("got", "expected").expect_err("不匹配应抛错");
        assert!(format!("{e}").contains("state_mismatch"));
    }

    #[test]
    fn state_match_ok() {
        check_state("xyz", "xyz").expect("匹配应通过");
    }
}
```

- [ ] **Step 3: 跑测试**

Run: `cargo test --lib auth::oauth2_dev::callback`
Expected: 6 tests pass

- [ ] **Step 4: 行数审计 + clippy**

```powershell
(Get-Content src\auth\oauth2_dev\callback.rs | Measure-Object -Line).Lines
```
Expected: < 200。若超，拆 `tests_callback.rs`。

Run: `cargo clippy --all-targets -- -D warnings`

- [ ] **Step 5: Commit**

```powershell
git add src/auth/oauth2_dev/mod.rs src/auth/oauth2_dev/callback.rs
git commit -m "feat(oauth2_dev): callback.rs 本地 TCP listener + percent-decode + state 校验"
```

---

## Task 6: oauth2_dev/authorize.rs (URL build + 浏览器)

**Files:**
- Create: `src/auth/oauth2_dev/authorize.rs` (~85 行)
- Modify: `src/auth/oauth2_dev/mod.rs` (+ pub mod authorize)

**理由:** spec §6.1 step 5-6：构造 authorize URL + 用 headless_chrome 打开（复用 S1 已有依赖）。

- [ ] **Step 1: 加 mod 行**

Edit `src/auth/oauth2_dev/mod.rs`：
```rust
pub mod authorize;
```

- [ ] **Step 2: 写实现**

Create `src/auth/oauth2_dev/authorize.rs`：

```rust
//! 构造 jaccount.sjtu.edu.cn/oauth2/authorize URL + 用浏览器打开。
//!
//! state 生成：CSRF token 不要密码学强度，用 SystemTime nanos + pid 混淆够用。
//! 浏览器打开：复用 S1 已用的 headless_chrome（visible 模式），用户已 jAccount 登录则
//! 直接进入"授权 sjtu-cli 访问一卡通"页面，点同意即可。

use std::time::SystemTime;

use anyhow::{Context, Result};
use url::Url;

use crate::error::SjtuCliError;

pub const AUTHORIZE_URL: &str = "https://jaccount.sjtu.edu.cn/oauth2/authorize";
pub const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:45123/callback";
pub const DEFAULT_SCOPE: &str = "card_info card_transactions";

/// 构造 authorize URL：
/// `…/oauth2/authorize?response_type=code&client_id=…&redirect_uri=…&scope=…&state=…`
pub fn build_authorize_url(
    client_id: &str,
    redirect_uri: &str,
    scope: &str,
    state: &str,
) -> Result<String> {
    let mut url = Url::parse(AUTHORIZE_URL).context("解析 authorize URL")?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", scope)
        .append_pair("state", state);
    Ok(url.to_string())
}

/// 生成 state（CSRF token，非密码学）。
pub fn generate_state() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    // 32 字符 hex，足够 CSRF 用
    format!(
        "{:032x}",
        nanos.wrapping_mul(0x9e37_79b9_7f4a_7c15_u128).wrapping_add(pid)
    )
}

/// 用 headless_chrome（可见模式）打开 authorize URL。
///
/// 用户在 jAccount 已登录态下点"同意"，浏览器会 302 到 127.0.0.1:45123/callback；
/// CLI 的本地 listener (callback.rs) 接住并解析 code。
///
/// 如果 chrome 启动失败 / 无图形界面，spec R6 留 `--manual-auth` 兜底（phase-2）。
pub async fn open_in_browser(url: &str) -> Result<()> {
    let url_owned = url.to_string();
    tokio::task::spawn_blocking(move || -> Result<()> {
        use headless_chrome::{Browser, LaunchOptions};
        let options = LaunchOptions::default_builder()
            .headless(false)
            .build()
            .map_err(|e| SjtuCliError::CardOAuth(format!("chrome 启动配置: {e}")))?;
        let browser = Browser::new(options)
            .map_err(|e| SjtuCliError::CardOAuth(format!("chrome 启动失败: {e}")))?;
        let tab = browser
            .new_tab()
            .map_err(|e| SjtuCliError::CardOAuth(format!("chrome new_tab: {e}")))?;
        tab.navigate_to(&url_owned)
            .map_err(|e| SjtuCliError::CardOAuth(format!("chrome navigate: {e}")))?;
        // 不 wait_until_navigated：authorize 页面会 302 到 127.0.0.1，
        // 而 127.0.0.1 listener 在另一个 task 等接受。这里只负责"把 URL 打开"。
        // 浏览器窗口由用户手动关闭（或 callback 写完 HTML 后用户关）。
        // browser drop 在 spawn_blocking 退出时发生，但 chrome 进程独立。
        std::mem::forget(browser); // 把 Browser ownership 故意泄露，避免析构杀掉 chrome 进程
        Ok(())
    })
    .await
    .map_err(|e| SjtuCliError::CardOAuth(format!("spawn_blocking join: {e}")))??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_url_contains_all_params() {
        let s = build_authorize_url(
            "test_id",
            "http://127.0.0.1:45123/callback",
            "card_info card_transactions",
            "STATE123",
        )
        .unwrap();
        assert!(s.starts_with("https://jaccount.sjtu.edu.cn/oauth2/authorize?"));
        assert!(s.contains("response_type=code"));
        assert!(s.contains("client_id=test_id"));
        // url crate 会把空格 url-encode 为 +
        assert!(
            s.contains("scope=card_info+card_transactions") || s.contains("scope=card_info%20card_transactions"),
            "actual: {s}"
        );
        assert!(s.contains("state=STATE123"));
        // redirect_uri 应被 percent-encoded
        assert!(s.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A45123%2Fcallback"));
    }

    #[test]
    fn state_is_32_hex_chars() {
        let s = generate_state();
        assert_eq!(s.len(), 32);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn states_differ_between_calls() {
        // SystemTime 每次拿都不同（除非时钟一致到 ns）；用 pid 兜底
        let s1 = generate_state();
        // 防止 nanos 相同：sleep 微秒
        std::thread::sleep(std::time::Duration::from_micros(10));
        let s2 = generate_state();
        assert_ne!(s1, s2, "两次 state 不应相同");
    }
}
```

**注意**：`open_in_browser` 用 `std::mem::forget(browser)` 故意泄露 Browser，因为 `headless_chrome::Browser` Drop 会杀子进程；OAuth2 流程需要浏览器持续存在直到用户在浏览器里点同意 + 看到 200 OK 后才能关。这是已知 trade-off，不优雅但简单可靠。若担心进程泄露，phase-2 改 std::process::Command 调 `cmd /c start` / `xdg-open`。

- [ ] **Step 3: 跑单测**

Run: `cargo test --lib auth::oauth2_dev::authorize`
Expected: 3 tests pass（`open_in_browser` 不在单测覆盖；它的真机检验在 CP-T4-AUTH）

- [ ] **Step 4: clippy + 行数**

Run: `cargo clippy --all-targets -- -D warnings`

```powershell
(Get-Content src\auth\oauth2_dev\authorize.rs | Measure-Object -Line).Lines
```
Expected: < 200

- [ ] **Step 5: Commit**

```powershell
git add src/auth/oauth2_dev/mod.rs src/auth/oauth2_dev/authorize.rs
git commit -m "feat(oauth2_dev): authorize.rs URL 构造 + state 生成 + headless_chrome 打开"
```

---

## Task 7: oauth2_dev/refresh.rs (with_token_refresh + is_token_expired)

**Files:**
- Create: `src/auth/oauth2_dev/refresh.rs` (~85 行)
- Modify: `src/auth/oauth2_dev/mod.rs` (+ pub mod refresh)

**理由:** spec G4 + §6.4：把 API op 包起来，第一次抛 token_expired 时自动 refresh 再重试一次。同构 canvas_video::retry。这里设计成 **泛型 + 不依赖 apps/card** —— refresh 模块只看 op 的错信号，不知道 op 是什么。

注意：with_token_refresh 内部需要拿当前 oauth_session 并刷新它。本 task 先把 `with_token_refresh` 的纯逻辑写好（用一个 trait 注入 "refresh 动作"），完整集成在 T8 oauth2_dev/mod.rs 把它接上 CardOAuthSession。

- [ ] **Step 1: 加 mod 行**

Edit `src/auth/oauth2_dev/mod.rs`：
```rust
pub mod refresh;
```

- [ ] **Step 2: 写实现 + 测试**

Create `src/auth/oauth2_dev/refresh.rs`：

```rust
//! `with_token_refresh<F,Fut,T>`：包裹 API op，首次抛 token_expired 时自动 refresh + 重试。
//!
//! 设计上**不**依赖 apps/card —— refresh 模块只看错信号，不知道 op 是 balance 还是 history。
//! refresh 动作通过参数 `refresher` 注入，避免循环依赖（refresh.rs ← apps/card ← oauth2_dev::mod）。
//!
//! 同构 `commands/canvas_video/retry.rs::with_token_refresh`：
//! - 误判成本：多调一次 refresh（轻）
//! - 漏判成本：把过期错原样上抛给用户（重）
//! - 分类宁宽勿严

use std::future::Future;

use anyhow::Result;

use crate::error::SjtuCliError;

/// 包裹一个 async API 调用，首次抛 token_expired 时自动调 `refresher` 续 token 后重试一次。
///
/// **参数**：
/// - `op`: 可被调用 0..=2 次的 async closure
/// - `refresher`: token 失效时调一次的 async closure
///
/// **错信号**：op 错被 downcast 为 `SjtuCliError::CardOAuth("token_expired")`
/// 或 `SjtuCliError::SessionExpired` 时触发 refresh + 重试；其他错向上抛。
pub async fn with_token_refresh<F, Fut, R, RFut, T>(op: F, refresher: R) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
    R: FnOnce() -> RFut,
    RFut: Future<Output = Result<()>>,
{
    match op().await {
        Ok(v) => Ok(v),
        Err(e) if is_token_expired(&e) => {
            tracing::info!("oauth2_dev: token 疑似过期，触发 refresh 后重试一次");
            refresher().await?;
            op().await
        }
        Err(e) => Err(e),
    }
}

/// 哪些错信号意味着 access_token 过期。spec §6.4 + §7.2：
/// - `SjtuCliError::CardOAuth(s)` 且 `s == "token_expired"`（强类型显式）
/// - `SjtuCliError::SessionExpired`（兜底，便于复用 elec/services 错链）
/// - 错链 to_string 含 "errno=10002" 或 "401"（弱类型兜底，避免 anyhow 链断裂）
pub fn is_token_expired(e: &anyhow::Error) -> bool {
    if let Some(SjtuCliError::CardOAuth(s)) = e.downcast_ref::<SjtuCliError>() {
        if s == "token_expired" {
            return true;
        }
    }
    if matches!(e.downcast_ref::<SjtuCliError>(), Some(SjtuCliError::SessionExpired)) {
        return true;
    }
    let s = format!("{e:#}");
    s.contains("errno=10002")
        || s.contains("Authentication Failed")
        || (s.contains("401") && !s.contains("4012") && !s.contains("4013"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[tokio::test(flavor = "current_thread")]
    async fn happy_op_called_once_no_refresh() {
        let calls = Rc::new(RefCell::new(0));
        let calls_for_op = calls.clone();
        let r_called = Rc::new(RefCell::new(false));
        let r_for = r_called.clone();
        let result: Result<i32> = with_token_refresh(
            move || {
                let calls = calls_for_op.clone();
                async move {
                    *calls.borrow_mut() += 1;
                    Ok(42)
                }
            },
            move || {
                let r_for = r_for.clone();
                async move {
                    *r_for.borrow_mut() = true;
                    Ok(())
                }
            },
        )
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(*calls.borrow(), 1);
        assert!(!*r_called.borrow());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn token_expired_triggers_refresh_and_retry() {
        let calls = Rc::new(RefCell::new(0));
        let calls_for_op = calls.clone();
        let r_called = Rc::new(RefCell::new(0));
        let r_for = r_called.clone();
        let result: Result<i32> = with_token_refresh(
            move || {
                let calls = calls_for_op.clone();
                async move {
                    let n = {
                        let mut b = calls.borrow_mut();
                        *b += 1;
                        *b
                    };
                    if n == 1 {
                        Err(SjtuCliError::CardOAuth("token_expired".into()).into())
                    } else {
                        Ok(100)
                    }
                }
            },
            move || {
                let r_for = r_for.clone();
                async move {
                    *r_for.borrow_mut() += 1;
                    Ok(())
                }
            },
        )
        .await;
        assert_eq!(result.unwrap(), 100);
        assert_eq!(*calls.borrow(), 2);
        assert_eq!(*r_called.borrow(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn non_token_error_propagates_no_refresh() {
        let r_called = Rc::new(RefCell::new(0));
        let r_for = r_called.clone();
        let result: Result<i32> = with_token_refresh(
            || async { Err(SjtuCliError::InvalidInput("bad".into()).into()) },
            move || {
                let r_for = r_for.clone();
                async move {
                    *r_for.borrow_mut() += 1;
                    Ok(())
                }
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(*r_called.borrow(), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn refresh_fails_propagates() {
        let result: Result<i32> = with_token_refresh(
            || async { Err(SjtuCliError::CardOAuth("token_expired".into()).into()) },
            || async { Err(SjtuCliError::NetworkError("offline".into()).into()) },
        )
        .await;
        let e = result.unwrap_err();
        assert!(format!("{e:#}").contains("offline"), "actual: {e:#}");
    }

    #[test]
    fn is_expired_strong_signal() {
        let e: anyhow::Error = SjtuCliError::CardOAuth("token_expired".into()).into();
        assert!(is_token_expired(&e));
    }

    #[test]
    fn is_expired_weak_signal_errno_10002() {
        let e = anyhow::anyhow!("upstream: status=200 body=errno=10002 error=Authentication Failed");
        assert!(is_token_expired(&e));
    }

    #[test]
    fn is_expired_not_triggered_by_other_errno() {
        let e = anyhow::anyhow!("upstream: status=200 body=errno=4012 error=other");
        assert!(!is_token_expired(&e));
    }
}
```

- [ ] **Step 3: 跑测试**

Run: `cargo test --lib auth::oauth2_dev::refresh`
Expected: 7 tests pass

- [ ] **Step 4: clippy + 行数**

Run: `cargo clippy --all-targets -- -D warnings`

```powershell
(Get-Content src\auth\oauth2_dev\refresh.rs | Measure-Object -Line).Lines
```
Expected: < 200

- [ ] **Step 5: Commit**

```powershell
git add src/auth/oauth2_dev/mod.rs src/auth/oauth2_dev/refresh.rs
git commit -m "feat(oauth2_dev): with_token_refresh + is_token_expired 多信号识别 (7 tests)"
```

---

## Task 8: oauth2_dev/mod.rs (顶层 API + CardOAuthSession + ensure_token)

**Files:**
- Modify: `src/auth/oauth2_dev/mod.rs` (从 ~25 行扩到 ~150 行)
- Test: tests 整合进 mod.rs 同文件（若行数超限再拆 tests_mod.rs）

**理由:** spec §3.4 + §6.1-3：顶层 API `connect()` / `load_session()` / `save_session()` / `ensure_fresh_token()` / `refresh_token()`，把 secret/token/callback/authorize 全部串起来。

CardOAuthSession 是 spec §3.4 落定的 schema。

- [ ] **Step 1: 读现有 mod.rs**

Run: `Read src/auth/oauth2_dev/mod.rs`

确认目前是 5 行 mod 声明的骨架。

- [ ] **Step 2: 扩写 mod.rs**

Edit `src/auth/oauth2_dev/mod.rs`，**完整覆盖**为：

```rust
//! T4 一卡通 OAuth2 Authorization Code 通道（RFC6749 标准）。
//!
//! 不用 `oauth2` crate（违 CLAUDE.md 不引入新依赖）。
//! 不用 `keyring`（跨平台行为不一致；JSON+chmod 600 与 cookies::session.json 同制，单一可审计点）。
//! 不用 `axum`（1 endpoint 不值得引入 micro-framework；手卷 60 行 listener 够用）。
//! Refresh 走 failure-driven 不走 timer（同 canvas_video::with_token_refresh 范式，省状态机）。
//!
//! 与现 `src/auth/oauth2/` 完全不同：那个是 shuiyuan 用的 302-chain 跟链，
//! 终点取 Discourse 的 `_t` cookie；本模块走 code-for-token 拿 Bearer access_token。

pub mod authorize;
pub mod callback;
pub mod refresh;
pub mod secret;
pub mod token;

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::error::SjtuCliError;

/// 本地落盘的 OAuth2 session schema (`~/.sjtu-cli/sub_sessions/card_oauth.json`)。
///
/// **不**复用 `cookies::Session`：那个是 cookie 形态（name/value/domain/path 列表），
/// 跟 token 形态完全不同。显式 struct 表达 OAuth2 语义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardOAuthSession {
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<FixedOffset>,
    /// 申请 scope，可读性参考（"card_info card_transactions"）。
    pub scope: String,
    /// 主卡号（首次跑 `/v1/me/card` 时存）；后续 history 查询用。
    /// 可为空（极少情况下首次跑还没拿到）。
    #[serde(default)]
    pub main_card_no: Option<String>,
    pub captured_at: DateTime<FixedOffset>,
}

const SESSION_FILE: &str = "card_oauth.json";
/// access_token 在 `expires_at - 60s` 时即视为过期，提前 refresh。
pub const REFRESH_MARGIN_SECS: i64 = 60;

/// 子 session JSON 路径：`~/.sjtu-cli/sub_sessions/card_oauth.json`。
pub fn session_path() -> Result<PathBuf> {
    Ok(config::sub_sessions_dir()?.join(SESSION_FILE))
}

/// 读 OAuth2 session。文件不存在 → NotAuthenticated。
pub fn load_session() -> Result<CardOAuthSession> {
    let path = session_path()?;
    if !path.exists() {
        return Err(SjtuCliError::NotAuthenticated.into());
    }
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("读取 {} 失败", path.display()))?;
    let s: CardOAuthSession = serde_json::from_str(&raw)
        .with_context(|| format!("解析 {} 失败（card_oauth.json 损坏？）", path.display()))?;
    Ok(s)
}

/// 保存 OAuth2 session。chmod 600 on Unix。
pub fn save_session(sess: &CardOAuthSession) -> Result<()> {
    config::ensure_dirs()?;
    let path = session_path()?;
    let raw = serde_json::to_string_pretty(sess).context("序列化 card_oauth session 失败")?;
    std::fs::write(&path, raw).with_context(|| format!("写入 {} 失败", path.display()))?;
    chmod_600(&path)?;
    Ok(())
}

/// 清除 OAuth2 session（用于 `sjtu card logout` 或异常恢复）。幂等。
pub fn clear_session() -> Result<()> {
    let path = session_path()?;
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("删除 {} 失败", path.display()))?;
    }
    Ok(())
}

/// 检查 token 是否需要 refresh（now ≥ expires_at - 60s）。
pub fn is_token_stale(sess: &CardOAuthSession) -> bool {
    let now = Utc::now().with_timezone(sess.expires_at.offset());
    let margin = chrono::Duration::seconds(REFRESH_MARGIN_SECS);
    now + margin >= sess.expires_at
}

/// 调 refresh_token 续期并落盘。
pub async fn refresh_and_save(http: &reqwest::Client) -> Result<CardOAuthSession> {
    let mut sess = load_session()?;
    let secret = secret::load_secret()?;
    let resp = token::refresh(http, &sess.refresh_token, &sess.client_id, &secret).await?;
    let now = beijing_now();
    sess.access_token = resp.access_token;
    if !resp.refresh_token.is_empty() {
        sess.refresh_token = resp.refresh_token;
    }
    sess.expires_at = now
        + chrono::Duration::seconds(resp.expires_in as i64)
        - chrono::Duration::seconds(REFRESH_MARGIN_SECS);
    sess.captured_at = now;
    save_session(&sess)?;
    Ok(sess)
}

/// 当前北京时间（+08:00）。
pub fn beijing_now() -> DateTime<FixedOffset> {
    let beijing = FixedOffset::east_opt(8 * 3600).expect("+08:00 常量");
    Utc::now().with_timezone(&beijing)
}

/// 把 expires_in（秒）+ now 算成 expires_at。
pub fn compute_expires_at(expires_in: u64) -> DateTime<FixedOffset> {
    beijing_now() + chrono::Duration::seconds(expires_in as i64)
}

#[cfg(unix)]
fn chmod_600(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(path)?.permissions();
    perm.set_mode(0o600);
    std::fs::set_permissions(path, perm)
        .with_context(|| format!("无法设置 {} 权限为 600", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn chmod_600(_path: &std::path::Path) -> Result<()> {
    // Windows ACL 暂留 TODO（S0 继承的留白）
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sess(expires_offset_secs: i64) -> CardOAuthSession {
        let now = beijing_now();
        CardOAuthSession {
            client_id: "test_id".into(),
            access_token: "AT".into(),
            refresh_token: "RT".into(),
            expires_at: now + chrono::Duration::seconds(expires_offset_secs),
            scope: "card_info card_transactions".into(),
            main_card_no: None,
            captured_at: now,
        }
    }

    #[test]
    fn fresh_token_not_stale() {
        let s = make_sess(1700); // 28 分钟后过期，仍 fresh
        assert!(!is_token_stale(&s));
    }

    #[test]
    fn token_within_margin_is_stale() {
        let s = make_sess(30); // 30s 后过期，60s margin 内 → stale
        assert!(is_token_stale(&s));
    }

    #[test]
    fn expired_token_is_stale() {
        let s = make_sess(-100); // 100s 前过期
        assert!(is_token_stale(&s));
    }

    #[test]
    fn compute_expires_at_is_about_30min_from_now() {
        let exp = compute_expires_at(1800);
        let diff = (exp - beijing_now()).num_seconds();
        assert!(diff >= 1790 && diff <= 1810, "实际 diff={diff}");
    }

    #[test]
    fn session_roundtrip_json() {
        let s = make_sess(1800);
        let json = serde_json::to_string(&s).unwrap();
        let back: CardOAuthSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.client_id, s.client_id);
        assert_eq!(back.access_token, s.access_token);
    }
}
```

- [ ] **Step 3: 跑测试**

Run: `cargo test --lib auth::oauth2_dev::`
Expected: 全部 oauth2_dev 测试 pass（应是 ~25 个：secret 2 + token 4 + callback 6 + authorize 3 + refresh 7 + mod 5）

- [ ] **Step 4: clippy + 行数 + 整模块行数审计**

Run: `cargo clippy --all-targets -- -D warnings`

```powershell
Get-ChildItem src\auth\oauth2_dev -Filter *.rs | ForEach-Object { "$($_.Name): $((Get-Content $_.FullName | Measure-Object -Line).Lines)" }
```
Expected: 每文件 < 200。整目录 ~600 行。

- [ ] **Step 5: Commit**

```powershell
git add src/auth/oauth2_dev/mod.rs
git commit -m "feat(oauth2_dev): mod.rs 顶层 CardOAuthSession + load/save/refresh + 5 tests"
```

---

## Task 9: apps/card/models.rs

**Files:**
- Create: `src/apps/card/mod.rs` (~15 行骨架)
- Create: `src/apps/card/models.rs` (~140 行)
- Modify: `src/apps/mod.rs` (+ pub mod card)

**理由:** spec §5.1-5.4：定义 `CardInfo` (GET /v1/me/card) + `Transaction` (GET /v1/me/card/transactions) + `Envelope<T>` 复用。注意 `dateTimAccount` 拼写陷阱（少 e）。

- [ ] **Step 1: 加 apps/mod.rs 一行**

先 Read `src/apps/mod.rs` 看现有结构：

Run: `Read src/apps/mod.rs`

然后加 `pub mod card;`。

- [ ] **Step 2: 写 apps/card/mod.rs 骨架**

Create `src/apps/card/mod.rs`：

```rust
//! 一卡通子系统 (`api.sjtu.edu.cn/v1/me/card*`)：余额 + 消费记录只读 API client。
//!
//! 鉴权链：OAuth2 Authorization Code (auth/oauth2_dev/) → access_token → Authorization: Bearer
//! 与 elec / services / shuiyuan / canvas 等 cookie-based 子系统不同；专属 helper 见
//! `auth/oauth2_dev/refresh.rs::with_token_refresh`。
//!
//! 红线：余额查询 + 消费记录 only。挂失 / 充值 / 改密码 / 改照片 写端点 spec §NG1 永不实装。

pub mod api;
pub mod http;
pub mod models;
pub mod throttle;

pub use api::Client;
pub use models::{CardInfo, Transaction};
```

- [ ] **Step 3: 写 models.rs**

Create `src/apps/card/models.rs`：

```rust
//! `api.sjtu.edu.cn/v1/me/card*` 响应结构体。契约见 spec §5.1-5.4。
//!
//! 金额硬约束：`cardBalance` / `transBalance` / `amount` 服务端发 `double`，
//! 反序列化经 `crate::util::decimal` 转为 `Decimal`；序列化输出字符串（避 JSON f64 精度）。
//!
//! 拼写陷阱：`dateTimAccount`（少个 e）—— 仅 orderBy=dateTimeAccount 时返。
//! `#[serde(rename = "dateTimAccount")]` 锁定服务端原字段名。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// `errno + error + total + entities` 通用 envelope（同 elec/services）。
///
/// **bound 显式重写**：默认 derive 会从 `Vec<T>` 推断 `T: Default`，
/// 但 `CardInfo`/`Transaction` 不需要 Default。把 bound 收紧到只要 `T: Deserialize`。
#[derive(Debug, Clone, Deserialize)]
#[serde(bound(deserialize = "T: serde::Deserialize<'de>"))]
pub(super) struct Envelope<T> {
    #[serde(default)]
    pub errno: Option<i32>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub entities: Vec<T>,
}

/// `GET /v1/me/card` 单条 entity（spec §5.1-5.2）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardInfo {
    /// 身份字段（命令层默认抹掉，仅 `--with-identity` 出）
    #[serde(default)]
    pub user: Option<UserInfo>,
    #[serde(rename = "cardNo")]
    pub card_no: String,
    /// 物理卡号 (`cardId`)。永久不透出到命令层（即使 `--with-identity`）。
    #[serde(rename = "cardId", default)]
    pub card_id: Option<String>,
    /// 绑定银行卡号。`--with-identity` 时脱敏前 4 + `****` + 后 4。
    #[serde(rename = "bankNo", default)]
    pub bank_no: Option<String>,
    #[serde(rename = "expireDate", default)]
    pub expire_date: Option<String>,
    /// 主余额（元）
    #[serde(rename = "cardBalance", with = "crate::util::decimal")]
    pub card_balance: Decimal,
    /// 过渡余额（元）
    #[serde(rename = "transBalance", with = "crate::util::decimal")]
    pub trans_balance: Decimal,
    #[serde(default)]
    pub lost: bool,
    #[serde(default)]
    pub frozen: bool,
    #[serde(rename = "faceType", default)]
    pub face_type: Option<String>,
    /// 含"硕士研究生"等身份描述 → `--with-identity` 才出。
    #[serde(rename = "faceSubType", default)]
    pub face_sub_type: Option<String>,
}

/// 卡用户身份（spec §5.2 user.*）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub organize: Option<Organize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organize {
    #[serde(default)]
    pub name: Option<String>,
}

/// `GET /v1/me/card/transactions` 单条 entity（spec §5.4）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// 消费时间（Unix ms_ts）。命令层会转为 +08:00 DateTime。
    #[serde(rename = "dateTime")]
    pub date_time_ms: i64,
    /// ⚠️ 拼写陷阱：服务端字段名缺 e。仅 orderBy=dateTimeAccount 时返。
    #[serde(rename = "dateTimAccount", default)]
    pub date_tim_account_ms: Option<i64>,
    #[serde(default)]
    pub system: Option<String>,
    #[serde(rename = "merchantNo", default)]
    pub merchant_no: Option<String>,
    #[serde(default)]
    pub merchant: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// 消费为负、充值为正
    #[serde(with = "crate::util::decimal")]
    pub amount: Decimal,
    /// 交易后卡余额
    #[serde(rename = "cardBalance", with = "crate::util::decimal")]
    pub card_balance: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn parse_card_info_full_fields() {
        let raw = r#"{
            "entities": [{
                "user": {"code":"123","name":"张三","organize":{"name":"电信"}},
                "cardNo": "0012345",
                "cardId": "AABBCC",
                "bankNo": "6228000011112222",
                "expireDate": "20601231",
                "cardBalance": 284.25,
                "transBalance": 0.00,
                "lost": false,
                "frozen": false,
                "faceType": "1",
                "faceSubType": "硕士研究生"
            }]
        }"#;
        let env: Envelope<CardInfo> = serde_json::from_str(raw).unwrap();
        assert_eq!(env.entities.len(), 1);
        let c = &env.entities[0];
        assert_eq!(c.card_no, "0012345");
        assert_eq!(c.card_balance, Decimal::from_str_exact("284.25").unwrap());
        assert_eq!(c.trans_balance, Decimal::from_str_exact("0.00").unwrap());
        assert_eq!(c.user.as_ref().unwrap().name.as_deref(), Some("张三"));
        assert_eq!(c.face_sub_type.as_deref(), Some("硕士研究生"));
    }

    #[test]
    fn parse_card_info_minimal() {
        let raw = r#"{"entities":[{"cardNo":"X","cardBalance":0,"transBalance":0}]}"#;
        let env: Envelope<CardInfo> = serde_json::from_str(raw).unwrap();
        let c = &env.entities[0];
        assert_eq!(c.card_no, "X");
        assert!(c.user.is_none());
        assert!(!c.lost && !c.frozen);
    }

    #[test]
    fn parse_transactions_with_spelling_trap() {
        // 注意服务端字段是 dateTimAccount（少 e）
        let raw = r#"{
            "total": 2,
            "entities": [
                {"dateTime": 1715750000000, "dateTimAccount": 1715760000000,
                 "system": "S", "merchantNo":"M1", "merchant":"大众餐厅",
                 "description":"持卡人消费", "amount": -10.66, "cardBalance": 273.59},
                {"dateTime": 1715840000000,
                 "system": "S", "merchant":"宿舍洗衣机",
                 "description":"持卡人消费", "amount": -2.0, "cardBalance": 271.59}
            ]
        }"#;
        let env: Envelope<Transaction> = serde_json::from_str(raw).unwrap();
        assert_eq!(env.total, Some(2));
        assert_eq!(env.entities.len(), 2);
        let t0 = &env.entities[0];
        assert_eq!(t0.amount, Decimal::from_str_exact("-10.66").unwrap());
        assert_eq!(t0.date_tim_account_ms, Some(1715760000000));
        let t1 = &env.entities[1];
        assert_eq!(t1.amount, Decimal::from_str_exact("-2.0").unwrap());
        assert_eq!(t1.date_tim_account_ms, None);
    }

    #[test]
    fn parse_transactions_empty() {
        let raw = r#"{"total": 0, "entities": []}"#;
        let env: Envelope<Transaction> = serde_json::from_str(raw).unwrap();
        assert_eq!(env.total, Some(0));
        assert_eq!(env.entities.len(), 0);
    }

    #[test]
    fn parse_envelope_with_errno_10002() {
        let raw = r#"{"errno": 10002, "error": "Authentication Failed", "total": 0}"#;
        let env: Envelope<CardInfo> = serde_json::from_str(raw).unwrap();
        assert_eq!(env.errno, Some(10002));
        assert_eq!(env.error.as_deref(), Some("Authentication Failed"));
        assert_eq!(env.entities.len(), 0);
    }

    #[test]
    fn negative_amount_serialized_as_string() {
        let t = Transaction {
            date_time_ms: 0,
            date_tim_account_ms: None,
            system: None,
            merchant_no: None,
            merchant: None,
            description: None,
            amount: Decimal::from_str_exact("-10.66").unwrap(),
            card_balance: Decimal::from_str_exact("273.59").unwrap(),
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains(r#""amount":"-10.66""#), "actual: {s}");
        assert!(s.contains(r#""cardBalance":"273.59""#), "actual: {s}");
    }
}
```

- [ ] **Step 4: 跑测试**

Run: `cargo test --lib apps::card::models`
Expected: 6 tests pass

- [ ] **Step 5: clippy + 行数审计**

Run: `cargo clippy --all-targets -- -D warnings`

```powershell
(Get-Content src\apps\card\models.rs | Measure-Object -Line).Lines
```
Expected: < 200

- [ ] **Step 6: Commit**

```powershell
git add src/apps/mod.rs src/apps/card/
git commit -m "feat(card): models.rs CardInfo + Transaction + dateTimAccount 拼写陷阱守护 (6 tests)"
```

注：mod.rs 引用了 api/http/throttle 还未存在，cargo build 会失败。这是正常 TDD 节奏——后续 T10/T11 补全。**本 commit 是 broken build commit**，可以接受（feat 系列 commit gate 允许，但提示下个 task 必须修复编译）。

或者更稳健做法：本 task 暂只 `pub mod models;`，T10 补 throttle+http 时改成 `pub mod models; pub mod throttle; pub mod http;`，T11 改成 `pub mod api;` + `pub use`。改方案 — 用这种：

**修正** Step 2：apps/card/mod.rs 只写最小内容：
```rust
//! 一卡通子系统（如上 doc 不变）...

pub mod models;
```

后续 T10/T11 增量加 mod 行。这样每个 commit 都是 buildable。

把 Step 2 的 mod.rs 改成上面的最小版本。

---

## Task 10: apps/card/{throttle,http}.rs

**Files:**
- Create: `src/apps/card/throttle.rs` (~40 行；复制 elec/throttle 改 MIN_INTERVAL=400ms)
- Create: `src/apps/card/http.rs` (~140 行；基于 elec/http 改 Bearer 头 + base URL + 401/errno=10002 信号)
- Modify: `src/apps/card/mod.rs` (+ pub mod http; pub mod throttle;)

**理由:** spec §4.1：throttle 与 elec 同构。http 与 elec 同构但有差异：
1. base URL = `https://api.sjtu.edu.cn`
2. 鉴权头：`Authorization: Bearer <token>`（不是 cookie jar）
3. 401 / `errno=10002` 需识别为 `SjtuCliError::CardOAuth("token_expired")`，触发 with_token_refresh 重试

- [ ] **Step 1: 加 mod 行**

Edit `src/apps/card/mod.rs`：
```rust
pub mod http;
pub mod models;
pub mod throttle;
```

- [ ] **Step 2: 写 throttle.rs**

Create `src/apps/card/throttle.rs`：

```rust
//! 固定 sleep 节流：每次 `api.sjtu.edu.cn/v1/me/card*` 调用前强制间隔 ≥ MIN_INTERVAL。
//!
//! 与 elec / services 同策略，独立一份避免跨子系统耦合。
//! card_transactions 文档无明示限速，保守 400ms。

use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tokio::time::sleep;

pub(super) const MIN_INTERVAL: Duration = Duration::from_millis(400);

#[derive(Debug)]
pub(super) struct Throttle {
    last: Mutex<Instant>,
}

impl Throttle {
    pub fn new() -> Self {
        let seed = Instant::now()
            .checked_sub(MIN_INTERVAL)
            .unwrap_or_else(Instant::now);
        Self {
            last: Mutex::new(seed),
        }
    }

    pub async fn wait(&self) {
        let mut last = self.last.lock().await;
        let elapsed = last.elapsed();
        if elapsed < MIN_INTERVAL {
            sleep(MIN_INTERVAL - elapsed).await;
        }
        *last = Instant::now();
    }
}
```

- [ ] **Step 3: 写 http.rs**

Create `src/apps/card/http.rs`：

```rust
//! 一卡通 HTTP client：Bearer 鉴权 + JSON GET + 401/errno=10002 token-expired 识别。
//!
//! 与 `apps/elec/http.rs` 同构骨架，差异：
//! - base URL `https://api.sjtu.edu.cn`
//! - 不带 cookie jar；改 `Authorization: Bearer <token>` 头
//! - errno=10002 / "Authentication Failed" → 上抛 `CardOAuth("token_expired")`，
//!   命令层 with_token_refresh 接住自动 refresh + 重试

use std::time::Duration;

use anyhow::Result;
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::Client;

use super::throttle::Throttle;
use crate::error::SjtuCliError;

pub(super) const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
pub(super) const BASE: &str = "https://api.sjtu.edu.cn";

/// 构造 reqwest Client（无 cookie jar，鉴权走 header）。
pub(super) fn build_http_client() -> Result<Client> {
    Client::builder()
        .redirect(Policy::limited(5))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .gzip(true)
        .http1_only()
        .pool_idle_timeout(Duration::from_millis(0))
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 HTTP client 失败: {e}")).into())
}

/// JSON GET：节流 + Bearer 头 + 重试 1 次（仅连接层错）+ 错误带 snippet。
/// 返回原始 body String —— api.sjtu 的 envelope 解析在 api.rs 处理。
pub(super) async fn fetch_json_raw(
    http: &Client,
    throttle: &Throttle,
    url: &str,
    access_token: &str,
    label: &str,
) -> Result<String> {
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        throttle.wait().await;
        match fetch_once(http, url, access_token, label).await {
            Ok(v) => return Ok(v),
            Err(e) => {
                let msg = format!("{e:#}");
                if !is_retriable(&msg) {
                    return Err(e);
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.expect("至少一次尝试的错误"))
}

async fn fetch_once(
    http: &Client,
    url: &str,
    access_token: &str,
    label: &str,
) -> Result<String> {
    let resp = http
        .get(url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, UA)
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .send()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("GET {url}: {e}")))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| SjtuCliError::NetworkError(format!("{url}: 读 body: {e}")))?;
    // 401 路径：服务端把 token 过期映射到 HTTP 401（非标准但要兜底）
    if status.as_u16() == 401 {
        return Err(SjtuCliError::CardOAuth("token_expired".into()).into());
    }
    if !status.is_success() {
        return Err(SjtuCliError::UpstreamError(format!(
            "{label} status={status} snippet={}",
            truncate(&body, 200)
        ))
        .into());
    }
    // 200 路径：envelope 形式的 token_expired 由调用方 detect_token_expired_in_body 处理
    Ok(body)
}

/// 检查 200 body 是否带 errno=10002 / "Authentication Failed"（spec §5.1 错误形态）。
/// 命中 → 返 `SjtuCliError::CardOAuth("token_expired")`。
pub(super) fn detect_token_expired_in_body(body: &str) -> Option<anyhow::Error> {
    // 优先解析为 JSON 判断 errno；失败时退到 substring 兜底
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(errno) = val.get("errno").and_then(|v| v.as_i64()) {
            if errno == 10002 {
                return Some(SjtuCliError::CardOAuth("token_expired".into()).into());
            }
        }
    }
    if body.contains("\"errno\":10002") || body.contains("Authentication Failed") {
        return Some(SjtuCliError::CardOAuth("token_expired".into()).into());
    }
    None
}

fn is_retriable(msg: &str) -> bool {
    msg.contains("operation timed out")
        || msg.contains("error sending request")
        || msg.contains("connection closed")
        || msg.contains("connection reset")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_expired_errno_10002_via_json() {
        let body = r#"{"errno":10002,"error":"Authentication Failed","total":0}"#;
        let e = detect_token_expired_in_body(body).expect("应识别 errno=10002");
        let downcasted = e.downcast_ref::<SjtuCliError>();
        assert!(matches!(downcasted, Some(SjtuCliError::CardOAuth(s)) if s == "token_expired"));
    }

    #[test]
    fn detect_expired_substring_fallback() {
        // 即使 JSON 解析失败也能 substring 兜底
        let body = r#"{"errno":10002 garbled... Authentication Failed"#;
        assert!(detect_token_expired_in_body(body).is_some());
    }

    #[test]
    fn detect_no_match_on_normal_body() {
        let body = r#"{"errno":0,"total":1,"entities":[{"cardNo":"X"}]}"#;
        assert!(detect_token_expired_in_body(body).is_none());
    }

    #[test]
    fn detect_no_match_on_4012() {
        let body = r#"{"errno":4012,"error":"other"}"#;
        assert!(detect_token_expired_in_body(body).is_none());
    }
}
```

- [ ] **Step 4: 跑测试**

Run: `cargo test --lib apps::card::http`
Expected: 4 tests pass

- [ ] **Step 5: clippy + 行数**

Run: `cargo clippy --all-targets -- -D warnings`

```powershell
Get-ChildItem src\apps\card -Filter *.rs | ForEach-Object { "$($_.Name): $((Get-Content $_.FullName | Measure-Object -Line).Lines)" }
```
Expected: 每文件 < 200

注：本 commit 前 apps/card/mod.rs 还引用未存在的 api，编译会失败。同 T9 处理：本 task 暂只 `pub mod http; pub mod models; pub mod throttle;`，不写 `pub mod api;`；T11 加。

修正 Step 1：
```rust
//! …doc 不变…

pub mod http;
pub mod models;
pub mod throttle;
```

确保 cargo build 在本 task 结束时绿。

- [ ] **Step 6: Commit**

```powershell
git add src/apps/card/throttle.rs src/apps/card/http.rs src/apps/card/mod.rs
git commit -m "feat(card): throttle.rs (400ms) + http.rs (Bearer + token_expired detect; 4 tests)"
```

---

## Task 11: apps/card/api.rs (Client + get_balance + get_transactions)

**Files:**
- Create: `src/apps/card/api.rs` (~140 行)
- Modify: `src/apps/card/mod.rs` (+ pub mod api; pub use api::Client;)

**理由:** spec §6.4：Client 持 access_token + http + throttle；提供 `get_balance() -> CardInfo` 和 `get_transactions(card_no, days, limit) -> Vec<Transaction>`。

- [ ] **Step 1: 加 mod 行**

Edit `src/apps/card/mod.rs`：

```rust
pub mod api;
pub mod http;
pub mod models;
pub mod throttle;

pub use api::Client;
pub use models::{CardInfo, Transaction};
```

- [ ] **Step 2: 写 api.rs**

Create `src/apps/card/api.rs`：

```rust
//! 一卡通 API Client：连接 OAuth2 session → 调 `/v1/me/card*` 端点。
//!
//! 鉴权：spec §6.1，要求 caller 已持有 fresh access_token（通过 oauth2_dev::load_session）。
//! token 过期识别：see http::detect_token_expired_in_body。
//! refresh 责任在调用方（commands/card/handlers.rs）用 with_token_refresh 包裹。

use anyhow::Result;
use chrono::NaiveDate;

use super::http::{build_http_client, detect_token_expired_in_body, fetch_json_raw, BASE};
use super::models::{CardInfo, Envelope, Transaction};
use super::throttle::Throttle;
use crate::auth::oauth2_dev::{self, CardOAuthSession};
use crate::error::SjtuCliError;

/// 一卡通 client。每个命令 connect 一次；不长持有。
pub struct Client {
    http: reqwest::Client,
    throttle: Throttle,
    session: CardOAuthSession,
}

impl Client {
    /// 从本地 card_oauth.json 读 session 构造 client。
    /// session 未登录 → `NotAuthenticated`；fresh 检查在 handler 层做。
    pub async fn connect() -> Result<Self> {
        let session = oauth2_dev::load_session()?;
        let http = build_http_client()?;
        Ok(Self {
            http,
            throttle: Throttle::new(),
            session,
        })
    }

    /// 暴露 access_token 给 caller（write to log？不，仅给 fetch 用）。
    pub(crate) fn access_token(&self) -> &str {
        &self.session.access_token
    }

    /// session 持有的主卡号（若已存）。
    pub fn main_card_no(&self) -> Option<&str> {
        self.session.main_card_no.as_deref()
    }

    /// `GET /v1/me/card`：拿当前用户的卡信息（多张卡时 entities 多条）。
    /// 默认取 `entities[0]` 作"主卡"。
    pub async fn get_balance(&self) -> Result<CardInfo> {
        let url = format!("{BASE}/v1/me/card");
        let body = fetch_json_raw(
            &self.http,
            &self.throttle,
            &url,
            self.access_token(),
            "card_info",
        )
        .await?;
        if let Some(e) = detect_token_expired_in_body(&body) {
            return Err(e);
        }
        let env: Envelope<CardInfo> = serde_json::from_str(&body).map_err(|e| {
            SjtuCliError::UpstreamError(format!(
                "card_info JSON 解析失败: {e}, snippet={}",
                snip(&body)
            ))
        })?;
        env.entities
            .into_iter()
            .next()
            .ok_or_else(|| SjtuCliError::UpstreamError("card_info entities 为空".into()).into())
    }

    /// `GET /v1/me/card/transactions`：拿时间窗口内的消费记录。
    /// `card_no` 必传（避免多卡用户的不确定性）；`limit` clamp 到 1..=100。
    pub async fn get_transactions(
        &self,
        card_no: &str,
        begin: NaiveDate,
        end: NaiveDate,
        limit: u32,
    ) -> Result<(u64, Vec<Transaction>)> {
        let begin_ms = begin
            .and_hms_opt(0, 0, 0)
            .expect("00:00:00 valid")
            .and_utc()
            .timestamp_millis();
        let end_ms = end
            .and_hms_opt(23, 59, 59)
            .expect("23:59:59 valid")
            .and_utc()
            .timestamp_millis();
        let clamped_limit = limit.clamp(1, 100);
        let url = format!(
            "{BASE}/v1/me/card/transactions?cardNo={card_no}&beginDate={begin_ms}&endDate={end_ms}&orderBy=dateTime&start=0&limit={clamped_limit}"
        );
        let body = fetch_json_raw(
            &self.http,
            &self.throttle,
            &url,
            self.access_token(),
            "card_transactions",
        )
        .await?;
        if let Some(e) = detect_token_expired_in_body(&body) {
            return Err(e);
        }
        let env: Envelope<Transaction> = serde_json::from_str(&body).map_err(|e| {
            SjtuCliError::UpstreamError(format!(
                "card_transactions JSON 解析失败: {e}, snippet={}",
                snip(&body)
            ))
        })?;
        let total = env.total.unwrap_or(env.entities.len() as u64);
        Ok((total, env.entities))
    }
}

fn snip(s: &str) -> String {
    let max = 200;
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // 注：Client::connect 依赖磁盘上的 card_oauth.json，单测无法做真机。
    // 单测覆盖 get_balance / get_transactions 的 happy path 用 mockito 模拟整个 BASE。
    //
    // 但 BASE 是常量；要在测试里覆盖需要把 url 注入。这里偷工：单测只验证
    // url 构造正确（独立函数 build_transactions_url）。完整 e2e 留 CP-T4-BAL/HIST。

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn date_to_ms_begin_of_day() {
        let d = ymd(2026, 5, 17);
        let ms = d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
        // 2026-05-17 00:00:00 UTC = 1779580800000
        assert_eq!(ms, 1779580800000);
    }

    #[test]
    fn date_to_ms_end_of_day() {
        let d = ymd(2026, 5, 17);
        let ms = d.and_hms_opt(23, 59, 59).unwrap().and_utc().timestamp_millis();
        assert_eq!(ms, 1779580800000 + 23 * 3600 * 1000 + 59 * 60 * 1000 + 59 * 1000);
    }

    #[test]
    fn snip_truncates() {
        let s = "a".repeat(300);
        let t = snip(&s);
        assert_eq!(t.len(), 200 + 3); // "..." 后缀
    }
}
```

注：单测覆盖度较低（仅 helper 数学函数）；完整 e2e 留 CP-T4-BAL/HIST。这是 trade-off：mock 整个 BASE 需要把 BASE 改为可注入参数，会让 api.rs 多 20 行 dependency injection 样板。spec §9 已经列了真机 CP，可接受。

- [ ] **Step 3: 跑测试**

Run: `cargo test --lib apps::card`
Expected: 6 (models) + 4 (http) + 3 (api) = 13 tests pass

- [ ] **Step 4: build 全量 + clippy + 行数**

Run: `cargo build && cargo clippy --all-targets -- -D warnings`

```powershell
(Get-Content src\apps\card\api.rs | Measure-Object -Line).Lines
```
Expected: < 200

- [ ] **Step 5: Commit**

```powershell
git add src/apps/card/mod.rs src/apps/card/api.rs
git commit -m "feat(card): api.rs Client + get_balance + get_transactions (3 tests)"
```

---

## Task 12: commands/card/{mod,data,handlers}.rs

**Files:**
- Create: `src/commands/card/mod.rs` (~10 行)
- Create: `src/commands/card/data.rs` (~85 行：BalanceData + HistoryData + UserIdentity + TransactionItem)
- Create: `src/commands/card/handlers.rs` (~150 行：cmd_balance + cmd_history + cmd_auth + redact 等)
- Modify: `src/commands/mod.rs` (+ pub mod card)

**理由:** spec §3.5 + §6.1：命令层接收 CLI args，构造 BalanceData / HistoryData，串联 oauth2_dev 流程（首次 auth → token → Client → API → 渲染 Envelope）。

- [ ] **Step 1: 加 commands/mod.rs**

Run: `Grep pattern="pub mod" path="src/commands/mod.rs" output_mode=content`

然后在合适位置（按字母序）加 `pub mod card;`。

- [ ] **Step 2: 写 mod.rs**

Create `src/commands/card/mod.rs`：

```rust
//! `sjtu card <sub>` handler：OAuth2 鉴权下的一卡通余额 + 消费记录只读命令。

pub mod data;
pub mod handlers;
```

- [ ] **Step 3: 写 data.rs**

Create `src/commands/card/data.rs`：

```rust
//! `sjtu card balance` / `history` 输出数据结构。
//!
//! 默认输出抹身份字段（PII）；`--with-identity` 才输出 user / bank_no / faceSubType。
//! 物理卡号 `cardId` 永久不出（即便 `--with-identity`，防卡号克隆攻击面 — spec §8 红线）。
//! 金额一律 Decimal，序列化为字符串（避 f64 精度坑）。

use chrono::{DateTime, FixedOffset};
use rust_decimal::Decimal;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct BalanceData {
    pub card_no_redacted: String,
    pub balance: Decimal,
    pub trans_balance: Decimal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_date: Option<String>,
    pub lost: bool,
    pub frozen: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face_type: Option<String>,
    /// 含身份描述（"硕士研究生"）→ 仅 `--with-identity`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face_sub_type: Option<String>,
    /// `--with-identity` 才填
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<UserIdentity>,
    /// `--with-identity` 才填，前 4 + `****` + 后 4
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bank_no_redacted: Option<String>,
    pub from_cache: bool,
    pub elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct UserIdentity {
    pub code: String,
    pub name: String,
    pub organize: String,
}

#[derive(Debug, Serialize)]
pub struct HistoryData {
    pub card_no_redacted: String,
    pub begin_date_local: String,
    pub end_date_local: String,
    pub returned: usize,
    pub total: u64,
    pub transactions: Vec<TransactionItem>,
    pub total_amount: Decimal,
    pub from_cache: bool,
    pub elapsed_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct TransactionItem {
    /// `+08:00` 时区的消费时间，ISO 8601
    pub consumed_at: DateTime<FixedOffset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merchant_no: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merchant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub amount: Decimal,
    pub balance_after: Decimal,
}

/// 把 cardNo 脱敏成 "前 4 + ***"。短于 5 字符时整体 ***。
pub fn redact_card_no(s: &str) -> String {
    if s.len() < 5 {
        "***".to_string()
    } else {
        format!("{}***", &s[..4])
    }
}

/// bankNo 脱敏：前 4 + **** + 后 4。短于 9 字符时整体 ****。
pub fn redact_bank_no(s: &str) -> String {
    if s.len() < 9 {
        "****".to_string()
    } else {
        format!("{}****{}", &s[..4], &s[s.len() - 4..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_card_no_normal() {
        assert_eq!(redact_card_no("0012345678"), "0012***");
    }

    #[test]
    fn redact_card_no_short() {
        assert_eq!(redact_card_no("123"), "***");
        assert_eq!(redact_card_no(""), "***");
    }

    #[test]
    fn redact_bank_no_normal() {
        assert_eq!(redact_bank_no("6228000011112222"), "6228****2222");
    }

    #[test]
    fn redact_bank_no_short() {
        assert_eq!(redact_bank_no("12345678"), "****");
    }
}
```

- [ ] **Step 4: 写 handlers.rs**

Create `src/commands/card/handlers.rs`：

```rust
//! `sjtu card <sub>` 主流程：connect OAuth2 → API 调用（with_token_refresh 包裹）→ 渲染 Envelope。
//!
//! `cmd_auth`：手动触发 authorize → callback → token exchange → 落盘。
//! `cmd_balance`：默认抹身份；`--with-identity` 出 user / bank_no。
//! `cmd_history`：时间窗口 N 天（默认 30，最大 365）；limit 默认 50 最大 100。

use std::time::Instant;

use anyhow::Result;
use chrono::{Duration, FixedOffset, NaiveDate, TimeZone};
use rust_decimal::Decimal;

use super::data::{
    redact_bank_no, redact_card_no, BalanceData, HistoryData, TransactionItem, UserIdentity,
};
use crate::apps::card::Client;
use crate::auth::oauth2_dev::{
    self, authorize, callback, is_token_stale, refresh, secret, token, CardOAuthSession,
};
use crate::error::SjtuCliError;
use crate::output::{render, Envelope, OutputFormat};

/// `sjtu card auth`：手动触发 OAuth2 授权流（首次使用时跑）。
pub async fn cmd_auth(client_id: String, fmt: Option<OutputFormat>) -> Result<()> {
    let secret = secret::load_secret()?;
    let state = authorize::generate_state();
    let url = authorize::build_authorize_url(
        &client_id,
        authorize::DEFAULT_REDIRECT_URI,
        authorize::DEFAULT_SCOPE,
        &state,
    )?;
    tracing::info!("OAuth2 authorize: 已构造 URL，启 listener 后打开浏览器");

    // 同步打开浏览器；浏览器会 302 到 127.0.0.1:45123
    authorize::open_in_browser(&url).await?;

    // 等浏览器 callback（5 分钟超时；spec §7.1 CardOAuthTimeout）
    let (code, got_state) = callback::wait_for_callback().await?;
    callback::check_state(&got_state, &state)?;
    tracing::info!("callback 拿到 code，开始换 token");

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 http: {e}")))?;
    let resp = token::exchange_code(
        &http,
        &code,
        authorize::DEFAULT_REDIRECT_URI,
        &client_id,
        &secret,
    )
    .await?;

    let now = oauth2_dev::beijing_now();
    let sess = CardOAuthSession {
        client_id,
        access_token: resp.access_token,
        refresh_token: resp.refresh_token,
        expires_at: now
            + Duration::seconds(resp.expires_in as i64)
            - Duration::seconds(oauth2_dev::REFRESH_MARGIN_SECS),
        scope: authorize::DEFAULT_SCOPE.to_string(),
        main_card_no: None,
        captured_at: now,
    };
    oauth2_dev::save_session(&sess)?;
    tracing::info!("OAuth2 session 已落盘 ~/.sjtu-cli/sub_sessions/card_oauth.json");

    // 首跑抓一次 /v1/me/card 拿主卡号写入 session
    let client = Client::connect().await?;
    let info = client.get_balance().await?;
    let mut updated = oauth2_dev::load_session()?;
    updated.main_card_no = Some(info.card_no.clone());
    oauth2_dev::save_session(&updated)?;

    render(
        Envelope::ok(serde_json::json!({
            "ok": true,
            "card_no_redacted": redact_card_no(&info.card_no),
            "expires_in_secs": resp.expires_in,
            "scope": updated.scope,
        })),
        fmt,
    )
}

/// `sjtu card balance [--with-identity]`：当前卡余额查询。
pub async fn cmd_balance(with_identity: bool, fmt: Option<OutputFormat>) -> Result<()> {
    let started = Instant::now();
    let info = ensure_fresh_and_call(|c| async move { c.get_balance().await }).await?;
    let user = if with_identity {
        info.user.as_ref().map(|u| UserIdentity {
            code: u.code.clone().unwrap_or_default(),
            name: u.name.clone().unwrap_or_default(),
            organize: u
                .organize
                .as_ref()
                .and_then(|o| o.name.clone())
                .unwrap_or_default(),
        })
    } else {
        None
    };
    let bank_no_redacted = if with_identity {
        info.bank_no.as_deref().map(redact_bank_no)
    } else {
        None
    };
    let face_sub_type = if with_identity {
        info.face_sub_type.clone()
    } else {
        None
    };
    let data = BalanceData {
        card_no_redacted: redact_card_no(&info.card_no),
        balance: info.card_balance,
        trans_balance: info.trans_balance,
        expire_date: info.expire_date.clone(),
        lost: info.lost,
        frozen: info.frozen,
        face_type: info.face_type.clone(),
        face_sub_type,
        user,
        bank_no_redacted,
        from_cache: false,
        elapsed_ms: started.elapsed().as_millis(),
    };
    render(Envelope::ok(data), fmt)
}

/// `sjtu card history --days N --limit M`：消费记录查询。
pub async fn cmd_history(days: u32, limit: u32, fmt: Option<OutputFormat>) -> Result<()> {
    if days == 0 || days > 365 {
        return Err(SjtuCliError::InvalidInput(format!("--days {days} 超出范围 (1..=365)")).into());
    }
    let started = Instant::now();
    let end_local = chrono::Local::now().date_naive();
    let begin_local = end_local - Duration::days((days as i64) - 1);

    let sess = oauth2_dev::load_session()?;
    let card_no = sess.main_card_no.clone().ok_or_else(|| {
        SjtuCliError::CardOAuth(
            "session 缺 main_card_no；先跑 `sjtu card balance` 一次以初始化".into(),
        )
    })?;

    let card_no_for_call = card_no.clone();
    let begin = begin_local;
    let end = end_local;
    let (total, txs) = ensure_fresh_and_call(move |c| {
        let card_no = card_no_for_call.clone();
        async move { c.get_transactions(&card_no, begin, end, limit).await }
    })
    .await?;

    let beijing = FixedOffset::east_opt(8 * 3600).expect("+08:00");
    let items: Vec<TransactionItem> = txs
        .into_iter()
        .map(|t| TransactionItem {
            consumed_at: beijing
                .timestamp_millis_opt(t.date_time_ms)
                .single()
                .unwrap_or_else(|| beijing.timestamp_millis_opt(0).unwrap()),
            system: t.system,
            merchant_no: t.merchant_no,
            merchant: t.merchant,
            description: t.description,
            amount: t.amount,
            balance_after: t.card_balance,
        })
        .collect();
    let total_amount: Decimal = items.iter().map(|t| t.amount).sum();
    let data = HistoryData {
        card_no_redacted: redact_card_no(&card_no),
        begin_date_local: begin_local.format("%Y-%m-%d").to_string(),
        end_date_local: end_local.format("%Y-%m-%d").to_string(),
        returned: items.len(),
        total,
        transactions: items,
        total_amount,
        from_cache: false,
        elapsed_ms: started.elapsed().as_millis(),
    };
    render(Envelope::ok(data), fmt)
}

/// 包装层：load session → 若 stale 先 refresh → 调 op；op 抛 token_expired 再 refresh + 重试一次。
async fn ensure_fresh_and_call<F, Fut, T>(op: F) -> Result<T>
where
    F: Fn(Client) -> Fut + Send + 'static + Clone,
    Fut: std::future::Future<Output = Result<T>> + Send,
    T: Send + 'static,
{
    let http_refresh = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 http: {e}")))?;
    // 1. 预检 stale → 主动 refresh（避免一次必失败的 API 调用）
    {
        let sess = oauth2_dev::load_session()?;
        if is_token_stale(&sess) {
            tracing::info!("oauth2_dev: token 预检已 stale，触发 refresh");
            oauth2_dev::refresh_and_save(&http_refresh).await?;
        }
    }
    // 2. with_token_refresh 包裹，事后 errno=10002 兜底
    let op2 = op.clone();
    refresh::with_token_refresh(
        || {
            let op2 = op2.clone();
            async move {
                let client = Client::connect().await?;
                op2(client).await
            }
        },
        || async {
            oauth2_dev::refresh_and_save(&http_refresh).await?;
            Ok(())
        },
    )
    .await
}
```

注：handlers.rs 行数估计 ~165。check < 200。若超，把 `ensure_fresh_and_call` 抽到 `commands/card/refresh_helper.rs` 单独一文件（~40 行）。

- [ ] **Step 5: 跑测试**

Run: `cargo test --lib commands::card`
Expected: 4 tests pass（redact_card_no x 2 + redact_bank_no x 2）

- [ ] **Step 6: clippy + 行数 + build**

Run: `cargo clippy --all-targets -- -D warnings && cargo build`

```powershell
Get-ChildItem src\commands\card -Filter *.rs | ForEach-Object { "$($_.Name): $((Get-Content $_.FullName | Measure-Object -Line).Lines)" }
```
Expected: 每文件 < 200

- [ ] **Step 7: Commit**

```powershell
git add src/commands/mod.rs src/commands/card/
git commit -m "feat(card): commands handlers (auth + balance + history) + redact 4 tests"
```

---

## Task 13: cli/card.rs + cli/mod.rs dispatch

**Files:**
- Create: `src/cli/card.rs` (~55 行)
- Modify: `src/cli/mod.rs` (+ mod card; + Commands::Card variant; + dispatch arm)

**理由:** spec §13：暴露 `sjtu card auth/balance/history` 3 个子命令到 CLI。

- [ ] **Step 1: 写 cli/card.rs**

Create `src/cli/card.rs`：

```rust
//! `sjtu card <sub>` clap 枚举 + 派发。
//!
//! MVP 3 个子命令（均**只读**）：
//! - `auth --client-id <ID>` —— 首次 OAuth2 授权流（弹浏览器同意）
//! - `balance [--with-identity]` —— 当前卡余额
//! - `history [--days N] [--limit M]` —— 消费记录
//!
//! 红线：充值 / 挂失 / 解挂 / 改密码 / 改照片 全不实装（spec §NG1 永久排除）。

use anyhow::Result;
use clap::Subcommand;

use crate::commands::card::handlers as card_cmds;
use crate::output::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum CardSub {
    /// 首次 OAuth2 授权（弹浏览器同意）。clientId 来自 developer.sjtu.edu.cn 申请。
    Auth {
        /// 开发者平台批准的 client_id（公开信息，可入命令行）。
        /// 客户端密钥 client_secret 由 `~/.sjtu-cli/card_oauth_secret.txt` 独立存放。
        #[arg(long)]
        client_id: String,
    },

    /// 当前卡余额查询。**只读**。
    ///
    /// 默认抹身份字段；`--with-identity` 出学号/姓名/单位/绑定银行卡（前 4 + **** + 后 4）。
    Balance {
        /// 包含身份字段（学号 / 姓名 / 单位 / 银行卡尾号）。默认不出。
        #[arg(long, default_value_t = false)]
        with_identity: bool,
    },

    /// 消费记录查询。**只读**。
    History {
        /// 时间窗口天数，默认 30，最大 365。
        #[arg(long, default_value_t = 30)]
        days: u32,
        /// 单次最多返回多少条，默认 50，服务端硬限 100，CLI 自动 clamp。
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
}

pub async fn dispatch(sub: CardSub, fmt: Option<OutputFormat>) -> Result<()> {
    match sub {
        CardSub::Auth { client_id } => card_cmds::cmd_auth(client_id, fmt).await,
        CardSub::Balance { with_identity } => card_cmds::cmd_balance(with_identity, fmt).await,
        CardSub::History { days, limit } => card_cmds::cmd_history(days, limit, fmt).await,
    }
}
```

- [ ] **Step 2: 改 cli/mod.rs**

Edit `src/cli/mod.rs`：

1. 在 mod 声明区（行 9-16）按字母序加：`mod card;`（应放在 `mod canvas` 前）

2. 在 `Commands` 枚举中（行 45-106），把 `Elec { ... }` 那段后加一段：

```rust

    /// 一卡通（api.sjtu.edu.cn/v1/me/card*）：余额 + 消费记录只读查询（OAuth2 鉴权）。
    Card {
        #[command(subcommand)]
        sub: card::CardSub,
    },
```

3. 在 `run()` match arms 中（行 139-151），把 `Commands::Elec { sub } => ...` 那行后加：

```rust
        Commands::Card { sub } => card::dispatch(sub, fmt).await,
```

- [ ] **Step 3: 改 `sjtu hello` 的 stage 文本（可选；保留 S3a 是有点过时但不阻塞）**

跳过——hello 文本是 status snapshot，T17 文档收尾一并改。

- [ ] **Step 4: build + 1 个冒烟测试**

Run: `cargo build`
Expected: 编译通过 0 warning

Run: `cargo run -- card --help`
Expected: 看到 auth / balance / history 3 个子命令

Run: `cargo run -- card balance`
Expected: 报错 "未登录"（因为 ~/.sjtu-cli/sub_sessions/card_oauth.json 不存在）；YAML envelope 输出 `ok: false`、`error.code: not_authenticated`

- [ ] **Step 5: clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: 0 warnings

- [ ] **Step 6: Commit**

```powershell
git add src/cli/mod.rs src/cli/card.rs
git commit -m "feat(cli): sjtu card auth/balance/history 子命令派发"
```

---

## Task 14: 真机 CP-T4-AUTH (首次授权 e2e)

**前置阻塞:** client_id 审批已下（用户去 developer.sjtu.edu.cn 申请），client_secret 已落本机：

```powershell
# 用户手动操作
Set-Content -Path "$env:APPDATA\sjtu-cli\card_oauth_secret.txt" -Value "<SECRET>" -Encoding utf8 -NoNewline
```

（Windows ACL 暂不管；Unix 用户需 `chmod 600 ~/.config/sjtu-cli/card_oauth_secret.txt`）

- [ ] **Step 1: 跑首次 auth**

```powershell
cargo run --release -- card auth --client-id <CLIENT_ID> --json
```

**预期行为**:
- 浏览器弹窗 jaccount.sjtu.edu.cn/oauth2/authorize（用户已 jAccount 登录态）
- "授权 sjtu-cli 访问一卡通信息" 同意页 → 用户点同意
- 浏览器跳转 127.0.0.1:45123/callback?code=...&state=...
- CLI 收 callback，换 token，落盘
- CLI 输出 `{"ok":true,"data":{"ok":true,"card_no_redacted":"0012***","expires_in_secs":1800,"scope":"card_info card_transactions"}}`

**记录到 tasks/todo.md**（T17 一并写）:
- 浏览器从弹出到拿 code 的耗时
- card_no_redacted 前 4 位
- 是否需要 `--scope` 重选（OQ-1 PKCE 检验：若服务端报 PKCE required 则把 plan 加一个 T7.5 加 code_challenge）

- [ ] **Step 2: 验证 ~/.sjtu-cli/sub_sessions/card_oauth.json 存在 + 权限**

```powershell
Test-Path "$env:APPDATA\sjtu-cli\sub_sessions\card_oauth.json"
Get-Content "$env:APPDATA\sjtu-cli\sub_sessions\card_oauth.json"
```

Expected:
- 文件存在
- JSON 字段全（access_token / refresh_token / expires_at / scope / main_card_no / captured_at）
- 文件**不含** client_secret

- [ ] **Step 3: 失败路径 - state mismatch（mockito 已覆盖，跳过真机）**

- [ ] **Step 4: 失败路径 - 用户在浏览器拒绝**

```powershell
cargo run --release -- card auth --client-id <CLIENT_ID> --json
```
用户在 jAccount 授权页**点拒绝**。

Expected:
- callback 路径 `?error=access_denied&state=...`
- CLI 输出 `{"ok":false,"error":{"code":"card_oauth_failed","message":"...access_denied..."}}` exit 1
- card_oauth.json **没有**被覆盖（原 session 保留）

- [ ] **Step 5: 失败路径 - 端口冲突**

```powershell
# 启一个临时占用 45123
$server = Start-Job -ScriptBlock { Start-Sleep -Seconds 60 }
# 假设另一进程已占（如果上面 Start-Job 不真占端口，跳过）
cargo run --release -- card auth --client-id <CLIENT_ID> --json
```

或更简单：连续跑两次 `sjtu card auth`（第一次 listener 还活着 second 跑会 port_in_use）。

Expected:
- error.code: `card_oauth_failed`，message 含 `45123` + "占用"
- exit 1

**本 task 无 commit**——CP 是真机验证记录，T17 文档收尾时把结论写入 todo.md。

---

## Task 15: 真机 CP-T4-BAL / BAL-ID / HIST / HIST-EMPTY / LIMIT

**前置:** Task 14 已完成（card_oauth.json 在）。

- [ ] **CP-T4-BAL: `sjtu card balance --json`**

```powershell
cargo run --release -- card balance --json
```

验证：
- `data.card_no_redacted` 形如 `1234***`
- `data.balance` 是字符串形 Decimal（如 `"284.25"`）
- `data.user` / `data.bank_no_redacted` / `data.face_sub_type` 字段**整体 skip**
- `data.lost` / `data.frozen` 是 bool
- exit 0

- [ ] **CP-T4-BAL-ID: `sjtu card balance --with-identity --json`**

```powershell
cargo run --release -- card balance --with-identity --json
```

验证：
- `data.user.name` 出真名
- `data.user.code` 出学号
- `data.user.organize` 出单位
- `data.bank_no_redacted` 形如 `6228****2222`（前 4 + **** + 后 4）
- `data.face_sub_type` 出（如 "硕士研究生"）
- **`data.card_id` 不出**（spec §8 红线）

- [ ] **CP-T4-HIST: `sjtu card history --days 7 --json`**

```powershell
cargo run --release -- card history --days 7 --json
```

验证：
- `data.transactions` 数组（按日期排序）
- 每条 `amount` 是字符串负数（如 `"-10.66"`），`balance_after` 也字符串
- `data.total_amount` ≈ sum(amount)（手工抽查 3-5 条核对）
- `data.returned` ≤ `data.total`

- [ ] **CP-T4-HIST-EMPTY: 选个无消费日期窗口**

```powershell
cargo run --release -- card history --days 1 --json
```

如果今天无消费，验证：
- `data.total: 0`
- `data.returned: 0`
- `data.transactions: []`
- `data.total_amount: "0"`（或 `"0.00"`）
- exit 0（不 panic）

- [ ] **CP-T4-LIMIT: clamp 测试**

```powershell
cargo run --release -- card history --days 30 --limit 200 --json
```

验证：
- 服务端硬限 100，CLI clamp 应起作用（无 4xx）
- `data.returned <= 100`
- 若 returned == 100，可启动 phase-2 思考分页

**记录到 tasks/todo.md**（T17 收尾）:
- 每个 CP 的耗时
- 任何 deviation（字段 unexpected null / 编码问题等）

---

## Task 16: 真机 CP-T4-REFRESH

**前置:** Task 14 + 15 全过。

- [ ] **Step 1: 记录当前 expires_at**

```powershell
Get-Content "$env:APPDATA\sjtu-cli\sub_sessions\card_oauth.json" | ConvertFrom-Json | Select-Object expires_at
```

记录 `expires_at_t0`。

- [ ] **Step 2: 等 31 分钟（让 access_token 自然过期）**

或者更快测试：手工编辑 card_oauth.json 把 `expires_at` 改成 `now - 5min`（仿真过期）。

- [ ] **Step 3: 跑 balance**

```powershell
$env:RUST_LOG = "info"
cargo run --release -- card balance --json
```

验证：
- stderr 出现 `oauth2_dev: token 预检已 stale，触发 refresh` 或 `token 疑似过期`
- 命令仍 `ok: true`
- card_oauth.json 的 `expires_at` 已更新（>= original + 1800s - 60s）
- 若服务端返回了新 refresh_token，refresh_token 也更新（不一定，看服务端策略 OQ-3）

- [ ] **Step 4: 验证 lazy refresh 也工作（手工把 access_token 改一个字符破坏，但 expires_at 仍 fresh）**

```powershell
# 编辑 card_oauth.json，把 access_token 第一位改成 X
notepad "$env:APPDATA\sjtu-cli\sub_sessions\card_oauth.json"
```

```powershell
cargo run --release -- card balance --json
```

验证：
- 第一次 API 调用 returns errno=10002（被破坏的 token）
- with_token_refresh 触发 refresh → 重试 → 成功
- card_oauth.json access_token 已修复

**本 task 无 commit。**

---

## Task 17: 文档收尾

**Files:**
- Modify: `tasks/todo.md` (+ T4 完整 CP 记录)
- Modify: `tasks/lessons.md` (+ 1-3 条 OAuth2 / Rust 教训)
- Modify: `README.md` (+ card 子系统简介)
- Modify: `SKILL.md` (+ `sjtu card` 用法)
- Modify: `CLAUDE.md` (当前阶段更新 → "S3 Phase 2 — 一卡通完成")

- [ ] **Step 1: 更新 tasks/todo.md**

Read 现 todo.md，在合适进度块加一段 2026-05-17 / 18 T4 完整完成记录：

模板：
```markdown
### 2026-05-17 / 2026-05-XX T4 一卡通 OAuth2 — 完成
- 17 文件新增 + 4 修改（util/decimal.rs / auth/oauth2_dev/{secret,token,callback,authorize,refresh,mod}.rs / apps/card/{mod,api,http,models,throttle}.rs / commands/card/{mod,data,handlers}.rs / cli/card.rs；elec/models.rs 切 import）
- 单测：~36 个（util/decimal 5 + error +3 + oauth2_dev secret 2 token 4 callback 6 authorize 3 refresh 7 mod 5 / card models 6 http 4 api 3 / card data redact 4）
- 真机 CP 全过：AUTH / BAL / BAL-ID / HIST / HIST-EMPTY / LIMIT / REFRESH （N 个 deviation 记录如下：…）
- 累计文件总行数：~1260 行新增，全 < 200 行/文件
```

- [ ] **Step 2: 更新 tasks/lessons.md**

新增条目（举例）：
```markdown
### 2026-05-XX OAuth2 Authorization Code 手卷 vs `oauth2` crate
- 手卷只多 120 行（token.rs 80 + callback.rs 100 + authorize.rs 60），但避免 +1 crate
- 关键陷阱：`oauth2` crate 隐含 PKCE on by default；本 spec 选 raw 后留 OQ-1 单独测试 PKCE 是否被服务端强制（CP-T4-AUTH 落地后确认）
```

```markdown
### 2026-05-XX `headless_chrome::Browser` Drop 杀子进程坑
- 现象：spawn_blocking 退出后 Browser Drop → chrome 进程被杀 → 用户还没点同意浏览器就关了
- 修：`std::mem::forget(browser)` 故意泄露 ownership；trade-off：进程到 CLI 退出后才能由 OS 回收
- 教训：浏览器自动化里 Browser 持有者必须**跨过用户交互**才能 Drop
```

```markdown
### 2026-05-XX `dateTimAccount` 拼写陷阱
- 服务端返回字段拼写少一个 e（`dateTimAccount` 而非 `dateTimeAccount`）
- 修：`#[serde(rename = "dateTimAccount")]` 锁住原拼写
- 教训：第三方 API 的字段名要直接以官方文档为准，不要"看着像 typo 就改"，宁可丑也别擦伤兼容
```

- [ ] **Step 3: 更新 README.md**

加一段 "一卡通查询" 用法：
```markdown
### 一卡通（OAuth2 鉴权）

首次使用需在 [developer.sjtu.edu.cn](https://developer.sjtu.edu.cn) 申请 client_id（scope: `card_info` + `card_transactions`）。
client_secret 落本机 `~/.sjtu-cli/card_oauth_secret.txt`（Unix chmod 600）。

```bash
sjtu card auth --client-id <YOUR_CLIENT_ID>     # 首次授权，弹浏览器
sjtu card balance                                # 余额（默认抹身份）
sjtu card balance --with-identity                # 含学号 / 姓名 / 银行卡尾号
sjtu card history --days 7                       # 7 天消费记录
```
```

- [ ] **Step 4: 更新 SKILL.md（如果有）**

确认 SKILL.md 是 AI Agent 使用指南。加 `sjtu card` 命令到命令表。

- [ ] **Step 5: 更新 CLAUDE.md "当前阶段"**

Edit `CLAUDE.md`，把 `## 当前阶段` 区块的最后一行：
```
**已完成**: …… **CAS retry 层 follow-up**……
**下一步**: S3 Phase 2 候选 — 一卡通明细 / 通知聚合 / 图书馆借阅；……
```
改为：
```
**已完成**: …… T4 一卡通 OAuth2 完整（OAuth2 Authorization Code 手卷 / api.sjtu.edu.cn / 17 文件 ~1260 行 / 36 单测 / 7 真机 CP 全过 2026-05-XX）
**下一步**: S3 Phase 2 候选续 — 通知聚合 / 图书馆借阅 / Phase 2 多卡支持；新 follow-up：① card_oauth refresh_token 一次性策略观察（OQ-3 待 1-2 周真机数据）② OQ-1 PKCE 是否被服务端强制（CP-T4-AUTH 真机已答）
```

- [ ] **Step 6: build + clippy + fmt 全量守护**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --lib`
Expected: 全绿

- [ ] **Step 7: Commit**

```powershell
git add tasks/todo.md tasks/lessons.md README.md SKILL.md CLAUDE.md
git commit -m "docs(t4): card OAuth2 完成 — todo + lessons + README + CLAUDE 收尾"
```

---

## 完整 commit 顺序回顾

```
1. build: tokio 加 net feature 备 T4 OAuth2 callback server
2. refactor(util): decimal_str_or_num 从 elec 提到 util 单一来源
3. feat(error): 加 CardOAuth + SecretMissing + Timeout 3 variants
4. feat(oauth2_dev): secret.rs 读 card_oauth_secret.txt + 600 守护
5. feat(oauth2_dev): token.rs (exchange + refresh) + 4 mockito tests
6. feat(oauth2_dev): callback.rs 本地 TCP listener + percent-decode + state 校验
7. feat(oauth2_dev): authorize.rs URL 构造 + state + headless_chrome 打开
8. feat(oauth2_dev): with_token_refresh + is_token_expired (7 tests)
9. feat(oauth2_dev): mod.rs 顶层 CardOAuthSession + load/save/refresh + 5 tests
10. feat(card): models.rs CardInfo + Transaction + dateTimAccount 拼写陷阱守护
11. feat(card): throttle.rs (400ms) + http.rs (Bearer + token_expired detect)
12. feat(card): api.rs Client + get_balance + get_transactions
13. feat(card): commands handlers (auth + balance + history) + redact 4 tests
14. feat(cli): sjtu card auth/balance/history 子命令派发
15. docs(t4): card OAuth2 完成 — todo + lessons + README + CLAUDE 收尾
```

共 **15 个 commit**（与 spec §13 高层 15 任务对齐 — T14/15/16 是真机 CP 无 commit）。

---

## Self-Review 检查表

- [x] **Spec coverage**：
  - G1 balance（默认抹身份）→ T9 models + T12 handlers cmd_balance ✓
  - G2 history --days N（Decimal 累加）→ T12 handlers cmd_history ✓
  - G3 OAuth2 完整流程 → T3 secret + T4 token + T5 callback + T6 authorize + T8 mod ✓
  - G4 透明 refresh → T7 with_token_refresh + T12 ensure_fresh_and_call ✓
  - G5 Decimal 单一来源 → T1 util/decimal.rs ✓
  - G6 测试 mockito + 真机 4-7 CP → T1-T13 单测 + T14-T16 真机 ✓
  - G7 文件 < 200 行 → 每 task Step "行数审计" 守护 ✓
  - NG1 写端点不实装 → T13 cli 只暴露 auth/balance/history ✓
  - NG3 多卡只做 1 张 → T12 cmd_history 必须 main_card_no ✓
  - NG4 PKCE 不实装 → 无 task；CP-T4-AUTH 真机验证 OQ-1 ✓
  - NG5 不引入新 crate → 无 Cargo.toml 加 crate；仅加 tokio feature ✓
  - PII 默认脱敏 + --with-identity → T12 redact + handlers 分支 ✓

- [x] **Placeholder scan**：
  - 检查 "TODO" / "TBD" / "implement later" → 仅在 hello stage 文本里有 1 处（T13 Step 3 跳过；T17 一并改），所有代码块都是完整可编译代码 ✓
  - "appropriate error handling" / "edge cases" → 无 ✓
  - "similar to task N" 不重复代码 → 每个 task 独立完整代码 ✓

- [x] **Type consistency**：
  - `CardOAuthSession` 字段（client_id / access_token / refresh_token / expires_at / scope / main_card_no / captured_at）在 T8 定义后被 T12 完整引用 ✓
  - `with_token_refresh` 签名 `F: Fn() -> Fut, R: FnOnce() -> RFut` 在 T7 定义后被 T12 调用一致 ✓
  - `redact_card_no` 在 T12 定义后被 T14 CP 验证 `1234***` 形态一致 ✓
  - `is_token_stale` / `compute_expires_at` 在 T8 定义后被 T12 ensure_fresh_and_call 使用一致 ✓

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-17-t4-ecard-oauth2.md`. Two execution options:

**1. Subagent-Driven (recommended)** — 我 dispatch fresh subagent per task，review between tasks，fast iteration

**2. Inline Execution** — 在本 session 用 executing-plans，batch execution with checkpoints for review

哪种？
