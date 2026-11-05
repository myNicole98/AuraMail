use std::fmt;
use std::future::Future;
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::sync::OnceLock;
use std::task::{Context, Poll};
use std::time::Duration;

use async_imap::types::NameAttribute;
use futures_util::TryStreamExt;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

use crate::accounts::{Account, AccountAuth, MailSecurity};
use crate::error::{AppError, AppResult};
use crate::mail_auth::MailCredential;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Fail a stalled connection only when no bytes arrive for this long. A slow
/// but flowing page (even one taking many minutes) is allowed to finish.
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Raw messages above this size are refused so opening mail cannot exhaust memory.
const MAX_MESSAGE_BYTES: u64 = 60 * 1024 * 1024;

pub struct ImapStream {
    kind: StreamKind,
    idle: Pin<Box<tokio::time::Sleep>>,
}

enum StreamKind {
    Plain(TcpStream),
    Tls(tokio_native_tls::TlsStream<TcpStream>),
}

impl ImapStream {
    pub fn plain(stream: TcpStream) -> Self {
        Self {
            kind: StreamKind::Plain(stream),
            idle: Box::pin(tokio::time::sleep(IDLE_TIMEOUT)),
        }
    }

    pub fn tls(stream: tokio_native_tls::TlsStream<TcpStream>) -> Self {
        Self {
            kind: StreamKind::Tls(stream),
            idle: Box::pin(tokio::time::sleep(IDLE_TIMEOUT)),
        }
    }

    fn touch(&mut self) {
        self.idle
            .as_mut()
            .reset(tokio::time::Instant::now() + IDLE_TIMEOUT);
    }

    fn into_plain(self) -> Option<TcpStream> {
        match self.kind {
            StreamKind::Plain(stream) => Some(stream),
            StreamKind::Tls(_) => None,
        }
    }
}

impl fmt::Debug for ImapStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            StreamKind::Plain(_) => f.write_str("ImapStream::Plain"),
            StreamKind::Tls(_) => f.write_str("ImapStream::Tls"),
        }
    }
}

impl AsyncRead for ImapStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let result = match &mut this.kind {
            StreamKind::Plain(stream) => Pin::new(stream).poll_read(cx, buffer),
            StreamKind::Tls(stream) => Pin::new(stream).poll_read(cx, buffer),
        };
        match result {
            Poll::Ready(Ok(())) => {
                if !buffer.filled().is_empty() {
                    this.touch();
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => {
                if this.idle.as_mut().poll(cx).is_ready() {
                    Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "IMAP connection stalled",
                    )))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

impl AsyncWrite for ImapStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let result = match &mut this.kind {
            StreamKind::Plain(stream) => Pin::new(stream).poll_write(cx, buffer),
            StreamKind::Tls(stream) => Pin::new(stream).poll_write(cx, buffer),
        };
        if matches!(result, Poll::Ready(Ok(_))) {
            this.touch();
        }
        result
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        match &mut self.get_mut().kind {
            StreamKind::Plain(stream) => Pin::new(stream).poll_flush(cx),
            StreamKind::Tls(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        match &mut self.get_mut().kind {
            StreamKind::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            StreamKind::Tls(stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

pub type ImapSession = async_imap::Session<ImapStream>;

/// async-imap needs a Tokio reactor. The app keeps its synchronous call sites
/// and drives the IMAP futures on this dedicated runtime.
fn imap_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("auramail-imap")
            .build()
            .expect("IMAP runtime")
    })
}

fn run<F: Future>(future: F) -> F::Output {
    let runtime = imap_runtime();
    match tokio::runtime::Handle::try_current() {
        Ok(_) => tokio::task::block_in_place(|| runtime.block_on(future)),
        Err(_) => runtime.block_on(future),
    }
}


/// Proton Bridge only listens on the local machine; plaintext never leaves it.
pub fn proton_host_allowed(host: &str) -> bool {
    let host = host.trim();
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    host == "127.0.0.1" || host == "::1" || host.eq_ignore_ascii_case("localhost")
}

pub fn proton_move_command_set(uids: &str, target: &str) -> String {
    format!("UID MOVE {uids} {}", quoted(target))
}

pub fn proton_delete_command_set(uids: &str) -> [String; 2] {
    [
        format!("UID STORE {uids} +FLAGS (\\Deleted)"),
        "EXPUNGE".to_string(),
    ]
}

struct OAuth2Authenticator<'a> {
    user: &'a str,
    access_token: &'a str,
}

impl async_imap::Authenticator for OAuth2Authenticator<'_> {
    type Response = String;
    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
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
    pub exists: u32,
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

    run(async {
        let client = connect_client(host, port, auth_type, security).await?;
        let mut session = authenticate(client, username, credential).await?;
        session
            .select("INBOX")
            .await
            .map_err(|_| AppError::Message("IMAP mailbox validation failed".into()))?;
        session
            .list(None, Some("*"))
            .await
            .map_err(|_| AppError::Message("IMAP mailbox validation failed".into()))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|_| AppError::Message("IMAP mailbox validation failed".into()))?;
        let _ = session.logout().await;
        Ok(())
    })
}

