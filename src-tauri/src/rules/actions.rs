//! Executing rule actions against the open IMAP session and the local database.
//!
//! Server operations happen first; local rows change only after the server
//! accepted. A failed move restores the row exactly where it was.

use std::cell::RefCell;
use std::collections::HashSet;

use base64::Engine as _;
use lettre::address::Envelope;
use tauri::AppHandle;

use crate::accounts::Account;
use crate::compose::{AttachmentUpload, ComposeInput};
use crate::db::Database;
use crate::imap::client::{self, ImapSession};
use crate::labels;
use crate::mail::{self, parse, Folder, Message};
use crate::mail_auth::MailCredential;
use crate::rules::Action;
use crate::smtp;

#[derive(Debug, Default, Clone)]
pub struct Outcome {
    pub moved: bool,
    pub deleted: bool,
    pub errors: Vec<String>,
}

impl Outcome {
    fn fail(message: impl Into<String>) -> Self {
        Self {
            errors: vec![message.into()],
            ..Self::default()
        }
    }
}

pub struct Engine<'a> {
    pub app: &'a AppHandle,
    pub db: &'a Database,
    pub account: &'a Account,
    pub credential: &'a MailCredential,
    pub folders: &'a [Folder],
    pub my_addresses: &'a [String],
    /// Rules may notify only when they were written to; a sweep stays silent.
    pub allow_notifications: bool,
    /// One forward per message per run, so two rules cannot both forward it.
    forwarded: RefCell<HashSet<i64>>,
}

pub struct Target<'a> {
    pub session: &'a mut ImapSession,
    pub folder: Folder,
    pub uid: u32,
    pub message: Message,
}

impl<'a> Engine<'a> {
    pub fn new(
        app: &'a AppHandle,
        db: &'a Database,
        account: &'a Account,
        credential: &'a MailCredential,
        folders: &'a [Folder],
        my_addresses: &'a [String],
        allow_notifications: bool,
    ) -> Self {
        Self {
            app,
            db,
            account,
            credential,
            folders,
            my_addresses,
            allow_notifications,
            forwarded: RefCell::new(HashSet::new()),
        }
    }

    pub fn apply(&self, target: &mut Target<'_>, action: &Action) -> Outcome {
        match action {
            Action::MoveTo { folder_id } => self.move_to(target, *folder_id),
            Action::CopyTo { folder_id } => self.copy_to(target, *folder_id),
            Action::Delete => self.delete(target, false),
            Action::PermanentDelete => self.delete(target, true),
            Action::MarkRead => self.set_flag(target, "\\Seen", true, true),
            Action::SetFlag => self.set_flag(target, "\\Flagged", true, false),
            Action::ClearFlag => self.set_flag(target, "\\Flagged", false, false),
            Action::MarkJunk => match self.folder_with_role("junk") {
                Some(junk) => {
                    let id = junk.id;
                    self.move_to(target, id)
                }
                None => Outcome::fail("no Junk folder is configured for this account"),
            },
            Action::SetImportance { value } => {
                self.set_column(target, "importance", value.as_str())
            }
            Action::CompleteFlag => self.set_column(target, "followup", "complete"),
            Action::AddLabel { label_id } => {
                let key = message_key(&target.message);
                match labels::add_for_message(self.db, target.message.account_id, &key, *label_id) {
                    Ok(()) => Outcome::default(),
                    Err(error) => Outcome::fail(error.to_string()),
                }
            }
            Action::ClearLabels => {
                let key = message_key(&target.message);
                match labels::clear_for_message(self.db, target.message.account_id, &key) {
                    Ok(()) => Outcome::default(),
                    Err(error) => Outcome::fail(error.to_string()),
                }
            }
            Action::ForwardTo { address } => self.forward(target, address, false),
            Action::ForwardAsAttachment { address } => self.forward(target, address, true),
            Action::RedirectTo { address } => self.redirect(target, address),
            Action::NotifyDesktop => {
                if self.allow_notifications {
                    crate::desktop::notify_message_now(
                        self.app,
                        self.db,
                        self.account,
                        &[target.message.id],
                    );
                }
                Outcome::default()
            }
            Action::PlaySound => {
                if self.allow_notifications {
                    crate::desktop::play_new_mail_sound();
                }
                Outcome::default()
            }
        }
    }

    fn folder(&self, id: i64) -> Option<&Folder> {
        self.folders
            .iter()
            .find(|folder| folder.id == id && folder.account_id == self.account.id)
    }

    fn folder_with_role(&self, role: &str) -> Option<&Folder> {
        self.folders
            .iter()
            .find(|folder| folder.role == role && folder.account_id == self.account.id)
    }

