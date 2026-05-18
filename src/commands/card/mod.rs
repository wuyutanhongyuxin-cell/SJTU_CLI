//! `sjtu card <sub>` handler：OAuth2 鉴权下的一卡通余额 + 消费记录只读命令。

pub mod data;
pub mod data_weixin;
pub mod handlers;
mod refresh_helper;
