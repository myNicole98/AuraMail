use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use imap::types::NameAttribute;
use native_tls::{TlsConnector, TlsStream};
use serde::Serialize;

use crate::accounts::{Account, MailSecurity};
use crate::error::{AppError, AppResult};
use crate::mail_auth::MailCredential;

const TIMEOUT: Duration = Duration::from_secs(10);

/// Raw messages above this size are refused so opening mail cannot exhaust memory.
const MAX_MESSAGE_BYTES: u64 = 60 * 1024 * 1024;

pub(crate) type ImapSession = imap::Session<TlsStream<TcpStream>>;

struct OAuth2Authenticator<'a> {
    user: &'a str,
    access_token: &'a str,
}

impl imap::Authenticator for OAuth2Authenticator<'_> {
    type Response = String;
    fn process(&self, _data: &[u8]) -> Self::Response {
        format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            self.user, self.access_token
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteFolder {
    pub path: String,
    pub name: String,
    pub role: String,
    pub delimiter: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderState {
    pub uid_validity: u32,
    pub latest_uid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedHeader {
    pub uid: u32,
    pub flags: Vec<String>,
    pub raw: Vec<u8>,
    pub preview: Vec<u8>,
}

pub fn validate_imap(
    host: &str,
    port: u16,
    security: MailSecurity,
    username: &str,
    credential: &MailCredential,
) -> AppResult<()> {
    if host.trim().is_empty() {
        return Err(AppError::Message("IMAP host is required".into()));
    }
    if port == 0 {
        return Err(AppError::Message("IMAP port is required".into()));
    }

    let tcp = connect_tcp(host, port)?;
    let tls = TlsConnector::builder()
        .build()
        .map_err(|_| AppError::Message("IMAP TLS configuration failed".into()))?;

    let client = match security {
        MailSecurity::Tls => {
            let stream = tls
                .connect(host, tcp)
                .map_err(|_| AppError::Message("IMAP TLS handshake failed".into()))?;
            let mut client = imap::Client::new(stream);
            client
                .read_greeting()
                .map_err(|_| AppError::Message("IMAP connection failed".into()))?;
            client
        }
        MailSecurity::StartTls => {
            let mut client = imap::Client::new(tcp);
            client
                .read_greeting()
                .map_err(|_| AppError::Message("IMAP connection failed".into()))?;
            client
                .secure(host, &tls)
                .map_err(|_| AppError::Message("IMAP TLS handshake failed".into()))?
        }
    };

    let mut session = match credential {
        MailCredential::Password(password) => client
            .login(username, password)
            .map_err(|_| AppError::Message("IMAP authentication failed".into())),
        MailCredential::AccessToken(token) => client
            .authenticate(
                "XOAUTH2",
                &OAuth2Authenticator {
                    user: username,
                    access_token: token,
                },
            )
            .map_err(|(_, _)| AppError::Message("IMAP authentication failed".into())),
    }?;
    session
        .select("INBOX")
        .map_err(|_| AppError::Message("IMAP mailbox validation failed".into()))?;
    session
        .list(None, Some("*"))
        .map_err(|_| AppError::Message("IMAP mailbox validation failed".into()))?;
    let _ = session.logout();
    Ok(())
}

pub fn list_folders(session: &mut ImapSession) -> AppResult<Vec<RemoteFolder>> {
    let names = session
        .list(None, Some("*"))
        .map_err(|_| AppError::Message("IMAP folder list failed".into()))?;
    let mut folders = Vec::new();
    for name in names.iter() {
        if name
            .attributes()
            .iter()
            .any(|attribute| matches!(attribute, NameAttribute::NoSelect))
        {
            continue;
        }
        let path = name.name().to_string();
        if path.is_empty() {
            continue;
        }
        let delimiter = name.delimiter().unwrap_or("/").to_string();
        let display = display_name(&path, &delimiter);
        let role =
            special_use_role(name.attributes()).unwrap_or_else(|| folder_role(&path, &display));
        folders.push(RemoteFolder {
            role: role.to_string(),
            path,
            name: display,
            delimiter,
        });
    }
    Ok(folders)
}

pub fn folder_state(session: &mut ImapSession, folder_path: &str) -> AppResult<FolderState> {
    let mailbox = session
        .status(folder_path, "(UIDNEXT UIDVALIDITY)")
        .map_err(|_| AppError::Message("IMAP folder status failed".into()))?;
    if let Some(next_uid) = mailbox.uid_next {
        return Ok(FolderState {
            uid_validity: mailbox.uid_validity.unwrap_or(0),
            latest_uid: next_uid.saturating_sub(1),
        });
    }
    let mailbox = session
        .select(folder_path)
        .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
    let uids = session
        .uid_search("ALL")
        .map_err(|_| AppError::Message("IMAP UID search failed".into()))?;
    Ok(FolderState {
        uid_validity: mailbox.uid_validity.unwrap_or(0),
        latest_uid: uids.into_iter().max().unwrap_or(0),
    })
}

pub fn folder_flags(
    session: &mut ImapSession,
    folder_path: &str,
) -> AppResult<Vec<(u32, Vec<String>)>> {
    session
        .select(folder_path)
        .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
    let fetches = session
        .uid_fetch("1:*", "(UID FLAGS)")
        .map_err(|_| AppError::Message("IMAP flag fetch failed".into()))?;
    Ok(fetches
        .iter()
        .filter_map(|fetch| {
            Some((
                fetch.uid?,
                fetch.flags().iter().map(ToString::to_string).collect(),
            ))
        })
        .collect())
}

pub fn fetch_headers(
    session: &mut ImapSession,
    folder_path: &str,
    from_uid: u32,
) -> AppResult<Vec<FetchedHeader>> {
    session
        .select(folder_path)
        .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
    let found = session
        .uid_search(format!("UID {from_uid}:*"))
        .map_err(|_| AppError::Message("IMAP header search failed".into()))?;
    let mut uids: Vec<u32> = found.into_iter().collect();
    uids.sort_unstable();
    let mut headers = Vec::new();
    for chunk in uids.chunks(500) {
        let set = chunk
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let fetches = session
            .uid_fetch(set, "(UID FLAGS BODY.PEEK[HEADER] BODY.PEEK[TEXT]<0.4096>)")
            .map_err(|_| AppError::Message("IMAP header fetch failed".into()))?;
        for fetch in fetches.iter() {
            let (Some(uid), Some(raw)) = (fetch.uid, fetch.header()) else {
                continue;
            };
            headers.push(FetchedHeader {
                uid,
                flags: fetch.flags().iter().map(ToString::to_string).collect(),
                raw: raw.to_vec(),
                preview: fetch.text().map(Vec::from).unwrap_or_default(),
            });
        }
    }
    Ok(headers)
}

pub fn fetch_message(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uid: u32,
) -> AppResult<Vec<u8>> {
    let mut session = open_session(account, credential)?;
    session
        .select(folder_path)
        .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
    let sizes = session
        .uid_fetch(uid.to_string(), "(RFC822.SIZE)")
        .map_err(|_| AppError::Message("IMAP message fetch failed".into()))?;
    let size = sizes.iter().find_map(|fetch| fetch.size).unwrap_or(0) as u64;
    if exceeds_message_limit(size) {
        let _ = session.logout();
        return Err(AppError::Message("message is too large to open".into()));
    }
    let fetches = session
        .uid_fetch(uid.to_string(), "(BODY.PEEK[])")
        .map_err(|_| AppError::Message("IMAP message fetch failed".into()))?;
    let raw = fetches
        .iter()
        .find_map(|fetch| fetch.body())
        .map(Vec::from)
        .ok_or_else(|| AppError::Message("IMAP message body is unavailable".into()))?;
    if exceeds_message_limit(raw.len() as u64) {
        let _ = session.logout();
        return Err(AppError::Message("message is too large to open".into()));
    }
    let _ = session.logout();
    Ok(raw)
}

fn exceeds_message_limit(size: u64) -> bool {
    size > MAX_MESSAGE_BYTES
}

pub fn set_seen(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uid: u32,
    seen: bool,
) -> AppResult<()> {
    store_flag(account, credential, folder_path, uid, "\\Seen", seen)
}

pub fn set_starred(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uid: u32,
    starred: bool,
) -> AppResult<()> {
    store_flag(account, credential, folder_path, uid, "\\Flagged", starred)
}

pub fn move_to_folder(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uid: u32,
    target_path: &str,
) -> AppResult<()> {
    let mut session = open_session(account, credential)?;
    session
        .select(folder_path)
        .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
    move_selected(&mut session, uid, target_path)?;
    let _ = session.logout();
    Ok(())
}

pub fn archive(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uid: u32,
    archive_path: Option<&str>,
    all_path: Option<&str>,
) -> AppResult<()> {
    let mut session = open_session(account, credential)?;
    session
        .select(folder_path)
        .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
    let gmail = session
        .capabilities()
        .map(|capabilities| capabilities.has_str("X-GM-EXT-1"))
        .unwrap_or(false);
    let target = archive_destination(archive_path, all_path, gmail)?;
    move_selected(&mut session, uid, &target)?;
    let _ = session.logout();
    Ok(())
}

pub fn archive_destination(
    archive_path: Option<&str>,
    all_path: Option<&str>,
    gmail: bool,
) -> AppResult<String> {
    if let Some(archive) = archive_path {
        return Ok(archive.to_string());
    }
    if gmail {
        if let Some(all) = all_path {
            return Ok(all.to_string());
        }
    }
    Err(AppError::Message(
        "archive folder is unavailable for this account".into(),
    ))
}

fn move_selected(session: &mut ImapSession, uid: u32, target_path: &str) -> AppResult<()> {
    require_uidplus(uidplus_supported(session))?;
    session
        .uid_copy(uid.to_string(), quoted(target_path))
        .map_err(|_| AppError::Message("IMAP message move failed".into()))?;
    session
        .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
        .map_err(|_| AppError::Message("IMAP message delete failed".into()))?;
    expunge_uid(session, uid)
}

pub fn move_to_trash(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uid: u32,
    trash_path: &str,
) -> AppResult<()> {
    move_to_folder(account, credential, folder_path, uid, trash_path)
}

pub fn delete_permanently(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uid: u32,
) -> AppResult<()> {
    let mut session = open_session(account, credential)?;
    session
        .select(folder_path)
        .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
    require_uidplus(uidplus_supported(&mut session))?;
    session
        .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
        .map_err(|_| AppError::Message("IMAP message delete failed".into()))?;
    expunge_uid(&mut session, uid)?;
    let _ = session.logout();
    Ok(())
}

fn uidplus_supported(session: &mut ImapSession) -> bool {
    session
        .capabilities()
        .map(|capabilities| capabilities.has_str("UIDPLUS"))
        .unwrap_or(false)
}

fn require_uidplus(uidplus: bool) -> AppResult<()> {
    if uidplus {
        Ok(())
    } else {
        Err(AppError::Message(
            "IMAP server does not support safe UID expunge".into(),
        ))
    }
}

fn expunge_uid(session: &mut ImapSession, uid: u32) -> AppResult<()> {
    require_uidplus(uidplus_supported(session))?;
    session
        .uid_expunge(uid.to_string())
        .map_err(|_| AppError::Message("IMAP expunge failed".into()))?;
    Ok(())
}

fn store_flag(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uid: u32,
    flag: &str,
    enabled: bool,
) -> AppResult<()> {
    let mut session = open_session(account, credential)?;
    session
        .select(folder_path)
        .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
    let operation = if enabled { "+FLAGS" } else { "-FLAGS" };
    session
        .uid_store(uid.to_string(), format!("{operation} ({flag})"))
        .map_err(|_| AppError::Message("IMAP flag update failed".into()))?;
    let _ = session.logout();
    Ok(())
}

pub(crate) fn open_session(
    account: &Account,
    credential: &MailCredential,
) -> AppResult<ImapSession> {
    if account.imap_host.trim().is_empty() {
        return Err(AppError::Message("IMAP host is required".into()));
    }
    if account.imap_port == 0 {
        return Err(AppError::Message("IMAP port is required".into()));
    }
    let security = MailSecurity::parse(&account.imap_security)?;
    let tcp = connect_tcp(&account.imap_host, account.imap_port)?;
    let tls = TlsConnector::builder()
        .build()
        .map_err(|_| AppError::Message("IMAP TLS configuration failed".into()))?;

    let client = match security {
        MailSecurity::Tls => {
            let stream = tls
                .connect(&account.imap_host, tcp)
                .map_err(|_| AppError::Message("IMAP TLS handshake failed".into()))?;
            let mut client = imap::Client::new(stream);
            client
                .read_greeting()
                .map_err(|_| AppError::Message("IMAP connection failed".into()))?;
            client
        }
        MailSecurity::StartTls => {
            let mut client = imap::Client::new(tcp);
            client
                .read_greeting()
                .map_err(|_| AppError::Message("IMAP connection failed".into()))?;
            client
                .secure(&account.imap_host, &tls)
                .map_err(|_| AppError::Message("IMAP TLS handshake failed".into()))?
        }
    };

    match credential {
        MailCredential::Password(password) => client
            .login(&account.username, password)
            .map_err(|_| AppError::Message("IMAP authentication failed".into())),
        MailCredential::AccessToken(token) => client
            .authenticate(
                "XOAUTH2",
                &OAuth2Authenticator {
                    user: &account.username,
                    access_token: token,
                },
            )
            .map_err(|(_, _)| AppError::Message("IMAP authentication failed".into())),
    }
}

fn connect_tcp(host: &str, port: u16) -> AppResult<TcpStream> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_| AppError::Message("IMAP connection failed".into()))?;
    let mut timed_out = false;
    for address in addresses {
        match TcpStream::connect_timeout(&address, TIMEOUT) {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(TIMEOUT));
                let _ = stream.set_write_timeout(Some(TIMEOUT));
                return Ok(stream);
            }
            Err(err) if err.kind() == io::ErrorKind::TimedOut => timed_out = true,
            Err(_) => {}
        }
    }
    if timed_out {
        Err(AppError::Message("IMAP connection timed out".into()))
    } else {
        Err(AppError::Message("IMAP connection failed".into()))
    }
}

