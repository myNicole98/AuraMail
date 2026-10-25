use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use lettre::message::{header::ContentType, Attachment, Mailbox, Message, MultiPart, SinglePart};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::db::{self, Database};
use crate::error::{AppError, AppResult};

pub const MAX_ATTACHMENT_TOTAL: usize = 25 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentUpload {
    pub name: String,
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InlineImageUpload {
    pub name: String,
    pub mime_type: String,
    pub data: String,
    pub content_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeInput {
    pub account_id: i64,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub html_body: Option<String>,
    #[serde(default)]
    pub inline_images: Vec<InlineImageUpload>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub attachments: Vec<AttachmentUpload>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub id: i64,
    pub account_id: i64,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub subject: String,
    pub body: String,
    pub body_html: Option<String>,
    pub inline_images: Vec<InlineImageUpload>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub attachments: Vec<AttachmentUpload>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftSummary {
    pub id: i64,
    pub account_id: i64,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub subject: String,
    pub snippet: String,
    pub has_attachments: bool,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAttachment {
    name: String,
    mime_type: String,
    #[serde(alias = "bytes")]
    data: String,
}

#[allow(dead_code)]
pub fn build_message(input: &ComposeInput, from: &str) -> AppResult<Vec<u8>> {
    Ok(build(input, from)?.formatted())
}

pub fn build(input: &ComposeInput, from: &str) -> AppResult<Message> {
    validate_attachments(input)?;
    if input
        .subject
        .chars()
        .any(|character| character == '\r' || character == '\n')
    {
        return Err(AppError::Message("subject is invalid".into()));
    }
    let from = mailbox(from, "sender")?;
    let in_reply_to = thread_header(input.in_reply_to.as_deref(), "In-Reply-To")?;
    let references = thread_header(input.references.as_deref(), "References")?;

    let mut builder = Message::builder()
        .from(from)
        .message_id(None)
        .subject(input.subject.clone());
    let mut recipients = 0;
    for address in &input.to {
        builder = builder.to(mailbox(address, "recipient")?);
        recipients += 1;
    }
    for address in &input.cc {
        builder = builder.cc(mailbox(address, "recipient")?);
        recipients += 1;
    }
    if recipients == 0 {
        return Err(AppError::Message(
            "at least one recipient is required".into(),
        ));
    }
    if let Some(id) = in_reply_to {
        builder = builder.in_reply_to(id);
    }
    if let Some(ids) = references {
        builder = builder.references(ids);
    }

    let html = input
        .html_body
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let message = match (html, input.attachments.is_empty()) {
        (None, true) => builder.singlepart(SinglePart::plain(input.body.clone())),
        (None, false) => {
            let mut multipart =
                MultiPart::mixed().singlepart(SinglePart::plain(input.body.clone()));
            for attachment in &input.attachments {
                multipart = multipart.singlepart(attachment_part(attachment)?);
            }
            builder.multipart(multipart)
        }
        (Some(html), _) => {
            let plain = SinglePart::plain(input.body.clone());
            let html = SinglePart::html(html.to_string());
            let body = if input.inline_images.is_empty() {
                MultiPart::alternative().singlepart(plain).singlepart(html)
            } else {
                let mut related = MultiPart::related().singlepart(html);
                for image in &input.inline_images {
                    related = related.singlepart(inline_part(image)?);
                }
                MultiPart::alternative()
                    .singlepart(plain)
                    .multipart(related)
            };
            if input.attachments.is_empty() {
                builder.multipart(body)
            } else {
                let mut mixed = MultiPart::mixed().multipart(body);
                for attachment in &input.attachments {
                    mixed = mixed.singlepart(attachment_part(attachment)?);
                }
                builder.multipart(mixed)
            }
        }
    };

    message.map_err(|_| AppError::Message("could not build message".into()))
}

fn attachment_part(attachment: &AttachmentUpload) -> AppResult<SinglePart> {
    let content_type = ContentType::parse(attachment.mime_type.trim())
        .unwrap_or_else(|_| ContentType::parse("application/octet-stream").unwrap());
    Ok(Attachment::new(attachment.name.clone()).body(attachment_bytes(attachment)?, content_type))
}

fn inline_part(image: &InlineImageUpload) -> AppResult<SinglePart> {
    let content_type = ContentType::parse(image.mime_type.trim())
        .unwrap_or_else(|_| ContentType::parse("application/octet-stream").unwrap());
    Ok(Attachment::new_inline(image.content_id.clone()).body(
        BASE64
            .decode(image.data.trim())
            .map_err(|_| AppError::Message("inline image data is invalid".into()))?,
        content_type,
    ))
}

pub fn validate_attachments(input: &ComposeInput) -> AppResult<()> {
    let mut total = 0usize;
    for attachment in &input.attachments {
        let name = attachment.name.trim();
        if name.is_empty() {
            return Err(AppError::Message("attachment name is required".into()));
        }
        if name.chars().any(char::is_control) {
            return Err(AppError::Message(
                "attachment name contains invalid characters".into(),
            ));
        }
        total = total
            .checked_add(attachment_bytes(attachment)?.len())
            .ok_or_else(|| AppError::Message("attachments exceed the 25 MB limit".into()))?;
    }
    for image in &input.inline_images {
        if image.content_id.trim().is_empty() || image.content_id.chars().any(char::is_control) {
            return Err(AppError::Message("inline image id is invalid".into()));
        }
        total = total
            .checked_add(
                BASE64
                    .decode(image.data.trim())
                    .map_err(|_| AppError::Message("inline image data is invalid".into()))?
                    .len(),
            )
            .ok_or_else(|| AppError::Message("attachments exceed the 25 MB limit".into()))?;
    }
    if total > MAX_ATTACHMENT_TOTAL {
        return Err(AppError::Message(
            "attachments exceed the 25 MB limit".into(),
        ));
    }
    Ok(())
}

fn attachment_bytes(attachment: &AttachmentUpload) -> AppResult<Vec<u8>> {
    BASE64
        .decode(attachment.data.trim())
        .map_err(|_| AppError::Message("attachment data is invalid".into()))
}

fn mailbox(value: &str, kind: &str) -> AppResult<Mailbox> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::Message(format!("{kind} address is required")));
    }
    let parsed = trimmed
        .parse::<Mailbox>()
        .map_err(|_| AppError::Message(format!("invalid {kind} address")))?;
    if parsed.email.user().is_empty() || parsed.email.domain().is_empty() {
        return Err(AppError::Message(format!("invalid {kind} address")));
    }
    Ok(parsed)
}

fn thread_header(value: Option<&str>, name: &str) -> AppResult<Option<String>> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) if value.chars().any(char::is_control) => {
            Err(AppError::Message(format!("{name} header is invalid")))
        }
        Some(value) => Ok(Some(value.to_string())),
        None => Ok(None),
    }
}

