//! SOAP 响应 XML → domain model parser。流式 quick-xml。
//!
//! `parse_search_response`：SearchResponse → `Vec<Mail>`（邮件列表页）
//! `parse_get_msg_response`：GetMsgResponse → `MailFull`（邮件详情，含 body + 地址）

use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use super::extract::{extract_addresses_by_type, extract_plain_body};
use super::models::{flags_contains_unread, Mail, MailFull};
use crate::error::SjtuCliError;

/// 解析 SearchResponse → Vec<Mail>。流式 quick-xml。
pub(super) fn parse_search_response(xml: &str) -> Result<Vec<Mail>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = Vec::new();
    let mut current: Option<Mail> = None;
    let mut text_target: Option<&'static str> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"m" => {
                    let mut m = Mail::default();
                    for attr in e.attributes().flatten() {
                        let key = attr.key.as_ref();
                        let val = attr.unescape_value().unwrap_or_default().to_string();
                        match key {
                            b"id" => m.id = val,
                            b"l" => m.folder_id = Some(val),
                            b"cid" => m.conversation_id = Some(val),
                            b"f" => {
                                m.unread = flags_contains_unread(Some(&val));
                                m.flags = Some(val);
                            }
                            b"s" => m.size_bytes = val.parse().ok(),
                            b"d" => m.date_ms = val.parse().ok(),
                            _ => {}
                        }
                    }
                    current = Some(m);
                }
                b"su" => text_target = Some("su"),
                b"fr" => text_target = Some("fr"),
                b"e" => {
                    if let Some(m) = current.as_mut() {
                        let mut addr = None;
                        let mut disp = None;
                        let mut typ = String::new();
                        for attr in e.attributes().flatten() {
                            let key = attr.key.as_ref();
                            let val = attr.unescape_value().unwrap_or_default().to_string();
                            match key {
                                b"a" => addr = Some(val),
                                b"d" => disp = Some(val),
                                b"t" => typ = val,
                                _ => {}
                            }
                        }
                        if typ == "f" {
                            m.from_address = addr;
                            m.from_display = disp;
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Text(t)) => {
                if let (Some(tag), Some(m)) = (text_target, current.as_mut()) {
                    let txt = t.unescape().unwrap_or_default().to_string();
                    match tag {
                        "su" => m.subject = Some(txt),
                        "fr" => m.fragment = Some(txt),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"m" => {
                    if let Some(m) = current.take() {
                        out.push(m);
                    }
                }
                b"su" | b"fr" => text_target = None,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(SjtuCliError::UpstreamError(format!(
                    "SearchResponse XML parse 失败: {e}"
                ))
                .into())
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

/// 解析 GetMsgResponse → MailFull（meta + body_plain）。
pub(super) fn parse_get_msg_response(xml: &str) -> Result<MailFull> {
    let mails = parse_search_response(xml)?;
    let meta = mails
        .into_iter()
        .next()
        .ok_or_else(|| SjtuCliError::UpstreamError("GetMsgResponse 缺 <m> 元素".to_string()))?;

    // 抓 text/plain part 的 <content>。简单实现：找第一个 ct="text/plain" 的 <mp>
    // 紧跟着的 <content>...</content> 文本。
    let body_plain = extract_plain_body(xml);

    Ok(MailFull {
        meta,
        body_plain,
        to_addresses: extract_addresses_by_type(xml, "t"),
        cc_addresses: extract_addresses_by_type(xml, "c"),
    })
}
