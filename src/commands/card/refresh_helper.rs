//! 包装层：load session → 若 stale 先 refresh → 调 op；op 抛 token_expired 再 refresh + 重试。
//!
//! 抽出为独立文件，避免 handlers.rs 超 200 行硬限（CLAUDE.md §1 行数硬限制）。

use anyhow::Result;

use crate::apps::card::Client;
use crate::auth::oauth2_dev::{self, is_token_stale, refresh};
use crate::error::SjtuCliError;

/// 包装层：load session → 若 stale 先 refresh → 调 op；op 抛 token_expired 再 refresh + 重试一次。
pub async fn ensure_fresh_and_call<F, Fut, T>(op: F) -> Result<T>
where
    F: Fn(Client) -> Fut + Send + 'static + Clone,
    Fut: std::future::Future<Output = Result<T>> + Send,
    T: Send + 'static,
{
    let http_refresh = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| SjtuCliError::NetworkError(format!("构造 http: {e}")))?;
    {
        let sess = oauth2_dev::load_session()?;
        if is_token_stale(&sess) {
            tracing::info!("oauth2_dev: token 预检已 stale，触发 refresh");
            oauth2_dev::refresh_and_save(&http_refresh).await?;
        }
    }
    let op2 = op.clone();
    refresh::with_token_refresh(
        move || {
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
