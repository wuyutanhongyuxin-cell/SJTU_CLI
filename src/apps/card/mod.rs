//! 一卡通子系统 (`api.sjtu.edu.cn/v1/me/card*`)：余额 + 消费记录只读 API client。
//!
//! 鉴权链：OAuth2 Authorization Code (auth/oauth2_dev/) → access_token → Authorization: Bearer
//! 与 elec / services / shuiyuan / canvas 等 cookie-based 子系统不同；专属 helper 见
//! `auth/oauth2_dev/refresh.rs::with_token_refresh`。
//!
//! 红线：余额查询 + 消费记录 only。挂失 / 充值 / 改密码 / 改照片 写端点 spec §NG1 永不实装。

pub mod http;
pub mod models;
pub mod throttle;

#[cfg(test)]
mod tests_parse;