    fn move_to(&self, target: &mut Target<'_>, folder_id: i64) -> Outcome {
        let Some(destination) = self.folder(folder_id) else {
            return Outcome::fail("the rule's target folder no longer exists");
        };
        if destination.id == target.folder.id {
            return Outcome::default();
        }
        let destination_path = destination.path.clone();
        let destination_id = destination.id;
        let original_folder = target.folder.id;
        let original_uid = target.uid;
        if let Err(error) = mail::move_message_pending(self.db, target.message.id, destination_id) {
            return Outcome::fail(format!("could not move the message: {error}"));
        }
        match client::move_selected_in_session(
            target.session,
            &[target.uid],
            &destination_path,
            self.account.auth_type,
        ) {
            Ok(()) => {
                target.message.folder_id = destination_id;
                target.folder.id = destination_id;
                target.folder.path = destination_path;
                Outcome {
                    moved: true,
                    ..Outcome::default()
                }
            }
            Err(error) => {
                let _ = mail::restore_message(
                    self.db,
                    target.message.id,
                    original_folder,
                    original_uid,
                );
                Outcome::fail(format!("could not move the message: {error}"))
            }
        }
    }

    fn copy_to(&self, target: &mut Target<'_>, folder_id: i64) -> Outcome {
        let Some(destination) = self.folder(folder_id) else {
            return Outcome::fail("the rule's target folder no longer exists");
        };
        match client::copy_selected_in_session(target.session, &[target.uid], &destination.path) {
            Ok(()) => Outcome::default(),
            Err(error) => Outcome::fail(format!("could not copy the message: {error}")),
        }
    }

    fn delete(&self, target: &mut Target<'_>, permanent: bool) -> Outcome {
        if !permanent {
            if let Some(trash) = self.folder_with_role("trash") {
                let id = trash.id;
                let outcome = self.move_to(target, id);
                if outcome.errors.is_empty() {
                    return Outcome {
                        moved: outcome.moved,
                        deleted: true,
                        ..Outcome::default()
                    };
                }
                return outcome;
            }
        }
        match client::delete_selected_permanently(
            target.session,
            &[target.uid],
            self.account.auth_type,
        ) {
            Ok(()) => {
                let _ = self
                    .db
                    .conn
                    .lock()
                    .unwrap()
                    .execute("delete from messages where id = ?1", [target.message.id]);
                Outcome {
                    deleted: true,
                    ..Outcome::default()
                }
            }
            Err(error) => Outcome::fail(format!("could not delete the message: {error}")),
        }
    }

    fn set_flag(
        &self,
        target: &mut Target<'_>,
        flag: &str,
        enabled: bool,
        is_seen: bool,
    ) -> Outcome {
        match client::set_flag_on(target.session, &[target.uid], flag, enabled) {
            Ok(()) => {
                if is_seen {
                    target.message.is_seen = enabled;
                    let _ = self.db.conn.lock().unwrap().execute(
                        "update messages set is_seen = ?2 where id = ?1",
                        rusqlite::params![target.message.id, enabled as i64],
                    );
                } else {
                    target.message.is_starred = enabled;
                    let _ = self.db.conn.lock().unwrap().execute(
                        "update messages set is_starred = ?2 where id = ?1",
                        rusqlite::params![target.message.id, enabled as i64],
                    );
                }
                Outcome::default()
            }
            Err(error) => Outcome::fail(format!("could not update the message: {error}")),
        }
    }

    fn set_column(&self, target: &mut Target<'_>, column: &str, value: &str) -> Outcome {
        let result = self.db.conn.lock().unwrap().execute(
            &format!("update messages set {column} = ?2 where id = ?1"),
            rusqlite::params![target.message.id, value],
        );
        match result {
            Ok(_) => Outcome::default(),
            Err(error) => Outcome::fail(format!("could not update the message: {error}")),
        }
    }

    /// Forward refuses to fire on mail the user sent, which is the only way a
    /// forwarding loop can start.
    fn forward(&self, target: &mut Target<'_>, address: &str, as_attachment: bool) -> Outcome {
        if self.is_mine(&target.message.from_addr) {
            return Outcome::fail("skipped forwarding mail sent by you");
        }
        if !self.forwarded.borrow_mut().insert(target.message.id) {
            return Outcome::fail("the message was already forwarded by a rule");
        }
        let raw = match client::fetch_message_in_session(target.session, target.uid) {
            Ok(raw) => raw,
            Err(error) => return Outcome::fail(format!("could not read the message: {error}")),
        };
        let parsed = parse::parse_message(&raw);
        let subject = format!("Fwd: {}", parsed.subject.trim());
        let attachments = if as_attachment {
            vec![AttachmentUpload {
                name: "forwarded-message.eml".into(),
                mime_type: "message/rfc822".into(),
                data: base64::engine::general_purpose::STANDARD.encode(&raw),
            }]
        } else {
            Vec::new()
        };
        let body = forwarded_body(&parsed);
        let input = ComposeInput {
            account_id: self.account.id,
            to: vec![address.trim().to_string()],
            cc: Vec::new(),
            subject,
            body,
            html_body: None,
            inline_images: Vec::new(),
            in_reply_to: None,
            references: None,
            attachments,
        };
        match smtp::send(self.account, self.credential, &input) {
            Ok(sent) => {
                self.append_to_sent(&sent);
                Outcome::default()
            }
            Err(error) => Outcome::fail(format!("could not forward the message: {error}")),
        }
    }