fn display_name(path: &str, delimiter: &str) -> String {
    if delimiter.is_empty() {
        return path.to_string();
    }
    path.rsplit(delimiter).next().unwrap_or(path).to_string()
}

fn special_use_role(attributes: &[NameAttribute<'_>]) -> Option<&'static str> {
    for attribute in attributes {
        let NameAttribute::Custom(value) = attribute else {
            continue;
        };
        match value.trim_start_matches('\\').to_ascii_lowercase().as_str() {
            "archive" => return Some("archive"),
            "all" => return Some("all"),
            "trash" => return Some("trash"),
            "sent" => return Some("sent"),
            "drafts" => return Some("drafts"),
            "junk" => return Some("junk"),
            _ => {}
        }
    }
    None
}

fn folder_role(path: &str, name: &str) -> &'static str {
    let path = path.to_ascii_lowercase();
    let name = name.to_ascii_lowercase();
    if path == "inbox" {
        "inbox"
    } else if matches!(
        name.as_str(),
        "sent" | "sent items" | "sent mail" | "sent messages" | "gesendet" | "enviados"
    ) {
        "sent"
    } else if matches!(
        name.as_str(),
        "drafts" | "draft" | "entwürfe" | "borradores"
    ) {
        "drafts"
    } else if matches!(
        name.as_str(),
        "junk" | "junk email" | "junk e-mail" | "spam" | "bulk mail"
    ) {
        "junk"
    } else if matches!(
        name.as_str(),
        "trash" | "deleted" | "deleted items" | "deleted messages" | "papierkorb" | "papelera"
    ) {
        "trash"
    } else if matches!(name.as_str(), "archive" | "archives" | "archivio") {
        "archive"
    } else if matches!(name.as_str(), "all mail" | "all") {
        "all"
    } else {
        "other"
    }
}

fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::MailSecurity;
    use imap::Authenticator;

    #[test]
    fn rejects_blank_host_without_network_access() {
        assert!(validate_imap(
            "",
            993,
            MailSecurity::Tls,
            "user",
            &MailCredential::Password("pass".into())
        )
        .is_err());
    }

    #[test]
    fn rejects_invalid_port_without_network_access() {
        assert!(validate_imap(
            "imap.example.com",
            0,
            MailSecurity::StartTls,
            "user",
            &MailCredential::Password("pass".into())
        )
        .is_err());
    }

    #[test]
    fn maps_folder_roles_and_display_names() {
        assert_eq!(folder_role("INBOX", "INBOX"), "inbox");
        assert_eq!(folder_role("INBOX/Sent", "Sent"), "sent");
        assert_eq!(folder_role("[Gmail]/Drafts", "Drafts"), "drafts");
        assert_eq!(folder_role("[Gmail]/Trash", "Trash"), "trash");
        assert_eq!(folder_role("[Gmail]/Spam", "Spam"), "junk");
        assert_eq!(folder_role("Junk Email", "Junk Email"), "junk");
        assert_eq!(folder_role("Archive", "Archive"), "archive");
        assert_eq!(folder_role("[Gmail]/All Mail", "All Mail"), "all");
        assert_eq!(folder_role("Projects", "Projects"), "other");
        assert_eq!(display_name("INBOX/Projects", "/"), "Projects");
        assert_eq!(display_name("INBOX", "/"), "INBOX");
        let attributes = vec![NameAttribute::Custom("\\Archive".into())];
        assert_eq!(special_use_role(&attributes), Some("archive"));
    }

    #[test]
    fn quotes_mailbox_names_with_spaces_and_quotes() {
        assert_eq!(quoted("Sent Items"), "\"Sent Items\"");
        assert_eq!(quoted("a\"b"), "\"a\\\"b\"");
    }

    #[test]
    fn archive_prefers_a_real_archive_folder() {
        assert_eq!(
            archive_destination(Some("Archive"), Some("[Gmail]/All Mail"), false).unwrap(),
            "Archive"
        );
        assert_eq!(
            archive_destination(Some("Archive"), Some("[Gmail]/All Mail"), true).unwrap(),
            "Archive"
        );
    }

    #[test]
    fn archive_uses_all_mail_only_for_gmail() {
        assert_eq!(
            archive_destination(None, Some("[Gmail]/All Mail"), true).unwrap(),
            "[Gmail]/All Mail"
        );
        assert!(archive_destination(None, Some("[Gmail]/All Mail"), false).is_err());
        assert!(archive_destination(None, None, true).is_err());
    }

    #[test]
    fn requires_uidplus_for_safe_expunge() {
        assert!(require_uidplus(true).is_ok());
        assert!(require_uidplus(false).is_err());
    }

    #[test]
    fn rejects_raw_messages_over_the_size_cap() {
        assert!(!exceeds_message_limit(MAX_MESSAGE_BYTES));
        assert!(exceeds_message_limit(MAX_MESSAGE_BYTES + 1));
    }

    #[test]
    fn xoauth2_authenticator_uses_bearer_token() {
        let auth = OAuth2Authenticator {
            user: "me@gmail.com",
            access_token: "token-123",
        };
        assert_eq!(
            auth.process(b""),
            "user=me@gmail.com\x01auth=Bearer token-123\x01\x01"
        );
    }
}
