//! `sjtu card <sub>` clap 枚举 + 派发。
//!
//! MVP 3 个子命令（均**只读**）：
//! - `auth --client-id <ID>` —— 首次 OAuth2 授权流（弹浏览器同意）
//! - `balance [--with-identity] [--via auto|oauth2|weixin]` —— 当前卡余额
//! - `history [--days N] [--limit M] [--via auto|oauth2|weixin]` —— 消费记录
//!
//! 红线：充值 / 挂失 / 解挂 / 改密码 / 改照片 全不实装（spec §NG1 永久排除）。

use anyhow::Result;
use clap::Subcommand;

use crate::apps::card::via::CardVia;
use crate::commands::card::handlers as card_cmds;
use crate::output::OutputFormat;

#[derive(Debug, Subcommand)]
pub enum CardSub {
    /// 首次 OAuth2 授权（弹浏览器同意）。clientId 来自 developer.sjtu.edu.cn 申请。
    Auth {
        /// 开发者平台批准的 client_id（公开信息，可入命令行）。
        /// 客户端密钥 client_secret 由 `~/.sjtu-cli/card_oauth_secret.txt` 独立存放。
        #[arg(long)]
        client_id: String,
    },

    /// 当前卡余额查询。**只读**。
    ///
    /// 默认抹身份字段；`--with-identity` 出学号/姓名/单位/绑定银行卡（前 4 + **** + 后 4）（OAuth2 path 限定）。
    Balance {
        /// 包含身份字段（学号 / 姓名 / 单位 / 银行卡尾号）。默认不出。OAuth2 path 限定。
        #[arg(long, default_value_t = false)]
        with_identity: bool,
        /// 鉴权路径：auto（默认，无 OAuth2 token 走 weixin）/ oauth2 / weixin。
        #[arg(long, value_enum, default_value_t = CardVia::Auto)]
        via: CardVia,
    },

    /// 消费记录查询。**只读**。
    History {
        /// 时间窗口天数，默认 30，最大 365。
        #[arg(long, default_value_t = 30)]
        days: u32,
        /// 单次最多返回多少条，默认 50，服务端硬限 100，CLI 自动 clamp。
        #[arg(long, default_value_t = 50)]
        limit: u32,
        /// 鉴权路径：auto（默认，无 OAuth2 token 走 weixin）/ oauth2 / weixin。
        #[arg(long, value_enum, default_value_t = CardVia::Auto)]
        via: CardVia,
    },
}

pub async fn dispatch(sub: CardSub, fmt: Option<OutputFormat>) -> Result<()> {
    match sub {
        CardSub::Auth { client_id } => card_cmds::cmd_auth(client_id, fmt).await,
        CardSub::Balance { with_identity, via } => {
            card_cmds::cmd_balance(with_identity, via, fmt).await
        }
        CardSub::History { days, limit, via } => {
            card_cmds::cmd_history(days, limit, via, fmt).await
        }
    }
}
