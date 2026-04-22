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
        }
    }
}
