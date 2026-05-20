//! `sjtu mail` 命令 handler：5 个只读 async fn + 默认分页常量。
//!
//! 红线：
//! - 所有调用均只读（inbox / unread / keyword / get_msg）
//! - `get_msg` builder 编译期保证 read=0（不会标记已读）
//! - 空参数（id / keyword）直接返回 `InvalidInput`，不到网络层

use anyhow::Result;

use super::data::{MailListData, MailReadData};
use crate::apps::mail::MailClient;
use crate::cookies::load_session;
use crate::error::SjtuCliError;
use crate::output::{render, Envelope, EnvelopeMeta, OutputFormat};

/// 默认分页大小（`--limit` 未指定时）。
pub const DEFAULT_LIMIT: u32 = 50;

/// 构造 MailClient：load session → 检查软 TTL → SSO 跟链。
async fn make_client() -> Result<MailClient> {
    let session = load_session()?;
    if session.is_expired() {
        return Err(SjtuCliError::SessionExpired.into());
    }
    MailClient::connect(&session).await
}

/// mail 命令统一的 Envelope 元数据。
fn mail_meta() -> EnvelopeMeta {
    EnvelopeMeta {
        via: Some("zimbra_soap".into()),
        source_hint: Some("mail.sjtu.edu.cn".into()),
    }
}

/// `sjtu mail list`：inbox 列表（分页）。
pub async fn cmd_mails(limit: u32, offset: u32, fmt: Option<OutputFormat>) -> Result<()> {
    let client = make_client().await?;
    let items = client.inbox(limit, offset).await?;
    let data = MailListData {
        count: items.len(),
        offset,
        items: items.into_iter().map(|m| m.into()).collect(),
    };
    render(Envelope::ok_with_meta(data, mail_meta()), fmt)
}

/// `sjtu mail list --unread`：仅未读邮件（分页）。
pub async fn cmd_mails_unread(limit: u32, offset: u32, fmt: Option<OutputFormat>) -> Result<()> {
    let client = make_client().await?;
    let items = client.unread(limit, offset).await?;
    let data = MailListData {
        count: items.len(),
        offset,
        items: items.into_iter().map(|m| m.into()).collect(),
    };
    render(Envelope::ok_with_meta(data, mail_meta()), fmt)
}

/// `sjtu mail list --search <keyword>`：关键字搜索（分页）。
///
/// 空 keyword 直接返回 `InvalidInput`，不发网络请求。
pub async fn cmd_mails_search(
    keyword: &str,
    limit: u32,
    offset: u32,
    fmt: Option<OutputFormat>,
) -> Result<()> {
    if keyword.trim().is_empty() {
        return Err(SjtuCliError::InvalidInput("--search 关键字不能为空".into()).into());
    }
    let client = make_client().await?;
    let items = client.keyword(keyword, limit, offset).await?;
    let data = MailListData {
        count: items.len(),
        offset,
        items: items.into_iter().map(|m| m.into()).collect(),
    };
    render(Envelope::ok_with_meta(data, mail_meta()), fmt)
}

/// `sjtu mail read <id>`：读单封邮件正文（**read=0，不标已读**）。
///
/// 空 id 直接返回 `InvalidInput`，不发网络请求。
pub async fn cmd_mail_read(msg_id: &str, fmt: Option<OutputFormat>) -> Result<()> {
    if msg_id.trim().is_empty() {
        return Err(SjtuCliError::InvalidInput("邮件 id 不能为空".into()).into());
    }
    let client = make_client().await?;
    let full = client.get_msg(msg_id).await?;
    let data: MailReadData = full.into();
    render(Envelope::ok_with_meta(data, mail_meta()), fmt)
}
