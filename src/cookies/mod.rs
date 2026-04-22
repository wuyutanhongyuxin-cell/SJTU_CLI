//! Cookie 持久化模块根：`Cookie` / `Session` 模型 + 读写 I/O re-export。
//!
//! - `model`：类型定义（本文件顶层）
//! - `io`：`~/.sjtu-cli/` 下主 / 子 session 的 load / save / clear
//! - `tests`：整合测试

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

mod io;
#[cfg(test)]
mod tests;

pub use io::{
    clear_session, clear_sub_session, load_session, load_sub_session, save_session,
    save_sub_session,
};

/// 单条 cookie。唯一性按 RFC 6265 §5.3 用 (name, domain, path) 三元组判定；
/// 同名同域但不同 path 是两条独立 cookie，不能互相覆盖。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub expires: Option<DateTime<Utc>>,
}

/// JAccount 主 session：一组 cookie + 抓取时间 + 软性过期时间。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub cookies: Vec<Cookie>,
    pub captured_at: DateTime<Utc>,
    /// 软性 TTL。S0 固定 30 天；拿到真实 `JAAuthCookie` 的 expires 后可覆盖。
    pub soft_expires_at: DateTime<Utc>,
}

impl Session {
    /// 用当前时间 + 30 天 TTL 构造。
    pub fn new(cookies: Vec<Cookie>) -> Self {
        let now = Utc::now();
        Self {
            cookies,
            captured_at: now,
            soft_expires_at: now + Duration::days(30),
        }
    }

    /// 软 TTL 是否已过期。真正失效由上游 401 触发刷新。
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.soft_expires_at
    }

    /// 查指定名字的 cookie 值（只比 name，不区分 domain/path）。
    pub fn get(&self, name: &str) -> Option<&str> {
        self.cookies
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.value.as_str())
    }

    /// 脱敏 cookie 表。key 用 `name@domain,path` 三元组（RFC 6265 §5.3），
    /// 避免同名同域不同 path 被 HashMap 覆盖；空值退化为 `-`。
    pub fn redacted(&self) -> HashMap<String, String> {
        self.cookies
            .iter()
            .map(|c| (cookie_key(c), redact(&c.value)))
            .collect()
    }
}

/// 脱敏展示键：`name@domain,path`。见 `Session::redacted`。
pub(super) fn cookie_key(c: &Cookie) -> String {
    let domain = c.domain.as_deref().unwrap_or("-");
    let path = c.path.as_deref().unwrap_or("-");
    format!("{}@{domain},{path}", c.name)
}

fn redact(v: &str) -> String {
    if v.len() <= 8 {
        "***".to_string()
    } else {
        format!("{}***", &v[..8])
    }
}
