//! 登录模块：S1 阶段的扫码登录主链路 + rookie 兜底。

pub mod browser_extract;
pub mod cas;
pub mod oauth2;
pub mod qr_login;
pub mod qr_render;

use anyhow::Result;

use crate::cookies::Session;

/// 登录后端选择。对应 CLI `--browser` 参数。
#[derive(Debug, Clone, Copy)]
pub enum Backend {
    /// 启动可见 Chrome，终端 + 窗口双展示 QR。主流程。
    Chrome,
    /// 从本机浏览器的 cookie 库直接读（rookie crate）。Chrome 启不起来时兜底。
    Rookie,
}

/// 统一登录入口：按 backend 走不同子流程，成功返回 `Session` 并已落盘。
pub fn login(backend: Backend) -> Result<Session> {
    match backend {
        Backend::Chrome => qr_login::login_with_chrome(),
        Backend::Rookie => browser_extract::login_via_rookie(),
    }
}