pub fn save_draft(db: &Database, input: &ComposeInput, draft_id: Option<i64>) -> AppResult<Draft> {
    if input.account_id <= 0 {
        return Err(AppError::Message("account is required".into()));
    }
    validate_attachments(input)?;
    let to_addrs = serde_json::to_string(&input.to)?;
    let cc_addrs = serde_json::to_string(&input.cc)?;
    let attachments = encode_attachments(&input.attachments)?;
    let inline_images = serde_json::to_string(&input.inline_images)?;
    let now = db::now();

    let id = {
        let conn = db.conn.lock().unwrap();
        match draft_id {
            Some(id) => {
                let updated = conn.execute(
                    "update drafts set account_id = ?1, in_reply_to = ?2, references_header = ?3, to_addrs = ?4, cc_addrs = ?5, subject = ?6, body = ?7, body_html = ?8, inline_images = ?9, attachments = ?10, updated_at = ?11 where id = ?12",
                    params![
                        input.account_id,
                        input.in_reply_to,
                        input.references,
                        to_addrs,
                        cc_addrs,
                        input.subject,
                        input.body,
                        input.html_body,
                        inline_images,
                        attachments,
                        now,
                        id,
                    ],
                )?;
                if updated == 0 {
                    return Err(AppError::Message("draft not found".into()));
                }
                id
            }
            None => {
                conn.execute(
                    "insert into drafts (account_id, in_reply_to, references_header, to_addrs, cc_addrs, subject, body, body_html, inline_images, attachments, updated_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        input.account_id,
                        input.in_reply_to,
                        input.references,
                        to_addrs,
                        cc_addrs,
                        input.subject,
                        input.body,
                        input.html_body,
                        inline_images,
                        attachments,
                        now,
                    ],
                )?;
                conn.last_insert_rowid()
            }
        }
    };

    draft_by_id(db, id)
}