pub fn list_folders(session: &mut ImapSession) -> AppResult<Vec<RemoteFolder>> {
    run(async {
        let names = session
            .list(None, Some("*"))
            .await
            .map_err(|error| AppError::Message(format!("IMAP folder list failed: {error}")))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|error| AppError::Message(format!("IMAP folder list failed: {error}")))?;
        let mut folders = names.iter().filter_map(remote_folder).collect::<Vec<_>>();
        // Some providers (Yandex) do not reliably include the special INBOX name
        // in a wildcard listing. INBOX always exists, so ask for it explicitly.
        if !folders
            .iter()
            .any(|folder| folder.path.eq_ignore_ascii_case("INBOX"))
        {
            if let Ok(inbox) = session.list(None, Some("INBOX")).await {
                if let Ok(inbox) = inbox.try_collect::<Vec<_>>().await {
                    folders.extend(inbox.iter().filter_map(remote_folder));
                }
            }
        }
        Ok(folders)
    })
}

fn remote_folder(name: &async_imap::types::Name) -> Option<RemoteFolder> {
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
    run(async {
        let mailbox = session
            .status(folder_path, "(UIDNEXT UIDVALIDITY MESSAGES)")
            .await
            .map_err(|error| AppError::Message(format!("IMAP folder status failed: {error}")))?;
        if let Some(next_uid) = mailbox.uid_next {
            return Ok(FolderState {
                uid_validity: mailbox.uid_validity.unwrap_or(0),
                latest_uid: next_uid.saturating_sub(1),
                exists: mailbox.exists,
            });
        }
        let mailbox = session
            .select(folder_path)
            .await
            .map_err(|error| AppError::Message(format!("IMAP folder select failed: {error}")))?;
        let uids = session
            .uid_search("ALL")
            .await
            .map_err(|error| AppError::Message(format!("IMAP UID search failed: {error}")))?;
        Ok(FolderState {
            uid_validity: mailbox.uid_validity.unwrap_or(0),
            latest_uid: uids.into_iter().max().unwrap_or(0),
            exists: mailbox.exists,
        })
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
    run(async {
        session
            .select(folder_path)
            .await
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
                .await
                .map_err(|error| AppError::Message(format!("IMAP flag fetch failed: {error}")))?
                .try_collect::<Vec<_>>()
                .await
                .map_err(|error| AppError::Message(format!("IMAP flag fetch failed: {error}")))?;
            result.extend(fetches.iter().filter_map(|fetch| {
                Some((
                    fetch.uid?,
                    fetch.flags().map(|flag| flag_text(&flag)).collect(),
                ))
            }));
        }
        Ok(result)
    })
}

/// Import a folder from the newest message backwards. `start_seq` is the
/// sequence number to start at (usually the mailbox EXISTS count). The
/// callback receives the page and the next sequence to continue from (0 when
/// the folder has been fully imported) so it can persist progress after every
/// page.
pub fn fetch_headers_backfill<F>(
    session: &mut ImapSession,
    folder_path: &str,
    start_seq: u32,
    max_pages: usize,
    mut on_batch: F,
) -> AppResult<()>
where
    F: FnMut(Vec<FetchedHeader>, u32) -> AppResult<()>,
{
    if start_seq == 0 || max_pages == 0 {
        return Ok(());
    }
    let mailbox = run(async {
        session.select(folder_path).await.map_err(|error| {
            AppError::Message(format!("IMAP folder select failed: {error}"))
        })
    })?;
    let mut end = start_seq.min(mailbox.exists);
    if end == 0 {
        on_batch(Vec::new(), 0)?;
        return Ok(());
    }
    for _ in 0..max_pages {
        let begin = end.saturating_sub(HEADER_FETCH_PAGE - 1).max(1);
        let set = format!("{begin}:{end}");
        let fetches = run(async {
            session
                .fetch(&set, HEADER_FETCH_QUERY)
                .await
                .map_err(|error| header_fetch_error(&set, error))?
                .try_collect::<Vec<_>>()
                .await
                .map_err(|error| header_fetch_error(&set, error))
        })?;
        let batch = collect_headers(&fetches);
        let next = begin.saturating_sub(1);
        on_batch(batch, next)?;
        if next == 0 {
            break;
        }
        end = next;
    }
    Ok(())
}

/// Fetch new messages by UID range. No `UID SEARCH` is used: Yandex
/// intermittently answers SEARCH with `NO [BACKEND]`, which used to fail the
/// whole folder before a single message was fetched. The initial import uses
/// `fetch_headers_backfill` (newest first).
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
    if from_uid > latest_uid {
        return Ok(());
    }
    run(async {
        session.select(folder_path).await.map_err(|error| {
            AppError::Message(format!("IMAP folder select failed: {error}"))
        })
    })?;
    // Incremental sync: fetch the new UID range directly (no SEARCH) in small
    // windows so a resumed backfill cannot buffer the rest of a huge mailbox in
    // one response.
    let mut from = from_uid;
    while from <= latest_uid {
        let end = from.saturating_add(HEADER_FETCH_PAGE - 1).min(latest_uid);
        let set = format!("{from}:{end}");
        let fetches = run(async {
            session
                .uid_fetch(&set, HEADER_FETCH_QUERY)
                .await
                .map_err(|error| header_fetch_error(&set, error))?
                .try_collect::<Vec<_>>()
                .await
                .map_err(|error| header_fetch_error(&set, error))
        })?;
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
const HEADER_FETCH_QUERY: &str =
    "(UID FLAGS BODY.PEEK[HEADER] BODY.PEEK[TEXT]<0.4096>)";

fn header_fetch_error(range: &str, error: async_imap::error::Error) -> AppError {
    AppError::Message(format!(
        "IMAP header fetch failed for {range}: {error}"
    ))
}

fn collect_headers(fetches: &[async_imap::types::Fetch]) -> Vec<FetchedHeader> {
    let mut batch = Vec::new();
    for fetch in fetches.iter() {
        let (Some(uid), Some(raw)) = (fetch.uid, fetch.header()) else {
            continue;
        };
        batch.push(FetchedHeader {
            uid,
            flags: fetch.flags().map(|flag| flag_text(&flag)).collect(),
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
    run(async {
        let mut session = open_session_async(account, credential).await?;
        session
            .select(folder_path)
            .await
            .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
        let sizes = session
            .uid_fetch(uid.to_string(), "(RFC822.SIZE)")
            .await
            .map_err(|_| AppError::Message("IMAP message fetch failed".into()))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|_| AppError::Message("IMAP message fetch failed".into()))?;
        let size = sizes.iter().find_map(|fetch| fetch.size).unwrap_or(0) as u64;
        if exceeds_message_limit(size) {
            let _ = session.logout().await;
            return Err(AppError::Message("message is too large to open".into()));
        }
        let fetches = session
            .uid_fetch(uid.to_string(), "(BODY.PEEK[])")
            .await
            .map_err(|_| AppError::Message("IMAP message fetch failed".into()))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|_| AppError::Message("IMAP message fetch failed".into()))?;
        let raw = fetches
            .iter()
            .find_map(|fetch| fetch.body())
            .map(Vec::from)
            .ok_or_else(|| AppError::Message("IMAP message body is unavailable".into()))?;
        if exceeds_message_limit(raw.len() as u64) {
            let _ = session.logout().await;
            return Err(AppError::Message("message is too large to open".into()));
        }
        let _ = session.logout().await;
        Ok(raw)
    })
}

fn exceeds_message_limit(size: u64) -> bool {
    size > MAX_MESSAGE_BYTES
}

/// Open a session and resolve a message's current UID by its Message-ID header.
pub fn find_message_uid(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    message_id: &str,
) -> AppResult<Option<u32>> {
    run(async {
        let mut session = open_session_async(account, credential).await?;
        session
            .select(folder_path)
            .await
            .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
        let uids = session
            .uid_search(format!("HEADER Message-ID \"{message_id}\""))
            .await
            .map_err(|_| AppError::Message("IMAP message search failed".into()))?;
        let _ = session.logout().await;
        Ok(uids.into_iter().max())
    })
}

/// Fetch headers and previews for specific UIDs, e.g. when filling snippets.
pub fn fetch_headers_for(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uids: &[u32],
) -> AppResult<Vec<FetchedHeader>> {
    if uids.is_empty() {
        return Ok(Vec::new());
    }
    run(async {
        let mut session = open_session_async(account, credential).await?;
        session
            .select(folder_path)
            .await
            .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
        let set = uids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let fetches = session
            .uid_fetch(
                set,
                "(UID BODY.PEEK[HEADER] BODY.PEEK[TEXT]<0.8192>)",
            )
            .await
            .map_err(|_| AppError::Message("IMAP message fetch failed".into()))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|_| AppError::Message("IMAP message fetch failed".into()))?;
        let headers = collect_headers(&fetches);
        let _ = session.logout().await;
        Ok(headers)
    })
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

/// Set one flag on many messages with one session and UID-set commands.
pub fn set_seen_many(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uids: &[u32],
    seen: bool,
) -> AppResult<()> {
    store_flag_many(account, credential, folder_path, uids, "\\Seen", seen)
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
    move_many(account, credential, folder_path, &[uid], target_path)
}

/// Move a whole selection with one session and UID-set commands. Providers
/// throttle per connection, so batching N messages into a handful of commands
/// is what keeps a 200-message action from opening 200 sessions.
pub fn move_many(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uids: &[u32],
    target_path: &str,
) -> AppResult<()> {
    if uids.is_empty() {
        return Ok(());
    }
    run(async {
        let mut session = open_session_async(account, credential).await?;
        session
            .select(folder_path)
            .await
            .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
        move_selected_many(&mut session, uids, target_path, account.auth_type).await?;
        let _ = session.logout().await;
        Ok(())
    })
}

/// Permanently delete a selection with one session and UID-set commands.
pub fn delete_many(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uids: &[u32],
) -> AppResult<()> {
    if uids.is_empty() {
        return Ok(());
    }
    run(async {
        let mut session = open_session_async(account, credential).await?;
        session
            .select(folder_path)
            .await
            .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
        delete_selected_many(&mut session, uids, account.auth_type).await?;
        let _ = session.logout().await;
        Ok(())
    })
}

pub fn archive(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uid: u32,
    archive_path: Option<&str>,
    all_path: Option<&str>,
) -> AppResult<()> {
    run(async {
        let mut session = open_session_async(account, credential).await?;
        session
            .select(folder_path)
            .await
            .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
        let gmail = session
            .capabilities()
            .await
            .map(|capabilities| capabilities.has_str("X-GM-EXT-1"))
            .unwrap_or(false);
        let target = archive_destination(archive_path, all_path, gmail)?;
        move_selected(&mut session, uid, &target, account.auth_type).await?;
        let _ = session.logout().await;
        Ok(())
    })
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

async fn move_selected(
    session: &mut ImapSession,
    uid: u32,
    target_path: &str,
    auth_type: AccountAuth,
) -> AppResult<()> {
    move_selected_many(session, &[uid], target_path, auth_type).await
}

/// Long UID lists are split so one command never grows without bound.
const MAX_UIDS_PER_COMMAND: usize = 200;

/// IMAP sequence sets accept ranges, so `[3, 1, 2]` becomes `1:3` and a
/// 200-message move stays a short line on the wire.
fn format_uid_set(uids: &[u32]) -> String {
    let mut sorted = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut parts = Vec::new();
    let mut index = 0;
    while index < sorted.len() {
        let start = sorted[index];
        let mut end = start;
        while index + 1 < sorted.len() && sorted[index + 1] == end.saturating_add(1) {
            index += 1;
            end = sorted[index];
        }
        if start == end {
            parts.push(start.to_string());
        } else {
            parts.push(format!("{start}:{end}"));
        }
        index += 1;
    }
    parts.join(",")
}

async fn move_selected_many(
    session: &mut ImapSession,
    uids: &[u32],
    target_path: &str,
    auth_type: AccountAuth,
) -> AppResult<()> {
    if auth_type == AccountAuth::Proton {
        for chunk in uids.chunks(MAX_UIDS_PER_COMMAND) {
            let set = format_uid_set(chunk);
            session
                .run_command_and_check_ok(proton_move_command_set(&set, target_path))
                .await
                .map_err(|error| {
                    AppError::Message(format!("IMAP message move failed: {error}"))
                })?;
        }
        return Ok(());
    }
    require_uidplus(uidplus_supported(session).await)?;
    for chunk in uids.chunks(MAX_UIDS_PER_COMMAND) {
        let set = format_uid_set(chunk);
        // async-imap quotes mailbox names itself; pass the raw path.
        session
            .uid_copy(set.clone(), target_path)
            .await
            .map_err(|error| AppError::Message(format!("IMAP message move failed: {error}")))?;
        session
            .uid_store(set.clone(), "+FLAGS (\\Deleted)")
            .await
            .map_err(|error| AppError::Message(format!("IMAP message delete failed: {error}")))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|error| AppError::Message(format!("IMAP message delete failed: {error}")))?;
        expunge_uid_set(session, &set).await?;
    }
    Ok(())
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
    run(async {
        let mut session = open_session_async(account, credential).await?;
        session
            .select(folder_path)
            .await
            .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
        if account.auth_type == AccountAuth::Proton {
            delete_selected_proton(&mut session, uid).await?;
            let _ = session.logout().await;
            return Ok(());
        }
        require_uidplus(uidplus_supported(&mut session).await)?;
        session
            .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
            .await
            .map_err(|error| AppError::Message(format!("IMAP message delete failed: {error}")))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|error| AppError::Message(format!("IMAP message delete failed: {error}")))?;
        expunge_uid(&mut session, uid).await?;
        let _ = session.logout().await;
        Ok(())
    })
}

async fn delete_selected_many(
    session: &mut ImapSession,
    uids: &[u32],
    auth_type: AccountAuth,
) -> AppResult<()> {
    if uids.is_empty() {
        return Ok(());
    }
    if auth_type == AccountAuth::Proton {
        for chunk in uids.chunks(MAX_UIDS_PER_COMMAND) {
            delete_selected_proton_set(session, &format_uid_set(chunk)).await?;
        }
        return Ok(());
    }
    require_uidplus(uidplus_supported(session).await)?;
    for chunk in uids.chunks(MAX_UIDS_PER_COMMAND) {
        let set = format_uid_set(chunk);
        session
            .uid_store(set.clone(), "+FLAGS (\\Deleted)")
            .await
            .map_err(|error| AppError::Message(format!("IMAP message delete failed: {error}")))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|error| AppError::Message(format!("IMAP message delete failed: {error}")))?;
        expunge_uid_set(session, &set).await?;
    }
    Ok(())
}

async fn delete_selected_proton(session: &mut ImapSession, uid: u32) -> AppResult<()> {
    delete_selected_proton_set(session, &uid.to_string()).await
}

async fn delete_selected_proton_set(session: &mut ImapSession, uids: &str) -> AppResult<()> {
    let [mark, expunge] = proton_delete_command_set(uids);
    session
        .run_command_and_check_ok(&mark)
        .await
        .map_err(|error| {
            AppError::Message(format!("IMAP message delete failed: {error}"))
        })?;
    session
        .run_command_and_check_ok(&expunge)
        .await
        .map_err(|error| AppError::Message(format!("IMAP expunge failed: {error}")))?;
    Ok(())
}

async fn uidplus_supported(session: &mut ImapSession) -> bool {
    session
        .capabilities()
        .await
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

async fn expunge_uid(session: &mut ImapSession, uid: u32) -> AppResult<()> {
    require_uidplus(uidplus_supported(session).await)?;
    expunge_uid_set(session, &uid.to_string()).await
}

async fn expunge_uid_set(session: &mut ImapSession, uids: &str) -> AppResult<()> {
    session
        .uid_expunge(uids.to_string())
        .await
        .map_err(|error| AppError::Message(format!("IMAP expunge failed: {error}")))?
        .try_collect::<Vec<_>>()
        .await
        .map_err(|error| AppError::Message(format!("IMAP expunge failed: {error}")))?;
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
    store_flag_many(account, credential, folder_path, &[uid], flag, enabled)
}

fn store_flag_many(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    uids: &[u32],
    flag: &str,
    enabled: bool,
) -> AppResult<()> {
    if uids.is_empty() {
        return Ok(());
    }
    run(async {
        let mut session = open_session_async(account, credential).await?;
        session
            .select(folder_path)
            .await
            .map_err(|_| AppError::Message("IMAP folder select failed".into()))?;
        store_flags_on(&mut session, uids, flag, enabled).await?;
        let _ = session.logout().await;
        Ok(())
    })
}

async fn store_flags_on(
    session: &mut ImapSession,
    uids: &[u32],
    flag: &str,
    enabled: bool,
) -> AppResult<()> {
    let operation = if enabled { "+FLAGS" } else { "-FLAGS" };
    for chunk in uids.chunks(MAX_UIDS_PER_COMMAND) {
        session
            .uid_store(format_uid_set(chunk), format!("{operation} ({flag})"))
            .await
            .map_err(|error| AppError::Message(format!("IMAP flag update failed: {error}")))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|error| AppError::Message(format!("IMAP flag update failed: {error}")))?;
    }
    Ok(())
}

pub(crate) fn open_session(
    account: &Account,
    credential: &MailCredential,
) -> AppResult<ImapSession> {
    run(open_session_async(account, credential))
}

/// Upload a copy of a message to a folder. SMTP submission leaves no copy on
/// the server for most providers, so a sent mail only exists in the
/// recipient's mailbox until the client appends it to Sent.
pub fn append_message(
    account: &Account,
    credential: &MailCredential,
    folder_path: &str,
    flags: &[&str],
    raw: &[u8],
) -> AppResult<()> {
    let mut session = open_session(account, credential)?;
    let result = append_to_session(&mut session, folder_path, flags, raw);
    let _ = run(async { session.logout().await });
    result
}

fn append_to_session(
    session: &mut ImapSession,
    folder_path: &str,
    flags: &[&str],
    raw: &[u8],
) -> AppResult<()> {
    // async-imap splices the flag argument verbatim, so the parentheses that
    // make it an IMAP flag-list are ours to add.
    let flag_list = format!("({})", flags.join(" "));
    run(async {
        session
            .append(folder_path, Some(&flag_list), None, raw)
            .await
            .map_err(|error| AppError::Message(format!("IMAP append failed: {error}")))
    })
}

async fn open_session_async(
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
    )
    .await?;
    authenticate(client, &account.username, credential).await
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

async fn connect_client(
    host: &str,
    port: u16,
    auth_type: AccountAuth,
    security: Option<MailSecurity>,
) -> AppResult<async_imap::Client<ImapStream>> {
    if auth_type == AccountAuth::Proton {
        require_proton_host(host)?;
        let tcp = connect_tcp(host, port).await?;
        let mut client = async_imap::Client::new(ImapStream::plain(tcp));
        client
            .read_response()
            .await
            .map_err(|_| AppError::Message("IMAP connection failed".into()))?;
        return Ok(client);
    }

    let security =
        security.ok_or_else(|| AppError::Message("security must be tls or starttls".into()))?;
    let tcp = connect_tcp(host, port).await?;
    match security {
        MailSecurity::Tls => {
            let stream = tls_connect(host, tcp).await?;
            let mut client = async_imap::Client::new(ImapStream::tls(stream));
            client
                .read_response()
                .await
                .map_err(|_| AppError::Message("IMAP connection failed".into()))?;
            Ok(client)
        }
        MailSecurity::StartTls => {
            let mut client = async_imap::Client::new(ImapStream::plain(tcp));
            client
                .read_response()
                .await
                .map_err(|_| AppError::Message("IMAP connection failed".into()))?;
            client
                .run_command_and_check_ok("STARTTLS", None)
                .await
                .map_err(|_| AppError::Message("IMAP STARTTLS rejected".into()))?;
            let tcp = client
                .into_inner()
                .into_plain()
                .ok_or_else(|| AppError::Message("IMAP STARTTLS failed".into()))?;
            let stream = tls_connect(host, tcp).await?;
            Ok(async_imap::Client::new(ImapStream::tls(stream)))
        }
    }
}

async fn tls_connect(
    host: &str,
    tcp: TcpStream,
) -> AppResult<tokio_native_tls::TlsStream<TcpStream>> {
    let connector = native_tls::TlsConnector::builder()
        .build()
        .map_err(|_| AppError::Message("IMAP TLS configuration failed".into()))?;
    tokio_native_tls::TlsConnector::from(connector)
        .connect(host, tcp)
        .await
        .map_err(|_| AppError::Message("IMAP TLS handshake failed".into()))
}

async fn authenticate(
    client: async_imap::Client<ImapStream>,
    username: &str,
    credential: &MailCredential,
) -> AppResult<ImapSession> {
    match credential {
        MailCredential::Password(password) => client
            .login(username, password)
            .await
            .map_err(|(error, _)| {
                AppError::Message(format!("IMAP authentication failed: {error}"))
            }),
        MailCredential::AccessToken(token) => client
            .authenticate(
                "XOAUTH2",
                OAuth2Authenticator {
                    user: username,
                    access_token: token,
                },
            )
            .await
            .map_err(|(error, _)| {
                AppError::Message(format!("IMAP authentication failed: {error}"))
            }),
    }
}

async fn connect_tcp(host: &str, port: u16) -> AppResult<TcpStream> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_| AppError::Message("IMAP connection failed".into()))?;
    let mut timed_out = false;
    for address in addresses {
        match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(address)).await {
            Ok(Ok(stream)) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Ok(Err(_)) => {}
            Err(_) => timed_out = true,
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
        let role = match attribute {
            NameAttribute::All => Some("all"),
            NameAttribute::Archive => Some("archive"),
            NameAttribute::Drafts => Some("drafts"),
            NameAttribute::Junk => Some("junk"),
            NameAttribute::Sent => Some("sent"),
            NameAttribute::Trash => Some("trash"),
            NameAttribute::Extension(value) => {
                match value.trim_start_matches('\\').to_ascii_lowercase().as_str() {
                    "inbox" => Some("inbox"),
                    "spam" => Some("junk"),
                    _ => None,
                }
            }
            _ => None,
        };
        if role.is_some() {
            return role;
        }
    }
    None
}

