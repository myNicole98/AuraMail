use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use imap::types::NameAttribute;
use native_tls::{TlsConnector, TlsStream};
use serde::Serialize;

use crate::accounts::{Account, AccountAuth, MailSecurity};
use crate::error::{AppError, AppResult};
use crate::mail_auth::MailCredential;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const IO_TIMEOUT: Duration = Duration::from_secs(60);

/// Raw messages above this size are refused so opening mail cannot exhaust memory.
const MAX_MESSAGE_BYTES: u64 = 60 * 1024 * 1024;

const MAX_IMAP_LINE: usize = 64 * 1024;
const STARTTLS_TAG: &[u8] = b"auramail1";

#[derive(Debug)]
pub enum ImapStream {
    Tls(TlsStream<TcpStream>),
    Plain(TcpStream),
}

impl Read for ImapStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Tls(stream) => stream.read(buffer),
            Self::Plain(stream) => stream.read(buffer),
        }
    }
}

impl Write for ImapStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Tls(stream) => stream.write(buffer),
            Self::Plain(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Tls(stream) => stream.flush(),
            Self::Plain(stream) => stream.flush(),
        }
    }
}

pub type ImapSession = imap::Session<ImapStream>;

/// Proton Bridge only listens on the local machine; plaintext never leaves it.
pub fn proton_host_allowed(host: &str) -> bool {
    let host = host.trim();
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    host == "127.0.0.1" || host == "::1" || host.eq_ignore_ascii_case("localhost")
}

pub fn proton_move_command(uid: u32, target: &str) -> String {
    format!("UID MOVE {uid} {}", quoted(target))
}

pub fn proton_delete_command(uid: u32) -> [String; 2] {
    [
        format!("UID STORE {uid} +FLAGS (\\Deleted)"),
        "EXPUNGE".to_string(),
    ]
}

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
    validate_with(
        host,
        port,
        AccountAuth::Password,
        Some(security),
        username,
        credential,
    )
}

/// Validates a Proton Bridge endpoint over plaintext loopback.
#[allow(dead_code)]
pub fn validate_proton_imap(
    host: &str,
    port: u16,
    username: &str,
    credential: &MailCredential,
) -> AppResult<()> {
    validate_with(host, port, AccountAuth::Proton, None, username, credential)
}

fn validate_with(
    host: &str,
    port: u16,
    auth_type: AccountAuth,
    security: Option<MailSecurity>,
    username: &str,
    credential: &MailCredential,
) -> AppResult<()> {
    if host.trim().is_empty() {
        return Err(AppError::Message("IMAP host is required".into()));
    }
    if port == 0 {
        return Err(AppError::Message("IMAP port is required".into()));
    }

    let client = connect_client(host, port, auth_type, security)?;
    let mut session = authenticate(client, username, credential)?;
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
        .map_err(|error| AppError::Message(format!("IMAP folder list failed: {error}")))?;
    let mut folders = names.iter().filter_map(remote_folder).collect::<Vec<_>>();
    // Some providers (Yandex) do not reliably include the special INBOX name in
    // a wildcard listing. INBOX always exists, so ask for it explicitly.
    if !folders
        .iter()
        .any(|folder| folder.path.eq_ignore_ascii_case("INBOX"))
    {
        if let Ok(inbox) = session.list(None, Some("INBOX")) {
            folders.extend(inbox.iter().filter_map(remote_folder));
        }
    }
    Ok(folders)
}

fn remote_folder(name: &imap::types::Name) -> Option<RemoteFolder> {
    if name
        .attributes()
        .iter()
        .any(|attribute| matches!(attribute, NameAttribute::NoSelect))
    {
        return None;
    }
    let path = name.name().to_string();
    if path.is_empty() {
        return None;
    }
    let delimiter = name.delimiter().unwrap_or("/").to_string();
    let display = display_name(&path, &delimiter);
    let role = special_use_role(name.attributes()).unwrap_or_else(|| folder_role(&path, &display));
    Some(RemoteFolder {
        role: role.to_string(),
        path,
        name: display,
        delimiter,
    })
}

