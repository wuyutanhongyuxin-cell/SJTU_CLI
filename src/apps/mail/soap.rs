//! SOAP envelope builder（只读白名单 4 类）。
//!
//! **红线**：本文件只实装 4 个**只读** request builder。
//! 编译期就找不到 SendMsgRequest / SaveDraftRequest / *ActionRequest 的入口。
//!
//! Envelope 通用骨架：
//! ```xml
//! <?xml version="1.0" encoding="UTF-8"?>
//! <soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope">
//!   <soap:Header>
//!     <context xmlns="urn:zimbra"><authToken>{TOKEN}</authToken></context>
//!   </soap:Header>
//!   <soap:Body>...</soap:Body>
//! </soap:Envelope>
//! ```

use quick_xml::escape::escape;

/// 通用 envelope wrapper。
fn wrap_envelope(auth_token: &str, body_xml: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope">
  <soap:Header><context xmlns="urn:zimbra"><authToken>{token}</authToken></context></soap:Header>
  <soap:Body>{body}</soap:Body>
</soap:Envelope>"#,
        token = escape(auth_token),
        body = body_xml
    )
}

/// SearchRequest envelope。query 必须 XML escape。
pub(super) fn build_search_envelope(
    auth_token: &str,
    query: &str,
    limit: u32,
    offset: u32,
) -> String {
    let body = format!(
        r#"<SearchRequest xmlns="urn:zimbraMail" types="message" limit="{limit}" offset="{offset}" sortBy="dateDesc"><query>{q}</query></SearchRequest>"#,
        q = escape(query)
    );
    wrap_envelope(auth_token, &body)
}

/// GetMsgRequest envelope。**强制 read="0" html="0" max="50000"**（红线注入）。
pub(super) fn build_get_msg_envelope(auth_token: &str, msg_id: &str) -> String {
    let body = format!(
        r#"<GetMsgRequest xmlns="urn:zimbraMail"><m id="{id}" read="0" html="0" max="50000"/></GetMsgRequest>"#,
        id = escape(msg_id)
    );
    wrap_envelope(auth_token, &body)
}

/// GetFolderRequest envelope。从 root (l=1) 拿可见 folder 树。
pub(super) fn build_get_folder_envelope(auth_token: &str) -> String {
    let body = r#"<GetFolderRequest xmlns="urn:zimbraMail" visible="1"><folder l="1"/></GetFolderRequest>"#;
    wrap_envelope(auth_token, body)
}

/// 检测 SOAP Fault Code = service.AUTH_REQUIRED。
/// 早于完整 parser，让 http 层快速识别 stale 信号。
pub(super) fn is_auth_required_fault(xml: &str) -> bool {
    xml.contains("service.AUTH_REQUIRED")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_envelope_contains_auth_and_query() {
        let env = build_search_envelope("TOK123", "in:inbox", 50, 0);
        assert!(env.contains("<authToken>TOK123</authToken>"));
        assert!(env.contains("<query>in:inbox</query>"));
        assert!(env.contains(r#"limit="50""#));
        assert!(env.contains(r#"sortBy="dateDesc""#));
    }

    #[test]
    fn search_envelope_escapes_xml_special_in_query() {
        let env = build_search_envelope("TOK", "foo<bar>&baz", 5, 0);
        assert!(!env.contains("foo<bar>"));
        assert!(env.contains("foo&lt;bar&gt;&amp;baz"));
    }

    #[test]
    fn get_msg_envelope_forces_red_line_attrs() {
        let env = build_get_msg_envelope("TOK", "3084");
        assert!(env.contains(r#"id="3084""#));
        assert!(env.contains(r#"read="0""#), "红线: read 必须等于 0");
        assert!(env.contains(r#"html="0""#));
        assert!(env.contains(r#"max="50000""#));
    }

    #[test]
    fn auth_required_fault_detection() {
        let xml = "<soap:Fault><Code>service.AUTH_REQUIRED</Code></soap:Fault>";
        assert!(is_auth_required_fault(xml));
        assert!(!is_auth_required_fault(
            "<SearchResponse><m id=\"1\"/></SearchResponse>"
        ));
    }
}
