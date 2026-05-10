//! V5.A 业务失败回退（token 失效自动清缓存重 launch）的共享 helper。
//!
//! 从 handlers.rs 抽出来，因 cmd_list / cmd_download / cmd_download_all 都要套，
//! 而 handlers.rs 已贴 200 行硬限，不能再扩。

use std::future::Future;
use std::sync::Arc;

use anyhow::Result;

use crate::apps::canvas_video::{cache, Client};

/// V5.A 业务失败回退：套外面跑一次 op；首次抛 token-invalid 错时清 cache 再跑一次。
/// `looks_like_token_invalid` 决定哪些错触发重试；其他错原封返。
///
/// 用法（cmd_list / cmd_download）：
/// ```ignore
/// with_token_refresh(course_id, tool_id, |client| async move {
///     client.list_lectures(client.cour_id(), client.lti_course_id()).await
/// }).await?
/// ```
#[allow(dead_code)]
pub(super) async fn with_token_refresh<F, Fut, T>(
    course_id: u64,
    lti_tool_id: u64,
    op: F,
) -> Result<T>
where
    F: Fn(Arc<Client>) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let client = Arc::new(Client::connect(course_id, lti_tool_id).await?);
    match op(client.clone()).await {
        Ok(v) => Ok(v),
        Err(e) if looks_like_token_invalid(&e) => {
            tracing::warn!(course_id, lti_tool_id, error = %e, "token 疑作废，清 cache 重试");
            cache::clear(Some(course_id), Some(lti_tool_id))?;
            let client2 = Arc::new(Client::connect(course_id, lti_tool_id).await?);
            op(client2).await
        }
        Err(e) => Err(e),
    }
}

/// 哪些错信号意味着 token 失效该清缓存重 launch。误判成本：多跑一次 ~21s LTI launch；
/// 漏判成本：把过期错原样上抛给用户。前者优于后者，分类宁宽勿严。
#[allow(dead_code)]
fn looks_like_token_invalid(e: &anyhow::Error) -> bool {
    let s = e.to_string();
    s.contains("业务失败 code") || s.contains("401") || s.contains("403") || s.contains("未授权")
}