pub fn folder_state(session: &mut ImapSession, folder_path: &str) -> AppResult<FolderState> {
    let mailbox = session
        .status(folder_path, "(UIDNEXT UIDVALIDITY)")
        .map_err(|error| AppError::Message(format!("IMAP folder status failed: {error}")))?;
    if let Some(next_uid) = mailbox.uid_next {
        return Ok(FolderState {
            uid_validity: mailbox.uid_validity.unwrap_or(0),
            latest_uid: next_uid.saturating_sub(1),
        });
    }
    let mailbox = session
        .select(folder_path)
        .map_err(|error| AppError::Message(format!("IMAP folder select failed: {error}")))?;
    let uids = session
        .uid_search("ALL")
        .map_err(|error| AppError::Message(format!("IMAP UID search failed: {error}")))?;
    Ok(FolderState {
        uid_validity: mailbox.uid_validity.unwrap_or(0),
        latest_uid: uids.into_iter().max().unwrap_or(0),
    })
}

pub fn folder_flags(
    session: &mut ImapSession,
    folder_path: &str,
    uids: &[u32],
) -> AppResult<Vec<(u32, Vec<String>)>> {
    if uids.is_empty() {
        return Ok(Vec::new());
    }
    session
        .select(folder_path)
        .map_err(|error| AppError::Message(format!("IMAP folder select failed: {error}")))?;
    let mut result = Vec::new();
    for chunk in uids.chunks(200) {
        let set = chunk
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let fetches = session
            .uid_fetch(set, "(UID FLAGS)")
            .map_err(|error| AppError::Message(format!("IMAP flag fetch failed: {error}")))?;
        result.extend(fetches.iter().filter_map(|fetch| {
            Some((
                fetch.uid?,
                fetch.flags().iter().map(ToString::to_string).collect(),
            ))
        }));
    }
    Ok(result)
}

/// Fetch headers in batches so the caller can persist and publish the first
/// results before a large mailbox has finished downloading.
///
/// The initial pass pages through the mailbox with sequence-number FETCH
/// commands instead of `UID SEARCH`: Yandex intermittently answers SEARCH with
/// `NO [BACKEND]`, which used to fail the whole folder (typically the large
/// INBOX) before a single message was fetched. Incremental syncs use a direct
/// UID range, which needs no search either.
pub fn fetch_headers_batched<F>(
    session: &mut ImapSession,
    folder_path: &str,
    from_uid: u32,
    latest_uid: u32,
    mut on_batch: F,
) -> AppResult<()>
where
    F: FnMut(Vec<FetchedHeader>) -> AppResult<()>,
{
    let mailbox = session
        .select(folder_path)
        .map_err(|error| AppError::Message(format!("IMAP folder select failed: {error}")))?;
    if from_uid <= 1 {
        let mut start = 1u32;
        while start <= mailbox.exists {
            let end = start.saturating_add(HEADER_FETCH_PAGE - 1).min(mailbox.exists);
            let fetches = session
                .fetch(
                    format!("{start}:{end}"),
                    "(UID FLAGS BODY.PEEK[HEADER] BODY.PEEK[TEXT]<0.4096>)",
                )
                .map_err(|error| {
                    AppError::Message(format!("IMAP header fetch failed: {error}"))
                })?;
            let batch = collect_headers(&fetches);
            if !batch.is_empty() {
                on_batch(batch)?;
            }
            start = end.saturating_add(1);
        }
        return Ok(());
    }
    // Incremental sync: fetch the new UID range directly (no SEARCH) in small
    // windows so a resumed backfill cannot buffer the rest of a huge mailbox in
    // one response.
    let mut from = from_uid;
    while from <= latest_uid {
        let end = from.saturating_add(HEADER_FETCH_PAGE - 1).min(latest_uid);
        let fetches = session
            .uid_fetch(
                format!("{from}:{end}"),
                "(UID FLAGS BODY.PEEK[HEADER] BODY.PEEK[TEXT]<0.4096>)",
            )
            .map_err(|error| AppError::Message(format!("IMAP header fetch failed: {error}")))?;
        let batch = collect_headers(&fetches);
        if !batch.is_empty() {
            on_batch(batch)?;
        }
        if end >= latest_uid {
            break;
        }
        from = end.saturating_add(1);
    }
    Ok(())
}