#[allow(dead_code)]
pub fn list_drafts(db: &Database, account_id: Option<i64>) -> AppResult<Vec<DraftSummary>> {
    list_drafts_in_scope(db, account_id, None)
}

pub fn list_drafts_for_space(
    db: &Database,
    account_id: Option<i64>,
    space_id: Option<i64>,
) -> AppResult<Vec<DraftSummary>> {
    list_drafts_in_scope(db, account_id, space_id)
}

fn list_drafts_in_scope(
    db: &Database,
    account_id: Option<i64>,
    space_id: Option<i64>,
) -> AppResult<Vec<DraftSummary>> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "select id, account_id, to_addrs, cc_addrs, subject, substr(body, 1, 200), case when substr(attachments, 1, 1) = '[' then case when attachments = '[]' then 0 else 1 end else -1 end, updated_at from drafts d where (?1 is null or d.account_id = ?1) and (?2 is null or d.account_id in (select id from accounts where space_id = ?2)) order by d.updated_at desc, d.id desc",
    )?;
    let rows = stmt
        .query_map(params![account_id, space_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(
            |(id, account_id, to_addrs, cc_addrs, subject, snippet, has, updated_at)| {
                if has < 0 {
                    return Err(corrupt_draft());
                }
                Ok(DraftSummary {
                    id,
                    account_id,
                    to_addrs: serde_json::from_str(&to_addrs).map_err(|_| corrupt_draft())?,
                    cc_addrs: serde_json::from_str(&cc_addrs).map_err(|_| corrupt_draft())?,
                    subject,
                    snippet,
                    has_attachments: has != 0,
                    updated_at,
                })
            },
        )
        .collect()
}

pub fn get_draft(db: &Database, draft_id: i64) -> AppResult<Draft> {
    draft_by_id(db, draft_id)
}

pub fn delete_draft(db: &Database, draft_id: i64) -> AppResult<()> {
    db.conn
        .lock()
        .unwrap()
        .execute("delete from drafts where id = ?1", params![draft_id])?;
    Ok(())
}

fn draft_by_id(db: &Database, draft_id: i64) -> AppResult<Draft> {
    let conn = db.conn.lock().unwrap();
    let row = conn
        .query_row(
            "select id, account_id, to_addrs, cc_addrs, subject, body, body_html, inline_images, in_reply_to, references_header, attachments, updated_at from drafts where id = ?1",
            params![draft_id],
            draft_columns,
        )
        .optional()?
        .ok_or_else(|| AppError::Message("draft not found".into()))?;
    draft_from_columns(row)
}

type DraftColumns = (
    i64,
    i64,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    String,
    i64,
);

fn draft_columns(row: &Row<'_>) -> rusqlite::Result<DraftColumns> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
    ))
}

