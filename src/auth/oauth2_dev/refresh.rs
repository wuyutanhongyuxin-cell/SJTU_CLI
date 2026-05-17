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