const HEADER_FETCH_PAGE: u32 = 200;

fn collect_headers(fetches: &[imap::types::Fetch]) -> Vec<FetchedHeader> {
    let mut batch = Vec::new();
    for fetch in fetches.iter() {
        let (Some(uid), Some(raw)) = (fetch.uid, fetch.header()) else {
            continue;
        };
        batch.push(FetchedHeader {
            uid,
            flags: fetch.flags().iter().map(ToString::to_string).collect(),
            raw: raw.to_vec(),
            preview: fetch.text().map(Vec::from).unwrap_or_default(),
        });
    }
    batch
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
    move_selected(&mut session, uid, target_path, account.auth_type)?;
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
    move_selected(&mut session, uid, &target, account.auth_type)?;
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

fn move_selected(
    session: &mut ImapSession,
    uid: u32,
    target_path: &str,
    auth_type: AccountAuth,
) -> AppResult<()> {
    if auth_type == AccountAuth::Proton {
        session
            .run_command_and_check_ok(proton_move_command(uid, target_path))
            .map_err(|error| {
                AppError::Message(format!("IMAP message move failed: {error}"))
            })?;
        return Ok(());
    }
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
    if account.auth_type == AccountAuth::Proton {
        delete_selected_proton(&mut session, uid)?;
        let _ = session.logout();
        return Ok(());
    }
    require_uidplus(uidplus_supported(&mut session))?;
    session
        .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
        .map_err(|_| AppError::Message("IMAP message delete failed".into()))?;
    expunge_uid(&mut session, uid)?;
    let _ = session.logout();
    Ok(())
}

fn delete_selected_proton(session: &mut ImapSession, uid: u32) -> AppResult<()> {
    let [mark, expunge] = proton_delete_command(uid);
    session
        .run_command_and_check_ok(&mark)
        .map_err(|error| {
            AppError::Message(format!("IMAP message delete failed: {error}"))
        })?;
    session
        .run_command_and_check_ok(&expunge)
        .map_err(|error| AppError::Message(format!("IMAP expunge failed: {error}")))?;
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
    let security = session_security(account)?;
    let client = connect_client(
        &account.imap_host,
        account.imap_port,
        account.auth_type,
        security,
    )?;
    authenticate(client, &account.username, credential)
}

fn session_security(account: &Account) -> AppResult<Option<MailSecurity>> {
    if account.auth_type == AccountAuth::Proton {
        Ok(None)
    } else {
        MailSecurity::parse(&account.imap_security).map(Some)
    }
}

fn require_proton_host(host: &str) -> AppResult<()> {
    if proton_host_allowed(host) {
        Ok(())
    } else {
        Err(AppError::Message(
            "Proton Bridge must be reached over loopback".into(),
        ))
    }
}

fn connect_client(
    host: &str,
    port: u16,
    auth_type: AccountAuth,
    security: Option<MailSecurity>,
) -> AppResult<imap::Client<ImapStream>> {
    if auth_type == AccountAuth::Proton {
        require_proton_host(host)?;
        let tcp = connect_tcp(host, port)?;
        let mut client = imap::Client::new(ImapStream::Plain(tcp));
        client
            .read_greeting()
            .map_err(|_| AppError::Message("IMAP connection failed".into()))?;
        return Ok(client);
    }

    let security =
        security.ok_or_else(|| AppError::Message("security must be tls or starttls".into()))?;
    let tcp = connect_tcp(host, port)?;
    let tls = TlsConnector::builder()
        .build()
        .map_err(|_| AppError::Message("IMAP TLS configuration failed".into()))?;

    let client = match security {
        MailSecurity::Tls => {
            let stream = tls
                .connect(host, tcp)
                .map_err(|_| AppError::Message("IMAP TLS handshake failed".into()))?;
            let mut client = imap::Client::new(ImapStream::Tls(stream));
            client
                .read_greeting()
                .map_err(|_| AppError::Message("IMAP connection failed".into()))?;
            client
        }
        MailSecurity::StartTls => {
            let stream = starttls_exchange(tcp)
                .map_err(|_| AppError::Message("IMAP TLS handshake failed".into()))?;
            let stream = tls
                .connect(host, stream)
                .map_err(|_| AppError::Message("IMAP TLS handshake failed".into()))?;
            imap::Client::new(ImapStream::Tls(stream))
        }
    };
    Ok(client)
}

fn starttls_exchange(mut stream: TcpStream) -> io::Result<TcpStream> {
    read_imap_line(&mut stream)?;
    stream.write_all(b"auramail1 STARTTLS\r\n")?;
    loop {
        let line = read_imap_line(&mut stream)?;
        if line.starts_with(b"*") {
            continue;
        }
        let Some(status) = line
            .strip_prefix(STARTTLS_TAG)
            .and_then(|rest| rest.strip_prefix(b" "))
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected IMAP STARTTLS response",
            ));
        };
        if status == b"OK" || status.starts_with(b"OK ") {
            return Ok(stream);
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IMAP STARTTLS rejected",
        ));
    }
}

