use mail_parser::decoders::html::html_to_text;
use mail_parser::{Message, MessageParser, MimeHeaders};

const SNIPPET_LEN: usize = 160;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedHeaders {
    pub message_id: Option<String>,
    pub references: Option<String>,
    pub from_addr: String,
    pub from_name: String,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub subject: String,
    pub date: i64,
    pub snippet: String,
    pub has_attachments: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedAttachment {
    pub filename: String,
    pub mime_type: String,
    pub size: usize,
    pub content_id: Option<String>,
    pub is_inline: bool,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedMessage {
    pub message_id: Option<String>,
    pub references: Option<String>,
    pub from_addr: String,
    pub from_name: String,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub subject: String,
    pub date: i64,
    pub snippet: String,
    pub text_body: String,
    pub html_body: Option<String>,
    pub attachments: Vec<ParsedAttachment>,
}

pub fn parse_headers(raw: &[u8]) -> ParsedHeaders {
    match MessageParser::default().parse(raw) {
        Some(message) => {
            let mut headers = headers_from(&message);
            headers.has_attachments = has_attachment_hint(raw);
            headers
        }
        None => ParsedHeaders::default(),
    }
}

pub fn parse_headers_with_preview(raw: &[u8], preview: &[u8]) -> ParsedHeaders {
    let mut headers = parse_headers(raw);
    if preview.is_empty() {
        return headers;
    }
    let mut combined = Vec::with_capacity(raw.len() + preview.len());
    combined.extend_from_slice(raw);
    combined.extend_from_slice(preview);
    let Some(message) = MessageParser::default().parse(&combined) else {
        return headers;
    };
    headers.has_attachments |= has_attachment_hint(&combined);
    let text = message
        .body_text(0)
        .map(|body| body.into_owned())
        .or_else(|| message.body_html(0).map(|body| html_to_text(&body)));
    if let Some(text) = text {
        headers.snippet = snippet_from_text(&text);
    }
    headers
}

pub fn parse_message(raw: &[u8]) -> ParsedMessage {
    let Some(message) = MessageParser::default().parse(raw) else {
        return ParsedMessage::default();
    };
    let headers = headers_from(&message);
    let text_body = message
        .body_text(0)
        .map(|body| body.into_owned())
        .unwrap_or_default();
    let html_body = message.body_html(0).map(|body| body.into_owned());
    let attachments = message
        .attachments()
        .map(|part| {
            let data = part.contents().to_vec();
            ParsedAttachment {
                filename: part.attachment_name().unwrap_or("attachment").to_string(),
                mime_type: mime_type(part),
                size: data.len(),
                content_id: part.content_id().map(str::to_string),
                is_inline: part
                    .content_disposition()
                    .is_some_and(|disposition| disposition.is_inline()),
                data,
            }
        })
        .collect();

    ParsedMessage {
        message_id: headers.message_id,
        references: headers.references,
        from_addr: headers.from_addr,
        from_name: headers.from_name,
        to_addrs: headers.to_addrs,
        cc_addrs: headers.cc_addrs,
        subject: headers.subject,
        date: headers.date,
        snippet: headers.snippet,
        text_body,
        html_body,
        attachments,
    }
}

fn headers_from(message: &Message<'_>) -> ParsedHeaders {
    let from = message.from().and_then(|address| address.first());
    let from_addr = from
        .and_then(|address| address.address())
        .unwrap_or_default()
        .to_string();
    let from_name = from
        .and_then(|address| address.name())
        .unwrap_or_default()
        .to_string();
    let to_addrs = addresses(message, message.to());
    let cc_addrs = addresses(message, message.cc());

    ParsedHeaders {
        message_id: message.message_id().map(str::to_string),
        references: references(message),
        from_addr,
        from_name,
        to_addrs,
        cc_addrs,
        subject: message.subject().unwrap_or_default().to_string(),
        date: message.date().map(|date| date.to_timestamp()).unwrap_or(0),
        snippet: snippet(message),
        has_attachments: message.attachment_count() > 0,
    }
}

fn addresses(_message: &Message<'_>, address: Option<&mail_parser::Address<'_>>) -> Vec<String> {
    address
        .map(|address| {
            address
                .iter()
                .filter_map(|addr| addr.address().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn references(message: &Message<'_>) -> Option<String> {
    let value = message.references();
    if let Some(items) = value.as_text_list() {
        let chain = items
            .iter()
            .map(|item| normalized_reference(item))
            .collect::<Vec<_>>()
            .join(" ");
        if !chain.is_empty() {
            return Some(chain);
        }
    }
    value
        .as_text()
        .map(|text| {
            text.split_whitespace()
                .map(normalized_reference)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|value| !value.is_empty())
}

fn normalized_reference(value: &str) -> String {
    let value = value.trim();
    if value.starts_with('<') && value.ends_with('>') {
        value.to_string()
    } else {
        format!("<{value}>")
    }
}

fn snippet(message: &Message<'_>) -> String {
    let text = message
        .body_text(0)
        .map(|body| body.into_owned())
        .or_else(|| message.body_html(0).map(|body| html_to_text(&body)))
        .unwrap_or_default();
    snippet_from_text(&text)
}

fn snippet_from_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(SNIPPET_LEN)
        .collect()
}

fn mime_type(part: &mail_parser::MessagePart<'_>) -> String {
    part.content_type()
        .map(|content_type| match content_type.subtype() {
            Some(subtype) => format!("{}/{}", content_type.ctype(), subtype),
            None => content_type.ctype().to_string(),
        })
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

fn has_attachment_hint(raw: &[u8]) -> bool {
    let text = String::from_utf8_lossy(raw).to_ascii_lowercase();
    text.contains("content-disposition: attachment")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_subject_sender_and_plain_body() {
        let raw = b"From: Ada <ada@example.com>\r\nSubject: Hello\r\nMessage-ID: <1@example.com>\r\nDate: Tue, 18 Oct 2026 10:00:00 +0000\r\nContent-Type: text/plain\r\n\r\nHi there\r\n";
        let message = parse_message(raw);
        assert_eq!(message.subject, "Hello");
        assert_eq!(message.from_addr, "ada@example.com");
        assert_eq!(message.text_body.trim(), "Hi there");
    }

    #[test]
    fn parses_headers_for_sync() {
        let raw = b"From: Ada Lovelace <ada@example.com>\r\nTo: Grace Hopper <grace@example.com>, me@example.com\r\nCc: team@example.com\r\nSubject: Hello\r\nMessage-ID: <1@example.com>\r\nDate: Tue, 18 Oct 2026 10:00:00 +0000\r\nContent-Type: multipart/mixed; boundary=\"b\"\r\n\r\n--b--\r\n";
        let headers = parse_headers(raw);
        assert_eq!(headers.message_id.as_deref(), Some("1@example.com"));
        assert_eq!(headers.from_name, "Ada Lovelace");
        assert_eq!(headers.from_addr, "ada@example.com");
        assert_eq!(
            headers.to_addrs,
            vec!["grace@example.com", "me@example.com"]
        );
        assert_eq!(headers.cc_addrs, vec!["team@example.com"]);
        assert!(!headers.has_attachments);
        assert!(headers.date > 1_700_000_000);
    }

    #[test]
    fn preview_detects_attachment_parts_without_trusting_multipart_alone() {
        let raw = b"From: Ada <ada@example.com>\r\nSubject: Files\r\nContent-Type: multipart/mixed; boundary=\"b\"\r\n\r\n";
        let preview = b"--b\r\nContent-Type: text/plain\r\n\r\nHello\r\n--b\r\nContent-Type: application/pdf; name=\"report.pdf\"\r\nContent-Disposition: attachment; filename=\"report.pdf\"\r\n\r\n";
        let headers = parse_headers_with_preview(raw, preview);
        assert!(headers.has_attachments);
    }

    #[test]
    fn inline_multipart_without_attachment_disposition_is_not_an_attachment() {
        let raw = b"From: Ada <ada@example.com>\r\nSubject: Newsletter\r\nContent-Type: multipart/related; boundary=\"b\"\r\n\r\n";
        let preview = b"--b\r\nContent-Type: text/html\r\n\r\n<p>Hello</p>\r\n--b\r\nContent-Type: image/png\r\nContent-ID: <logo@example>\r\n\r\n";
        let headers = parse_headers_with_preview(raw, preview);
        assert!(!headers.has_attachments);
    }

    #[test]
    fn extracts_html_and_attachment_metadata() {
        let raw = b"From: Ada <ada@example.com>\r\nSubject: Files\r\nMessage-ID: <2@example.com>\r\nContent-Type: multipart/mixed; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: text/html\r\n\r\n<p>Hello</p>\r\n--b\r\nContent-Type: application/pdf; name=\"report.pdf\"\r\nContent-Disposition: attachment; filename=\"report.pdf\"\r\nContent-Transfer-Encoding: base64\r\nContent-ID: <att-1>\r\n\r\naGVsbG8=\r\n--b--\r\n";
        let message = parse_message(raw);
        assert!(message
            .html_body
            .as_deref()
            .is_some_and(|html| html.contains("<p>Hello</p>")));
        assert_eq!(message.attachments.len(), 1);
        let attachment = &message.attachments[0];
        assert_eq!(attachment.filename, "report.pdf");
        assert_eq!(attachment.mime_type, "application/pdf");
        assert_eq!(attachment.size, 5);
        assert_eq!(attachment.content_id.as_deref(), Some("att-1"));
    }

    #[test]
    fn headers_use_fetched_text_preview_for_snippet() {
        let raw =
            b"From: Ada <ada@example.com>\r\nSubject: Preview\r\nContent-Type: text/plain\r\n\r\n";
        let headers = parse_headers_with_preview(raw, b"Hello from the body preview");
        assert_eq!(headers.subject, "Preview");
        assert_eq!(headers.snippet, "Hello from the body preview");
    }

    #[test]
    fn snippet_is_plain_text_and_limited_to_160_characters() {
        let long = "x".repeat(300);
        let raw = format!(
            "From: Ada <ada@example.com>\r\nSubject: Long\r\nContent-Type: text/plain\r\n\r\n{long}\r\n"
        );
        let message = parse_message(raw.as_bytes());
        assert_eq!(message.snippet.chars().count(), 160);
        assert!(!message.snippet.contains('\r'));
        assert!(!message.snippet.contains('\n'));
    }

    #[test]
    fn parses_references_chains_from_headers() {
        let raw = b"From: Ada <ada@example.com>\r\nSubject: Hello\r\nMessage-ID: <3@example.com>\r\nReferences: <1@example.com> <2@example.com>\r\nContent-Type: text/plain\r\n\r\nHi\r\n";
        let headers = parse_headers(raw);
        assert_eq!(
            headers.references.as_deref(),
            Some("<1@example.com> <2@example.com>")
        );
        assert_eq!(
            parse_message(raw).references.as_deref(),
            Some("<1@example.com> <2@example.com>")
        );
    }

    #[test]
    fn preview_decodes_single_part_html_without_markup() {
        let raw =
            b"From: Ada <ada@example.com>\r\nSubject: Preview\r\nContent-Type: text/html\r\n\r\n";
        let headers = parse_headers_with_preview(raw, b"<p>Hello <b>world</b></p>");
        assert_eq!(headers.snippet, "Hello world");
    }

    #[test]
    fn preview_reads_text_from_multipart_messages() {
        let raw = b"From: Ada <ada@example.com>\r\nSubject: Preview\r\nContent-Type: multipart/mixed; boundary=\"b\"\r\n\r\n";
        let preview = b"--b\r\nContent-Type: text/plain\r\n\r\nHello from the body\r\n--b--\r\n";
        let headers = parse_headers_with_preview(raw, preview);
        assert_eq!(headers.snippet, "Hello from the body");
    }

    #[test]
    fn malformed_input_falls_back_to_empty_values() {
        let message = parse_message(b"not a message");
        assert_eq!(message.subject, "");
        assert_eq!(message.from_addr, "");
        assert_eq!(message.text_body, "");
        assert!(message.attachments.is_empty());
    }
}
