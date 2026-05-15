//! CAS 子系统 stale-detect + auto-refresh retry 通用 helper。
//!
//! ## 为何手卷而非 reqwest-middleware
//!
//! 2026 Rust HTTP 客户端业界 idiomatic 是 `reqwest-middleware` + `RetryableStrategy`
//! trait impl（composable / testable / scale）。SJTU-CLI 选手卷闭包 helper 的 4 条理由：
//!
//! 1. CLAUDE.md 不引入新依赖硬约束（middleware 需 +2 crate）
//! 2. 改造面 ×6 子系统（裸 reqwest::Client → ClientWithMiddleware sweeping refactor）
//! 3. Stateful side-effect（clear_sub_session + 重 CAS）在 stateless RetryableStrategy 里别扭
//! 4. 本轮 scope = 1 个子系统接入，1 处手卷更轻
//!
//! 未来若 retry 场景扩到 4+ 子系统，考虑迁 reqwest-middleware 重做（见
//! docs/superpowers/specs/2026-05-15-cas-retry-layer-design.md §1.3）。
//!
//! 同构 pattern 先例：src/commands/canvas_video/retry.rs::with_token_refresh。

use std::future::Future;

use anyhow::Result;
use tracing::warn;

use super::cas_login;
use crate::cookies::{clear_sub_session, Session};
use crate::error::SjtuCliError;

/// CAS 子系统服务端 stale-detect + auto-refresh retry helper。
///
/// 工作流程：
/// 1. cas_login 拿 session（命中 cache 则直接返）
/// 2. 调 op(session) 跑业务
/// 3. 若返 SubSessionStale → clear_sub_session(name) + 重 cas_login → op(session2)
/// 4. 若仍返 SubSessionStale 或主 session 也挂 → 原样上抛
///
/// **仅适用于 GET only / 幂等只读**操作（CLAUDE.md i.sjtu 硬红线）；
/// 上层不得用此 helper 包 POST/PUT/DELETE。
pub async fn with_cas_refresh<F, Fut, T>(name: &'static str, target_url: &str, op: F) -> Result<T>
where
    F: Fn(Session) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let r = cas_login(name, target_url).await?;
    let target = target_url.to_string();
    with_refresh_inner(r.session, op, || async move {
        clear_sub_session(name)?;
        let r2 = cas_login(name, &target).await?;
        Ok(r2.session)
    })
    .await
    .map(|(v, _refreshed)| v)
}

/// 提取的核心 retry 逻辑：注入 refresh fn，便于单测（不依赖 cas_login）。
///
/// 返回 (op result, 是否触发过 refresh)，refreshed 给测试断言用。
pub(super) async fn with_refresh_inner<F, Fut, T, R, RFut>(
    initial_session: Session,
    op: F,
    refresh: R,
) -> Result<(T, bool)>
where
    F: Fn(Session) -> Fut,
    Fut: Future<Output = Result<T>>,
    R: FnOnce() -> RFut,
    RFut: Future<Output = Result<Session>>,
{
    match op(initial_session).await {
        Ok(v) => Ok((v, false)),
        Err(e) if is_sub_session_stale(&e) => {
            warn!(error = %e, "sub_session 服务端 stale，清缓存重做 CAS");
            let session2 = refresh().await?;
            op(session2).await.map(|v| (v, true))
        }
        Err(e) => Err(e),
    }
}

/// downcast 判定 retry 信号；不依赖错误 message 字符串。
fn is_sub_session_stale(e: &anyhow::Error) -> bool {
    e.downcast_ref::<SjtuCliError>()
        .map(|err| matches!(err, SjtuCliError::SubSessionStale(_)))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cookies::{Cookie, Session};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn fresh_session() -> Session {
        Session::new(vec![Cookie {
            name: "JAAuthCookie".into(),
            value: "0123456789abcdef".into(),
            domain: Some("jaccount.sjtu.edu.cn".into()),
            path: Some("/".into()),
            expires: None,
        }])
    }

    /// 首次返 SubSessionStale → 触发 refresh → 第二次返 Ok → 总体返 Ok + refreshed=true。
    #[tokio::test]
    async fn retry_on_stale_then_ok() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_c = calls.clone();

        let (val, refreshed) = with_refresh_inner(
            fresh_session(),
            |_session| {
                let n = calls_c.fetch_add(1, Ordering::SeqCst);
                async move {
                    if n == 0 {
                        Err(SjtuCliError::SubSessionStale("jwc").into())
                    } else {
                        Ok(42u32)
                    }
                }
            },
            || async { Ok(fresh_session()) },
        )
        .await
        .expect("retry 后 op 应成功");

        assert_eq!(val, 42);
        assert!(refreshed, "应该触发过 refresh");
        assert_eq!(calls.load(Ordering::SeqCst), 2, "op 应被调 2 次");
    }

    /// 首次返 NetworkError（非 stale）→ 不触发 retry → 原样上抛 + refresh 未调。
    #[tokio::test]
    async fn no_retry_on_other_error() {
        let refresh_called = Arc::new(AtomicUsize::new(0));
        let refresh_c = refresh_called.clone();

        let result: Result<(u32, bool)> =
            with_refresh_inner(
                fresh_session(),
                |_session| async move {
                    Err(SjtuCliError::NetworkError("connection reset".into()).into())
                },
                || {
                    refresh_c.fetch_add(1, Ordering::SeqCst);
                    async move { Ok(fresh_session()) }
                },
            )
            .await;

        assert!(result.is_err());
        let err = format!("{:#}", result.unwrap_err());
        assert!(err.contains("connection reset"), "应保留原错信息: {err}");
        assert_eq!(
            refresh_called.load(Ordering::SeqCst),
            0,
            "非 stale 不应触发 refresh"
        );
    }

    /// 首次 stale → refresh 成功 → 第二次仍 stale → 第二次错原样上抛。
    #[tokio::test]
    async fn retry_then_fail_returns_second_error() {
        let result: Result<(u32, bool)> = with_refresh_inner(
            fresh_session(),
            |_session| async move { Err(SjtuCliError::SubSessionStale("jwc").into()) },
            || async { Ok(fresh_session()) },
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        let downcasted = err.downcast_ref::<SjtuCliError>();
        assert!(
            matches!(downcasted, Some(SjtuCliError::SubSessionStale(_))),
            "应是第二次的 SubSessionStale 错，实际：{err:#}"
        );
    }
}