fn read_imap_line(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if stream.read(&mut byte)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "IMAP connection closed",
            ));
        }
        if byte[0] == b'\n' {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(line);
        }
        line.push(byte[0]);
        if line.len() > MAX_IMAP_LINE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "IMAP line is too long",
            ));
        }
    }
}

fn authenticate(
    client: imap::Client<ImapStream>,
    username: &str,
    credential: &MailCredential,
) -> AppResult<ImapSession> {
    match credential {
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
    }
}

fn connect_tcp(host: &str, port: u16) -> AppResult<TcpStream> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_| AppError::Message("IMAP connection failed".into()))?;
    let mut timed_out = false;
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
                let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
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
            "inbox" => return Some("inbox"),
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
    use crate::accounts::{AccountAuth, MailSecurity};
    use imap::Authenticator;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn account(auth_type: AccountAuth) -> Account {
        Account {
            id: 1,
            display_name: "Me".into(),
            email: "me@example.com".into(),
            username: "me@example.com".into(),
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            imap_security: "tls".into(),
            smtp_host: "smtp.example.com".into(),
            smtp_port: 465,
            smtp_security: "tls".into(),
            color: "accent".into(),
            signature: String::new(),
            signature_fields: Default::default(),
            signature_on_new: true,
            auth_type,
            oauth_provider: String::new(),
            space_id: 1,
            position: 0,
            sync_interval_secs: 120,
            created_at: 0,
        }
    }

    fn proton_account() -> Account {
        let mut proton = account(AccountAuth::Proton);
        proton.imap_host = "127.0.0.1".into();
        proton.imap_port = 1143;
        proton.imap_security = "plain".into();
        proton.smtp_security = "plain".into();
        proton
    }

    fn closed_loopback_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }

    #[test]
    fn accepts_only_loopback_proton_hosts() {
        for host in ["127.0.0.1", "::1", "[::1]", "localhost", "LOCALHOST"] {
            assert!(proton_host_allowed(host), "expected {host} to be allowed");
        }
        for host in [
            "imap.example.com",
            "127.0.0.1.example.com",
            "localhost.evil.com",
            "0.0.0.0",
        ] {
            assert!(!proton_host_allowed(host), "expected {host} to be refused");
        }
        assert!(!proton_host_allowed(""));
    }

    #[test]
    fn proton_move_uses_uid_move_with_a_quoted_mailbox() {
        assert_eq!(
            proton_move_command(42, "Archive"),
            "UID MOVE 42 \"Archive\""
        );
        assert_eq!(
            proton_move_command(7, "Sent \"Items\""),
            "UID MOVE 7 \"Sent \\\"Items\\\"\""
        );
    }

    #[test]
    fn proton_delete_marks_deleted_then_expunges() {
        assert_eq!(
            proton_delete_command(42),
            [
                "UID STORE 42 +FLAGS (\\Deleted)".to_string(),
                "EXPUNGE".to_string()
            ]
        );
    }

    #[test]
    fn selects_plaintext_only_for_proton_accounts() {
        assert_eq!(session_security(&proton_account()).unwrap(), None);

        let mut manual = account(AccountAuth::Password);
        manual.imap_security = "plain".into();
        assert!(session_security(&manual).is_err());

        let tls = account(AccountAuth::Password);
        assert_eq!(session_security(&tls).unwrap(), Some(MailSecurity::Tls));

        let mut oauth = account(AccountAuth::OAuth);
        oauth.imap_security = "starttls".into();
        assert_eq!(
            session_security(&oauth).unwrap(),
            Some(MailSecurity::StartTls)
        );
    }

    #[test]
    fn proton_refuses_a_non_loopback_imap_host_without_connecting() {
        let mut proton = proton_account();
        proton.imap_host = "imap.example.com".into();
        let error =
            open_session(&proton, &MailCredential::Password("bridge-pass".into())).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Proton Bridge must be reached over loopback"
        );

        let error = validate_proton_imap(
            "imap.example.com",
            1143,
            "me@example.com",
            &MailCredential::Password("bridge-pass".into()),
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Proton Bridge must be reached over loopback"
        );
    }

    #[test]
    fn proton_skips_security_parsing_and_uses_plain_tcp() {
        let mut proton = proton_account();
        proton.imap_port = closed_loopback_port();
        let error =
            open_session(&proton, &MailCredential::Password("bridge-pass".into())).unwrap_err();
        assert!(
            error.to_string().starts_with("IMAP connection"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn proton_validation_rejects_blank_host_and_zero_port() {
        let credential = MailCredential::Password("bridge-pass".into());
        assert!(validate_proton_imap("", 1143, "me@example.com", &credential).is_err());
        assert!(validate_proton_imap("127.0.0.1", 0, "me@example.com", &credential).is_err());
    }

    #[test]
    fn plain_imap_stream_round_trips_bytes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            let mut request = [0u8; 4];
            peer.read_exact(&mut request).unwrap();
            assert_eq!(&request, b"ping");
            peer.write_all(b"pong").unwrap();
        });

        let tcp = TcpStream::connect(address).unwrap();
        let mut stream = ImapStream::Plain(tcp);
        stream.write_all(b"ping").unwrap();
        let mut response = [0u8; 4];
        stream.read_exact(&mut response).unwrap();
        assert_eq!(&response, b"pong");
        handle.join().unwrap();
    }

    fn fake_imap_server(response: &'static str) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            peer.write_all(b"* OK IMAP4rev1 ready\r\n").unwrap();
            let mut reader = BufReader::new(peer.try_clone().unwrap());
            let mut command = String::new();
            reader.read_line(&mut command).unwrap();
            let tag = std::str::from_utf8(STARTTLS_TAG).unwrap();
            assert!(
                command.starts_with(&format!("{tag} ")) && command.ends_with(" STARTTLS\r\n"),
                "unexpected command: {command:?}"
            );
            let command_tag = command.split(' ').next().unwrap();
            peer.write_all(format!("{command_tag} {response}\r\n").as_bytes())
                .unwrap();
            if response.starts_with("OK") {
                let mut ping = String::new();
                reader.read_line(&mut ping).unwrap();
                assert_eq!(ping, "ping\r\n");
                peer.write_all(b"pong\r\n").unwrap();
            }
        });
        (port, handle)
    }

    #[test]
    fn starttls_exchange_accepts_an_ok_response() {
        let (port, handle) = fake_imap_server("OK STARTTLS completed");
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut stream = starttls_exchange(stream).unwrap();
        stream.write_all(b"ping\r\n").unwrap();
        assert_eq!(read_imap_line(&mut stream).unwrap(), b"pong".to_vec());
        handle.join().unwrap();
    }

    #[test]
    fn starttls_exchange_rejects_no_bad_and_malformed_responses() {
        for response in ["NO STARTTLS unavailable", "BAD unexpected command", "WHAT"] {
            let (port, handle) = fake_imap_server(response);
            let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            assert!(
                starttls_exchange(stream).is_err(),
                "expected {response:?} to be rejected"
            );
            handle.join().unwrap();
        }
    }

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
        // Yandex marks INBOX with \Inbox even though it does not advertise
        // SPECIAL-USE; the role must not depend on the mailbox name.
        let attributes = vec![NameAttribute::Custom("\\Inbox".into())];
        assert_eq!(special_use_role(&attributes), Some("inbox"));
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

    fn scripted_imap_server<F>(handler: F) -> (u16, thread::JoinHandle<Vec<String>>)
    where
        F: Fn(&str, &str) -> String + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (peer, _) = listener.accept().unwrap();
            let mut writer = peer.try_clone().unwrap();
            writer.write_all(b"* OK IMAP4rev1 ready\r\n").unwrap();
            let mut reader = BufReader::new(peer);
            let mut commands = Vec::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let tag = line.split(' ').next().unwrap_or_default().to_string();
                commands.push(line.trim_end().to_string());
                let response = handler(&line, &tag);
                if response.is_empty() || line.starts_with(&format!("{tag} LOGOUT")) {
                    break;
                }
                writer.write_all(response.as_bytes()).unwrap();
            }
            commands
        });
        (port, handle)
    }

    fn yandex_like_handler(line: &str, tag: &str) -> String {
        let upper = line.to_ascii_uppercase();
        if upper.contains(" UID SEARCH") {
            format!("{tag} NO [BACKEND] search is unavailable\r\n")
        } else if upper.contains(" SELECT ") {
            format!(
                "* 2 EXISTS\r\n* OK [UIDVALIDITY 7] ok\r\n* OK [UIDNEXT 3] ok\r\n\
                 {tag} OK [READ-WRITE] SELECT completed\r\n"
            )
        } else if upper.contains("FETCH") {
            format!("{tag} OK FETCH completed\r\n")
        } else {
            format!("{tag} OK completed\r\n")
        }
    }

    fn connect_plain(port: u16) -> ImapSession {
        let tcp = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut client = imap::Client::new(ImapStream::Plain(tcp));
        client.read_greeting().unwrap();
        client.login("me@example.com", "pass").ok().unwrap()
    }

    #[test]
    fn initial_sync_does_not_depend_on_uid_search() {
        // Yandex intermittently answers UID SEARCH with NO [BACKEND]. The
        // initial import must page through the mailbox with FETCH instead of
        // failing the whole folder (INBOX) before fetching anything.
        let (port, handle) = scripted_imap_server(yandex_like_handler);
        let mut session = connect_plain(port);
        let mut batches = 0;
        fetch_headers_batched(&mut session, "INBOX", 1, 2, |_| {
            batches += 1;
            Ok(())
        })
        .expect("initial sync must not require UID SEARCH");
        assert_eq!(batches, 0);
        drop(session);
        let commands = handle.join().unwrap();
        assert!(
            !commands.iter().any(|line| line.contains("UID SEARCH")),
            "unexpected SEARCH commands: {commands:?}"
        );
    }

    #[test]
    fn incremental_sync_fetches_the_uid_range_directly() {
        let (port, handle) = scripted_imap_server(yandex_like_handler);
        let mut session = connect_plain(port);
        fetch_headers_batched(&mut session, "INBOX", 5, 9, |_| Ok(())).unwrap();
        drop(session);
        let commands = handle.join().unwrap();
        assert!(
            commands
                .iter()
                .any(|line| line.contains("UID FETCH 5:9")),
            "expected a direct UID FETCH: {commands:?}"
        );
        assert!(
            !commands.iter().any(|line| line.contains("UID SEARCH")),
            "unexpected SEARCH commands: {commands:?}"
        );
    }
}
