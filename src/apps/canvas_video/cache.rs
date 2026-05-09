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

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
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

/// 保存 CachedBootstrap 到指定路径：原子写（tmp + rename）+ Unix chmod 600。
/// `pub(super)` 给 tests_parse.rs 用；prod 走 save() 包装。
pub(super) fn save_to_path(
    path: &Path,
    course_id: u64,
    lti_tool_id: u64,
    b: &Bootstrap,
) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("缓存路径无父目录: {}", path.display()))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("mkdir {} 失败", parent.display()))?;
    let cached = CachedBootstrap::from_bootstrap(course_id, lti_tool_id, b);
    let json = serde_json::to_string_pretty(&cached).context("序列化 CachedBootstrap 失败")?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json).with_context(|| format!("写 {} 失败", tmp.display()))?;
    chmod_600(&tmp)?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("rename {} → {} 失败", tmp.display(), path.display()))?;
    Ok(())
}

/// 从指定路径加载并通过 4 重校验：IO / JSON / id 匹配 / TTL。
/// 任一失败返 None（不 panic、不 propagate），上层调缓存命中失败重 launch 即可。
/// `pub(super)` 给 tests_parse.rs 用；prod 走 load() 包装。
pub(super) fn load_from_path(
    path: &Path,
    course_id: u64,
    lti_tool_id: u64,
) -> Option<Bootstrap> {
    let raw = std::fs::read_to_string(path).ok()?;
    let cached: CachedBootstrap = serde_json::from_str(&raw).ok()?;
    if cached.course_id != course_id || cached.lti_tool_id != lti_tool_id {
        debug!(
            file_course = cached.course_id,
            want_course = course_id,
            "Bootstrap 缓存 course_id / lti_tool_id 不匹配，忽略"
        );
        return None;
    }
    if cached.is_expired(Utc::now()) {
        debug!(course_id, lti_tool_id, "Bootstrap 缓存过期，忽略");
        return None;
    }
    Some(cached.into_bootstrap())
}

/// Unix 收 600 权限（cookies/io.rs 同款）。Windows 暂留 ACL TODO。
#[cfg(unix)]
fn chmod_600(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(path)?.permissions();
    perm.set_mode(0o600);
    std::fs::set_permissions(path, perm)
        .with_context(|| format!("无法设置 {} 权限为 600", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn chmod_600(_path: &Path) -> Result<()> {
    Ok(())
}