fn flag_text(flag: &async_imap::types::Flag<'_>) -> String {
    use async_imap::types::Flag;
    match flag {
        Flag::Seen => "\\Seen".into(),
        Flag::Answered => "\\Answered".into(),
        Flag::Flagged => "\\Flagged".into(),
        Flag::Deleted => "\\Deleted".into(),
        Flag::Draft => "\\Draft".into(),
        Flag::Recent => "\\Recent".into(),
        Flag::MayCreate => "\\*".into(),
        Flag::Custom(value) => value.to_string(),
    }
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
    use async_imap::Authenticator;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn account(auth_type: AccountAuth) -> Account {
        Account {
            id: 1,
            display_name: "Me".into(),
            identity: String::new(),
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
            proton_move_command_set("42", "Archive"),
            "UID MOVE 42 \"Archive\""
        );
        assert_eq!(
            proton_move_command_set("7", "Sent \"Items\""),
            "UID MOVE 7 \"Sent \\\"Items\\\"\""
        );
    }

    #[test]
    fn proton_delete_marks_deleted_then_expunges() {
        assert_eq!(
            proton_delete_command_set("42"),
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
    fn move_copies_marks_deleted_and_expunges() {
        let handler = |line: &str, tag: &str| -> String {
            let upper = line.to_ascii_uppercase();
            if upper.contains("CAPABILITY") {
                format!("* CAPABILITY IMAP4rev1 UIDPLUS\r\n{tag} OK CAPABILITY completed\r\n")
            } else if upper.contains("UID COPY") {
                format!("{tag} OK COPY completed\r\n")
            } else if upper.contains("UID STORE") {
                format!("* 1 FETCH (UID 1 FLAGS (\\Deleted))\r\n{tag} OK STORE completed\r\n")
            } else if upper.contains("UID EXPUNGE") {
                format!("* 1 EXPUNGE\r\n{tag} OK EXPUNGE completed\r\n")
            } else {
                format!("{tag} OK completed\r\n")
            }
        };
        let (port, handle) = scripted_imap_server(handler);
        let mut session = connect_plain(port);
        run(move_selected(&mut session, 1, "Deleted Items", AccountAuth::Password)).unwrap();
        drop(session);
        let commands = handle.join().unwrap();
        for expected in ["UID COPY", "UID STORE", "UID EXPUNGE"] {
            assert!(
                commands.iter().any(|line| line.contains(expected)),
                "missing {expected}: {commands:?}"
            );
        }
        assert!(
            commands
                .iter()
                .any(|line| line.contains("UID COPY 1 \"Deleted Items\"")),
            "mailbox must be quoted exactly once: {commands:?}"
        );
        assert!(
            !commands.iter().any(|line| line.contains("\\\"Deleted Items\\\"")),
            "mailbox was double-quoted: {commands:?}"
        );
    }

    #[test]
    fn run_works_from_a_multi_thread_runtime() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let value = runtime.block_on(async { run(async { 7 }) });
        assert_eq!(value, 7);
    }

    #[test]
    fn run_works_from_spawn_blocking() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let value = runtime.block_on(async {
            tokio::task::spawn_blocking(|| run(async { 7 }))
                .await
                .unwrap()
        });
        assert_eq!(value, 7);
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
        let attributes = vec![NameAttribute::Archive];
        assert_eq!(special_use_role(&attributes), Some("archive"));
        // Yandex marks INBOX with \Inbox even though it does not advertise
        // SPECIAL-USE; the role must not depend on the mailbox name.
        let attributes = vec![NameAttribute::Extension("\\Inbox".into())];
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
        let mut auth = OAuth2Authenticator {
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
        run(async {
            let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            let mut client = async_imap::Client::new(ImapStream::plain(tcp));
            client.read_response().await.unwrap();
            client.login("me@example.com", "pass").await.ok().unwrap()
        })
    }

    /// Serves whatever precedes an APPEND, then swallows the literal payload
    /// and acknowledges it, recording both the command and the bytes.
    fn append_server() -> (u16, thread::JoinHandle<Vec<String>>) {
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
                commands.push(line.trim_end().to_string());
                let tag = line.split(' ').next().unwrap_or_default().to_string();
                if line.to_ascii_uppercase().contains(" APPEND ") {
                    let size = line
                        .rsplit('{')
                        .next()
                        .and_then(|tail| tail.trim().trim_end_matches('}').parse().ok())
                        .expect("APPEND must announce a literal size");
                    writer.write_all(b"+ ready\r\n").unwrap();
                    let mut body = vec![0u8; size];
                    reader.read_exact(&mut body).unwrap();
                    reader.read_exact(&mut [0u8; 2]).unwrap();
                    commands.push(String::from_utf8_lossy(&body).to_string());
                    writer
                        .write_all(format!("{tag} OK APPEND completed\r\n").as_bytes())
                        .unwrap();
                    continue;
                }
                if line.starts_with(&format!("{tag} LOGOUT")) {
                    writer
                        .write_all(format!("{tag} OK LOGOUT completed\r\n").as_bytes())
                        .unwrap();
                    break;
                }
                writer
                    .write_all(format!("{tag} OK completed\r\n").as_bytes())
                    .unwrap();
            }
            commands
        });
        (port, handle)
    }

    #[test]
    fn appends_the_sent_copy_to_the_requested_folder() {
        let (port, handle) = append_server();
        let mut session = connect_plain(port);
        let raw = b"Subject: hi\r\n\r\nbody";
        append_to_session(&mut session, "[Gmail]/Sent Mail", &["\\Seen"], raw).unwrap();
        drop(session);
        let commands = handle.join().unwrap();
        let index = commands
            .iter()
            .position(|line| line.contains(" APPEND "))
            .expect("an APPEND should be sent");
        assert!(
            commands[index].contains("APPEND \"[Gmail]/Sent Mail\" (\\Seen) {19}"),
            "got {:?}",
            commands[index]
        );
        assert_eq!(commands[index + 1], "Subject: hi\r\n\r\nbody");
    }

    #[test]
    fn backfill_imports_newest_first_without_uid_search() {
        // Yandex intermittently answers UID SEARCH with NO [BACKEND]. The
        // initial import pages through the mailbox with sequence FETCH instead.
        let (port, handle) = scripted_imap_server(yandex_like_handler);
        let mut session = connect_plain(port);
        let mut next = Vec::new();
        fetch_headers_backfill(&mut session, "INBOX", 2, 10, |_, next_seq| {
            next.push(next_seq);
            Ok(())
        })
        .expect("backfill must not require UID SEARCH");
        assert_eq!(next, vec![0]);
        drop(session);
        let commands = handle.join().unwrap();
        assert!(
            commands.iter().any(|line| line.contains("FETCH 1:2")),
            "expected a sequence FETCH: {commands:?}"
        );
        assert!(
            !commands.iter().any(|line| line.contains("UID SEARCH")),
            "unexpected SEARCH commands: {commands:?}"
        );
    }

    #[test]
    fn backfill_pages_from_the_newest_message_backwards() {
        let handler = |line: &str, tag: &str| -> String {
            let upper = line.to_ascii_uppercase();
            if upper.contains(" SELECT ") {
                format!(
                    "* 450 EXISTS\r\n* OK [UIDVALIDITY 7] ok\r\n* OK [UIDNEXT 451] ok\r\n\
                     {tag} OK [READ-WRITE] SELECT completed\r\n"
                )
            } else if upper.contains("FETCH") {
                format!("{tag} OK FETCH completed\r\n")
            } else {
                format!("{tag} OK completed\r\n")
            }
        };
        let (port, handle) = scripted_imap_server(handler);
        let mut session = connect_plain(port);
        let mut next = Vec::new();
        fetch_headers_backfill(&mut session, "INBOX", 450, 10, |_, next_seq| {
            next.push(next_seq);
            Ok(())
        })
        .unwrap();
        drop(session);
        let commands = handle.join().unwrap();
        assert_eq!(next, vec![250, 50, 0]);
        let fetches = commands
            .iter()
            .filter(|line| line.contains(" FETCH "))
            .collect::<Vec<_>>();
        assert!(fetches[0].contains("FETCH 251:450"), "got {fetches:?}");
        assert!(fetches[1].contains("FETCH 51:250"), "got {fetches:?}");
        assert!(fetches[2].contains("FETCH 1:50"), "got {fetches:?}");
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

    fn uidplus_handler(line: &str, tag: &str) -> String {
        let upper = line.to_ascii_uppercase();
        if upper.contains("CAPABILITY") {
            format!("* CAPABILITY IMAP4rev1 UIDPLUS\r\n{tag} OK CAPABILITY completed\r\n")
        } else if upper.contains("UID COPY") {
            format!("{tag} OK COPY completed\r\n")
        } else if upper.contains("UID STORE") {
            format!("{tag} OK STORE completed\r\n")
        } else if upper.contains("UID EXPUNGE") {
            format!("{tag} OK EXPUNGE completed\r\n")
        } else {
            format!("{tag} OK completed\r\n")
        }
    }

    #[test]
    fn uid_sets_compress_consecutive_runs() {
        assert_eq!(format_uid_set(&[]), "");
        assert_eq!(format_uid_set(&[7]), "7");
        assert_eq!(format_uid_set(&[3, 1, 2]), "1:3");
        assert_eq!(format_uid_set(&[6, 1, 3, 4, 6]), "1,3:4,6");
    }

    #[test]
    fn moving_many_messages_sends_one_uid_set_command() {
        let (port, handle) = scripted_imap_server(uidplus_handler);
        let mut session = connect_plain(port);
        run(move_selected_many(
            &mut session,
            &[3, 1, 2],
            "Trash",
            AccountAuth::Password,
        ))
        .unwrap();
        drop(session);
        let commands = handle.join().unwrap();
        let copies = commands
            .iter()
            .filter(|line| line.contains("UID COPY"))
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), 1, "expected one COPY: {commands:?}");
        assert!(
            copies[0].contains("UID COPY 1:3 \"Trash\""),
            "expected a UID set: {commands:?}"
        );
        assert!(
            commands
                .iter()
                .any(|line| line.contains("UID STORE 1:3 +FLAGS (\\Deleted)")),
            "expected one STORE for the set: {commands:?}"
        );
        assert!(
            commands.iter().any(|line| line.contains("UID EXPUNGE 1:3")),
            "expected one UID EXPUNGE for the set: {commands:?}"
        );
    }

    #[test]
    fn deleting_many_messages_sends_one_uid_set_command() {
        let (port, handle) = scripted_imap_server(uidplus_handler);
        let mut session = connect_plain(port);
        run(delete_selected_many(
            &mut session,
            &[5, 4, 6],
            AccountAuth::Password,
        ))
        .unwrap();
        drop(session);
        let commands = handle.join().unwrap();
        assert!(
            commands
                .iter()
                .any(|line| line.contains("UID STORE 4:6 +FLAGS (\\Deleted)")),
            "expected one STORE for the set: {commands:?}"
        );
        assert_eq!(
            commands
                .iter()
                .filter(|line| line.contains("UID EXPUNGE"))
                .count(),
            1,
            "expected one UID EXPUNGE: {commands:?}"
        );
    }

    #[test]
    fn flagging_many_messages_sends_one_uid_set_command() {
        let handler = |line: &str, tag: &str| -> String {
            if line.to_ascii_uppercase().contains("UID STORE") {
                format!("{tag} OK STORE completed\r\n")
            } else {
                format!("{tag} OK completed\r\n")
            }
        };
        let (port, handle) = scripted_imap_server(handler);
        let mut session = connect_plain(port);
        run(store_flags_on(&mut session, &[2, 1, 3], "\\Seen", true)).unwrap();
        drop(session);
        let commands = handle.join().unwrap();
        let stores = commands
            .iter()
            .filter(|line| line.contains("UID STORE"))
            .collect::<Vec<_>>();
        assert_eq!(stores.len(), 1, "expected one STORE: {commands:?}");
        assert!(
            stores[0].contains("UID STORE 1:3 +FLAGS (\\Seen)"),
            "expected a UID set: {commands:?}"
        );
    }

    #[test]
    fn proton_move_of_many_messages_uses_a_uid_set() {
        assert_eq!(
            proton_move_command_set("1:3,7", "Archive"),
            "UID MOVE 1:3,7 \"Archive\""
        );

        let handler = |line: &str, tag: &str| -> String {
            if line.to_ascii_uppercase().contains("UID MOVE") {
                format!("{tag} OK MOVE completed\r\n")
            } else {
                format!("{tag} OK completed\r\n")
            }
        };
        let (port, handle) = scripted_imap_server(handler);
        let mut session = connect_plain(port);
        run(move_selected_many(
            &mut session,
            &[8, 7, 9],
            "Archive",
            AccountAuth::Proton,
        ))
        .unwrap();
        drop(session);
        let commands = handle.join().unwrap();
        let moves = commands
            .iter()
            .filter(|line| line.contains("UID MOVE"))
            .collect::<Vec<_>>();
        assert_eq!(moves.len(), 1, "expected one MOVE: {commands:?}");
        assert!(
            moves[0].contains("UID MOVE 7:9 \"Archive\""),
            "expected a UID set: {commands:?}"
        );
    }
}
