//! 统一异常：库层用 `SjtuCliError`（thiserror 派生），bin 层用 anyhow 收口。
//!
//! 每个 variant 都能映射到 Envelope 的 `error.code`（见 `code()` 方法）。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SjtuCliError {
    /// 未登录：`~/.sjtu-cli/session.json` 不存在。
    #[error("未登录。请先运行 `sjtu login` 扫码。")]
    NotAuthenticated,

    /// Session 过期：本地软 TTL 已过 或 上游返回 401 / 302 到登录页。
    #[error("登录已过期。请重新运行 `sjtu login`。")]
    SessionExpired,

    /// 子系统不可达（CAS 跳转失败 / 子系统 500）。
    #[error("子系统 `{0}` 不可达：{1}")]
    SubSystemUnreachable(&'static str, String),

    /// 上游返回了非预期的内容（HTML 改版 / JSON 缺字段）。
    #[error("上游响应解析失败：{0}")]
    UpstreamError(String),

    /// 参数无效。
    #[error("参数无效：{0}")]
    InvalidInput(String),

    /// 网络层错误。
    #[error("网络错误：{0}")]
    NetworkError(String),

    /// OAuth2 跟链失败：主 session 有效但无法在水源落到 `_t`。
    /// 典型触发：停在 jaccount 授权确认页 / 登录页、state 校验失败、callback 无法换 code。
    #[error("OAuth2 登录失败：{0}")]
    OAuth2Failed(String),

    /// 水源 Discourse API 非 2xx 或响应解析失败（保留原始 snippet 便于定位 API 漂移）。
    #[error("水源 API 错误：{0}")]
    ShuiyuanApi(String),

    /// Canvas LMS API 非 2xx 或响应解析失败。
    #[error("Canvas API 错误：{0}")]
    CanvasApi(String),

    /// Canvas PAT 无效或已被 revoke（401）。Envelope code 仍归到 `session_expired`，
    /// 但行动项是 `sjtu canvas setup` 而非 `sjtu login`。
    #[error("Canvas PAT 无效或已过期。请重新运行 `sjtu canvas setup` 生成新 token。")]
    CanvasTokenInvalid,

    /// 子系统 sub_session 客户端 cookie 仍 fresh 但服务端已 timeout
    /// （ZF 30 分钟无活动 / OAuth2 短 TTL 等）。`with_cas_refresh` 捕获此 variant
    /// 触发 clear_sub_session + 重 CAS。`&'static str` 是子系统 name（"jwc" 等）。
    #[error("子系统 `{0}` session 已被服务端失效")]
    SubSessionStale(&'static str),
}

impl SjtuCliError {
    /// Envelope 里的 `error.code`：variant 名 → snake_case。
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotAuthenticated => "not_authenticated",
            Self::SessionExpired => "session_expired",
            Self::SubSystemUnreachable(_, _) => "sub_system_unreachable",
            Self::UpstreamError(_) => "upstream_error",
            Self::InvalidInput(_) => "invalid_input",
            Self::NetworkError(_) => "network_error",
            Self::OAuth2Failed(_) => "oauth2_failed",
            Self::ShuiyuanApi(_) => "shuiyuan_api",
            Self::CanvasApi(_) => "canvas_api",
            Self::CanvasTokenInvalid => "session_expired",
            Self::SubSessionStale(_) => "session_expired",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_session_stale_has_session_expired_code() {
        let e = SjtuCliError::SubSessionStale("jwc");
        assert_eq!(e.code(), "session_expired");
    }

    #[test]
    fn sub_session_stale_carries_subsystem_name() {
        let e = SjtuCliError::SubSessionStale("jwc");
        let s = format!("{e}");
        assert!(s.contains("jwc"), "错误消息应包含子系统 name，实际：{s}");
    }

    #[test]
    fn sub_session_stale_can_be_downcast_from_anyhow() {
        let e: anyhow::Error = SjtuCliError::SubSessionStale("jwc").into();
        let downcasted = e.downcast_ref::<SjtuCliError>();
        assert!(matches!(
            downcasted,
            Some(SjtuCliError::SubSessionStale("jwc"))
        ));
    }
}
