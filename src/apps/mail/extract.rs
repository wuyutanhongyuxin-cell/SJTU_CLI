//! parser 内部辅助：从 SOAP XML body 抽取 text/plain body + 收件人/抄送地址列表。
//!
//! 两个函数都对传入的完整 GetMsgResponse XML 做独立的流式扫描，
//! 因此调用两次也是安全的（各自持有独立 Reader）。

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use super::models::Address;

/// 从 GetMsgResponse XML 中找第一个 ct="text/plain" 的 `<mp>` 块内的 `<content>` 文本。
/// 找不到返回 None（比如纯 HTML 邮件）。
pub(super) fn extract_plain_body(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut in_plain_part = false;
    let mut depth_in_part: i32 = 0;
    let mut in_content = false;
    let mut acc = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"mp" {
                    let mut is_plain = false;
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"ct" {
                            let v = attr.unescape_value().unwrap_or_default();
                            if v == "text/plain" {
                                is_plain = true;
                            }
                        }
                    }
                    if is_plain && depth_in_part == 0 {
                        in_plain_part = true;
                        depth_in_part = 1;
                    } else if in_plain_part {
                        depth_in_part += 1;
                    }
                } else if in_plain_part && e.name().as_ref() == b"content" {
                    in_content = true;
                }
            }
            Ok(Event::Text(t)) if in_content => {
                acc.push_str(&t.unescape().unwrap_or_default());
            }
            Ok(Event::CData(t)) if in_content => {
                acc.push_str(&String::from_utf8_lossy(t.as_ref()));
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"content" => in_content = false,
                b"mp" if in_plain_part => {
                    depth_in_part -= 1;
                    if depth_in_part == 0 {
                        break;
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    if acc.is_empty() {
        None
    } else {
        Some(acc)
    }
}

/// 从 GetMsgResponse XML 中提取指定类型（t="t" 收件人 / t="c" 抄送）的地址列表。
pub(super) fn extract_addresses_by_type(xml: &str, t_value: &str) -> Vec<Address> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut out = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.name().as_ref() == b"e" => {
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
                if typ == t_value {
                    if let Some(a) = addr {
                        out.push(Address {
                            address: a,
                            display: disp,
                        });
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}
