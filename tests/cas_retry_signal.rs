//! 跨模块 sanity：SubSessionStale variant 的 downcast 在 anyhow 链上保留。
//! 防 ical/handler.rs::fetch_all 之类的 fail-soft 路径再次破坏 downcast 链。

use sjtu_cli::error::SjtuCliError;

#[test]
fn sub_session_stale_survives_anyhow_boxing() {
    let err: anyhow::Error = SjtuCliError::SubSessionStale("jwc").into();
    let downcasted = err.downcast_ref::<SjtuCliError>();
    assert!(matches!(
        downcasted,
        Some(SjtuCliError::SubSessionStale("jwc"))
    ));
}

#[test]
fn sub_session_stale_survives_context_wrapping() {
    use anyhow::Context;
    let err: anyhow::Error = SjtuCliError::SubSessionStale("jwc").into();
    let wrapped = Err::<(), _>(err).context("额外上下文");
    // 加 context 后 root cause 仍可 downcast
    let err2 = wrapped.unwrap_err();
    let downcasted = err2.downcast_ref::<SjtuCliError>();
    assert!(matches!(
        downcasted,
        Some(SjtuCliError::SubSessionStale("jwc"))
    ));
}

#[test]
fn sub_session_stale_does_not_survive_string_reraise() {
    // 反例：用 anyhow!("{}", err) 重 raise 破坏 downcast 链 —— 这是 ical fetch_all
    // 老 bug 路径。本测保证 plan T6 的 fix（重 raise variant 而非 string）的 invariant。
    let err: anyhow::Error = SjtuCliError::SubSessionStale("jwc").into();
    let reraised_err = anyhow::anyhow!("{:#}", err);
    let downcasted = reraised_err.downcast_ref::<SjtuCliError>();
    assert!(
        downcasted.is_none(),
        "string format reraise 应破坏 downcast 链"
    );
}