    /// Redirect resends the original untouched, with `Resent-*` headers, which
    /// is what Outlook's "redirect" does.
    fn redirect(&self, target: &mut Target<'_>, address: &str) -> Outcome {
        if self.is_mine(&target.message.from_addr) {
            return Outcome::fail("skipped redirecting mail sent by you");
        }
        if !self.forwarded.borrow_mut().insert(target.message.id) {
            return Outcome::fail("the message was already forwarded by a rule");
        }
        let raw = match client::fetch_message_in_session(target.session, target.uid) {
            Ok(raw) => raw,
            Err(error) => return Outcome::fail(format!("could not read the message: {error}")),
        };
        let Ok(to) = address.trim().parse::<lettre::Address>() else {
            return Outcome::fail("the rule's redirect address is invalid");
        };
        let Ok(from) = self.account.email.parse::<lettre::Address>() else {
            return Outcome::fail("the account address is invalid");
        };
        let Ok(envelope) = Envelope::new(Some(from), vec![to.clone()]) else {
            return Outcome::fail("could not build the redirect envelope");
        };
        let date = chrono::Utc::now().to_rfc2822();
        let mut out = format!(
            "Resent-From: {}\r\nResent-To: {}\r\nResent-Date: {}\r\n",
            self.account.email, to, date
        )
        .into_bytes();
        out.extend_from_slice(&raw);
        match smtp::send_raw(self.account, self.credential, &envelope, &out) {
            Ok(()) => {
                self.append_to_sent(&out);
                Outcome::default()
            }
            Err(error) => Outcome::fail(format!("could not redirect the message: {error}")),
        }
    }

    fn append_to_sent(&self, raw: &[u8]) {
        let Some(sent) = self.folder_with_role("sent") else {
            return;
        };
        let _ = client::append_message(self.account, self.credential, &sent.path, &["\\Seen"], raw);
    }

    fn is_mine(&self, address: &str) -> bool {
        let address = address.trim().to_ascii_lowercase();
        self.my_addresses
            .iter()
            .any(|mine| mine.trim().to_ascii_lowercase() == address)
    }
}

pub fn message_key(message: &Message) -> String {
    labels::message_key(message.message_id.as_deref(), message.id)
}

fn forwarded_body(parsed: &parse::ParsedMessage) -> String {
    let mut body = String::from("---------- Forwarded message ----------\n");
    if !parsed.from_name.trim().is_empty() {
        body.push_str(&format!(
            "From: {} <{}>\n",
            parsed.from_name, parsed.from_addr
        ));
    } else if !parsed.from_addr.trim().is_empty() {
        body.push_str(&format!("From: {}\n", parsed.from_addr));
    }
    body.push_str(&format!("Subject: {}\n", parsed.subject));
    body.push_str(&format!("To: {}\n\n", parsed.to_addrs.join(", ")));
    if !parsed.text_body.trim().is_empty() {
        body.push_str(&parsed.text_body);
    } else if let Some(html) = parsed.html_body.as_deref() {
        body.push_str(&parse::html_body_to_text(html));
    }
    body
}

/// Importance is only ever set locally by a rule; the header value is parsed at
/// sync time and stored in the same column.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarded_body_carries_the_original_text() {
        let raw = b"From: Alice <alice@example.org>\r\nTo: me@example.com\r\nSubject: Hello\r\n\r\nOriginal text";
        let parsed = parse::parse_message(raw);
        let body = forwarded_body(&parsed);
        assert!(body.contains("From: Alice <alice@example.org>"));
        assert!(body.contains("Subject: Hello"));
        assert!(body.contains("Original text"));
    }

    #[test]
    fn message_key_matches_the_label_identity() {
        let message = Message {
            id: 12,
            account_id: 1,
            folder_id: 2,
            uid: 3,
            message_id: Some(" id@example.com ".into()),
            references: None,
            thread_key: String::new(),
            from_addr: String::new(),
            from_name: String::new(),
            to_addrs: Vec::new(),
            cc_addrs: Vec::new(),
            subject: String::new(),
            date: 0,
            snippet: String::new(),
            is_seen: false,
            is_starred: false,
            has_attachments: false,
            body_cached: false,
            tab_id: None,
            label_ids: Vec::new(),
        };
        assert_eq!(message_key(&message), "id@example.com");
    }
}