fn draft_from_columns(row: DraftColumns) -> AppResult<Draft> {
    let (
        id,
        account_id,
        to_addrs,
        cc_addrs,
        subject,
        body,
        body_html,
        inline_images,
        in_reply_to,
        references,
        attachments,
        updated_at,
    ) = row;
    Ok(Draft {
        id,
        account_id,
        to_addrs: serde_json::from_str(&to_addrs).map_err(|_| corrupt_draft())?,
        cc_addrs: serde_json::from_str(&cc_addrs).map_err(|_| corrupt_draft())?,
        subject,
        body,
        body_html,
        inline_images: serde_json::from_str(&inline_images).map_err(|_| corrupt_draft())?,
        in_reply_to,
        references,
        attachments: decode_attachments(&attachments)?,
        updated_at,
    })
}

fn corrupt_draft() -> AppError {
    AppError::Message("draft is corrupt".into())
}

fn encode_attachments(attachments: &[AttachmentUpload]) -> AppResult<String> {
    let stored: Vec<StoredAttachment> = attachments
        .iter()
        .map(|attachment| {
            attachment_bytes(attachment)?;
            Ok(StoredAttachment {
                name: attachment.name.clone(),
                mime_type: attachment.mime_type.clone(),
                data: attachment.data.clone(),
            })
        })
        .collect::<AppResult<_>>()?;
    Ok(serde_json::to_string(&stored)?)
}

