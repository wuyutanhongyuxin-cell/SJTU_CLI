//! LTI launch Bootstrap 30min 缓存（V5.A）。
//!
//! 文件：`~/.sjtu-cli/sub_sessions/canvas_video_bootstrap_<course_id>_<lti_tool_id>.json`
//! 权限：Unix 600 / Windows ACL TODO（沿用 cookies/io.rs 同款 chmod_600 cfg-gate）。
//!
//! 失效路径：
//! - TTL 30min 过期 → load 返 None → 上层重 launch
//! - course_id / lti_tool_id 不匹配 → 防错配 → load 返 None
//! - JSON 反序列化失败 → 坏缓存 → load 返 None（下次 save 自愈）
//! - 业务失败（30min 内 token 提前作废）→ handlers 层 with_token_refresh 调 clear() 然后重 launch

use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::models::Bootstrap;
use crate::cookies::{sub_session_path, Cookie};

/// 30 分钟。远低于 token 真实 TTL（1-3h），保守。
const TTL_SECS: u64 = 1800;

/// 文件命名前缀，clear() 用前缀匹配。
const FILE_PREFIX: &str = "canvas_video_bootstrap_";

/// 序列化到 sub_session 文件的结构（自描述：含 course_id / lti_tool_id / saved_at / ttl）。
/// 跟 Bootstrap 平铺等价 + 4 个元数据字段。
#[derive(Debug, Serialize, Deserialize)]
struct CachedBootstrap {
    course_id: u64,
    lti_tool_id: u64,
    saved_at: DateTime<Utc>,
    ttl_secs: u64,
    token: String,
    cour_id: String,
    lti_course_id: String,
    session_cookies: Vec<Cookie>,
}

impl CachedBootstrap {
    fn from_bootstrap(course_id: u64, lti_tool_id: u64, b: &Bootstrap) -> Self {
        Self {
            course_id,
            lti_tool_id,
            saved_at: Utc::now(),
            ttl_secs: TTL_SECS,
            token: b.token.clone(),
            cour_id: b.cour_id.clone(),
            lti_course_id: b.lti_course_id.clone(),
            session_cookies: b.session_cookies.clone(),
        }
    }

    fn into_bootstrap(self) -> Bootstrap {
        Bootstrap {
            token: self.token,
            cour_id: self.cour_id,
            lti_course_id: self.lti_course_id,
            session_cookies: self.session_cookies,
        }
    }

    /// `saved_at + ttl <= now` 视为过期。`now` 显式入参便于单测。
    fn is_expired(&self, now: DateTime<Utc>) -> bool {
        let ttl = Duration::seconds(self.ttl_secs as i64);
        self.saved_at + ttl <= now
    }
}

/// 缓存文件路径：`canvas_video_bootstrap_<course_id>_<lti_tool_id>.json`。
fn bootstrap_cache_path(course_id: u64, lti_tool_id: u64) -> Result<PathBuf> {
    sub_session_path(&format!("{FILE_PREFIX}{course_id}_{lti_tool_id}"))
}
