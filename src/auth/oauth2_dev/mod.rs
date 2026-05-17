//! T4 一卡通 OAuth2 Authorization Code 通道（RFC6749 标准）。
//!
//! 不用 `oauth2` crate（违 CLAUDE.md 不引入新依赖）。
//! 不用 `keyring`（跨平台行为不一致；JSON+chmod 600 与 cookies::session.json 同制，单一可审计点）。
//! 不用 `axum`（1 endpoint 不值得引入 micro-framework；手卷 60 行 listener 够用）。
//! Refresh 走 failure-driven 不走 timer（同 canvas_video::with_token_refresh 范式，省状态机）。
//!
//! 与现 `src/auth/oauth2/` 完全不同：那个是 shuiyuan 用的 302-chain 跟链，
//! 终点取 Discourse 的 `_t` cookie；本模块走 code-for-token 拿 Bearer access_token。

pub mod secret;