fn decode_attachments(value: &str) -> AppResult<Vec<AttachmentUpload>> {
    let stored: Vec<StoredAttachment> = serde_json::from_str(value).map_err(|_| corrupt_draft())?;
    Ok(stored
        .into_iter()
        .map(|attachment| AttachmentUpload {
            name: attachment.name,
            mime_type: attachment.mime_type,
            data: attachment.data,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upload(name: &str, mime_type: &str, bytes: &[u8]) -> AttachmentUpload {
        AttachmentUpload {
            name: name.into(),
            mime_type: mime_type.into(),
            data: BASE64.encode(bytes),
        }
    }

    #[test]
    fn reply_sets_thread_headers() {
        let input = ComposeInput {
            account_id: 1,
            to: vec!["ada@example.com".into()],
            cc: vec![],
            subject: "Re: Hello".into(),
            body: "Thanks".into(),
            html_body: None,
            inline_images: vec![],
            in_reply_to: Some("<1@example.com>".into()),
            references: Some("<1@example.com>".into()),
            attachments: vec![],
        };
        let raw = build_message(&input, "me@example.com").unwrap();
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("In-Reply-To: <1@example.com>"));
        assert!(text.contains("References: <1@example.com>"));
        assert!(text.contains("Subject: Re: Hello"));
    }

    fn valid_input() -> ComposeInput {
        ComposeInput {
            account_id: 1,
            to: vec!["ada@example.com".into()],
            cc: vec![],
            subject: "Hello".into(),
            body: "Body".into(),
            html_body: None,
            inline_images: vec![],
            in_reply_to: None,
            references: None,
            attachments: vec![],
        }
    }

    #[test]
    fn rejects_invalid_recipient() {
        let mut input = valid_input();
        input.to = vec!["not-an-email".into()];
        assert!(build_message(&input, "me@example.com").is_err());
    }

    #[test]
    fn deserializes_the_camel_case_frontend_payload() {
        let json = r#"{"accountId":7,"to":["ada@example.com"],"cc":[],"subject":"Hi","body":"Body","inReplyTo":null,"references":null,"attachments":[{"name":"a.txt","mimeType":"text/plain","data":"aGk="}]}"#;
        let input: ComposeInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.account_id, 7);
        assert_eq!(input.attachments[0].mime_type, "text/plain");
        assert_eq!(attachment_bytes(&input.attachments[0]).unwrap(), b"hi");
    }

    #[test]
    fn builds_plain_text_without_html() {
        let raw = build_message(&valid_input(), "me@example.com").unwrap();
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("Content-Type: text/plain"));
        assert!(!text.to_ascii_lowercase().contains("text/html"));
        assert!(text.contains("Body"));
        assert!(text.contains("Message-ID: <"));
    }

    #[test]
    fn builds_html_alternative_with_plain_text_fallback() {
        let mut input = valid_input();
        input.html_body = Some("<p><b>Body</b></p>".into());
        let raw = build_message(&input, "me@example.com").unwrap();
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("multipart/alternative"));
        assert!(text.contains("Content-Type: text/plain"));
        assert!(text.contains("Content-Type: text/html"));
        assert!(text.contains("<p><b>Body</b></p>"));
    }

    #[test]
    fn inlines_images_in_a_related_part_referenced_by_cid() {
        let mut input = valid_input();
        input.html_body = Some("<p>See <img src=\"cid:image-one\"></p>".into());
        input.inline_images = vec![InlineImageUpload {
            name: "pasted.png".into(),
            mime_type: "image/png".into(),
            data: BASE64.encode(b"png"),
            content_id: "image-one".into(),
        }];
        let raw = build_message(&input, "me@example.com").unwrap();
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("multipart/alternative"));
        assert!(text.contains("multipart/related"));
        assert!(text.contains("Content-ID: <image-one>"));
        assert!(text.contains("Content-Disposition: inline"));
        assert!(text.contains("Content-Type: image/png"));
    }

    #[test]
    fn combines_inline_images_and_regular_attachments() {
        let mut input = valid_input();
        input.html_body = Some("<p>See <img src=\"cid:one\"></p>".into());
        input.inline_images = vec![InlineImageUpload {
            name: "pasted.png".into(),
            mime_type: "image/png".into(),
            data: BASE64.encode(b"png"),
            content_id: "one".into(),
        }];
        input.attachments = vec![upload("report.pdf", "application/pdf", b"hello")];
        let raw = build_message(&input, "me@example.com").unwrap();
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("multipart/mixed"));
        assert!(text.contains("multipart/related"));
        assert!(text.contains("filename=\"report.pdf\""));
    }

    #[test]
    fn adds_attachments_as_mixed_multipart() {
        let mut input = valid_input();
        input.attachments = vec![upload("report.pdf", "application/pdf", b"hello")];
        let raw = build_message(&input, "me@example.com").unwrap();
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("multipart/mixed"));
        assert!(text.contains("Content-Type: application/pdf"));
        assert!(text.contains("filename=\"report.pdf\""));
    }

    #[test]
    fn rejects_attachments_over_the_total_limit() {
        let mut input = valid_input();
        input.attachments = vec![upload(
            "large.bin",
            "application/octet-stream",
            &vec![0; MAX_ATTACHMENT_TOTAL + 1],
        )];
        assert!(validate_attachments(&input).is_err());

        input.attachments = vec![AttachmentUpload {
            name: "bad.bin".into(),
            mime_type: "application/octet-stream".into(),
            data: "not base64!".into(),
        }];
        assert!(validate_attachments(&input).is_err());
    }

    #[test]
    fn requires_at_least_one_recipient() {
        let mut input = valid_input();
        input.to = vec![];
        assert!(build_message(&input, "me@example.com").is_err());
    }

    #[test]
    fn rejects_control_characters_in_thread_headers() {
        let mut input = valid_input();
        input.in_reply_to = Some("<1@example.com>\r\nBcc: evil@example.com".into());
        assert!(build_message(&input, "me@example.com").is_err());
    }

    fn seed_account(db: &Database) -> i64 {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "insert into accounts (display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, sync_interval_secs, created_at) values ('Me', 'me@example.com', 'me@example.com', 'imap.example.com', 993, 'tls', 'smtp.example.com', 465, 'tls', 120, 0)",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn drafts_round_trip_attachments_and_reply_headers() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db);
        let input = ComposeInput {
            account_id: account,
            to: vec!["ada@example.com".into()],
            cc: vec!["team@example.com".into()],
            subject: "Draft".into(),
            body: "Work in progress".into(),
            html_body: Some("<p>Work in progress</p>".into()),
            inline_images: vec![],
            in_reply_to: Some("<1@example.com>".into()),
            references: Some("<1@example.com>".into()),
            attachments: vec![upload("note.txt", "text/plain", b"payload")],
        };

        let draft = save_draft(&db, &input, None).unwrap();
        assert!(draft.id > 0);
        assert_eq!(draft.to_addrs, vec!["ada@example.com"]);
        assert_eq!(draft.attachments[0].data, "cGF5bG9hZA==");

        let listed = list_drafts(&db, Some(account)).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].subject, "Draft");
        assert_eq!(listed[0].snippet, "Work in progress");
        assert!(listed[0].has_attachments);

        let loaded = get_draft(&db, draft.id).unwrap();
        assert_eq!(loaded.attachments[0].name, "note.txt");
        assert_eq!(
            attachment_bytes(&loaded.attachments[0]).unwrap(),
            b"payload"
        );
    }

    #[test]
    fn drafts_round_trip_html_and_inline_images() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db);
        let mut input = valid_input();
        input.account_id = account;
        input.html_body = Some("<p>Hello <b>world</b></p>".into());
        input.inline_images = vec![InlineImageUpload {
            name: "pasted.png".into(),
            mime_type: "image/png".into(),
            data: BASE64.encode(b"png"),
            content_id: "one".into(),
        }];

        let draft = save_draft(&db, &input, None).unwrap();
        assert_eq!(
            draft.body_html.as_deref(),
            Some("<p>Hello <b>world</b></p>")
        );
        assert_eq!(draft.inline_images[0].content_id, "one");

        let loaded = get_draft(&db, draft.id).unwrap();
        assert_eq!(loaded.inline_images[0].data, BASE64.encode(b"png"));
        assert_eq!(loaded.body_html, input.html_body);
    }

    #[test]
    fn draft_listing_does_not_load_attachment_payloads() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db);
        let input = ComposeInput {
            account_id: account,
            to: vec!["ada@example.com".into()],
            cc: vec![],
            subject: "Large".into(),
            body: "Body".into(),
            html_body: None,
            inline_images: vec![],
            in_reply_to: None,
            references: None,
            attachments: vec![upload("big.bin", "application/octet-stream", b"payload")],
        };
        save_draft(&db, &input, None).unwrap();

        let listed = list_drafts(&db, None).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].has_attachments);
        assert!(!listed[0].snippet.contains("cGF5bG9hZA=="));
    }

    #[test]
    fn saving_with_an_id_updates_the_existing_draft() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db);
        let mut input = valid_input();
        input.account_id = account;
        let draft = save_draft(&db, &input, None).unwrap();

        input.subject = "Updated".into();
        let updated = save_draft(&db, &input, Some(draft.id)).unwrap();
        assert_eq!(updated.id, draft.id);
        assert_eq!(updated.subject, "Updated");
        assert_eq!(list_drafts(&db, None).unwrap().len(), 1);
    }

    #[test]
    fn deleting_a_draft_is_idempotent() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db);
        let mut input = valid_input();
        input.account_id = account;
        let draft = save_draft(&db, &input, None).unwrap();

        delete_draft(&db, draft.id).unwrap();
        delete_draft(&db, draft.id).unwrap();
        assert!(list_drafts(&db, None).unwrap().is_empty());
    }

    #[test]
    fn corrupt_draft_json_returns_a_typed_error() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db);
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "insert into drafts (account_id, to_addrs, cc_addrs, attachments, updated_at) values (?1, 'not-json', '[]', '[]', 0)",
                params![account],
            )
            .unwrap();
        }
        let error = list_drafts(&db, None).unwrap_err();
        assert_eq!(error.to_string(), "draft is corrupt");

        {
            let conn = db.conn.lock().unwrap();
            conn.execute("delete from drafts", []).unwrap();
            conn.execute(
                "insert into drafts (account_id, to_addrs, cc_addrs, attachments, updated_at) values (?1, '[]', '[]', 'not-json', 0)",
                params![account],
            )
            .unwrap();
        }
        let error = list_drafts(&db, None).unwrap_err();
        assert_eq!(error.to_string(), "draft is corrupt");
    }
}
