use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::accounts::{self, Account, AccountAuth, ProtonAccountInput, SignatureFields};
use crate::error::{AppError, AppResult};
use crate::imap::client::validate_proton_imap;
use crate::mail_auth::MailCredential;
use crate::secrets::account_key;
use crate::smtp::validate_proton_smtp;

use super::bridge::ProtonBridge;
use super::ports;

const EVENT_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const IDLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const EXIT_TIMEOUT: Duration = Duration::from_secs(2);
const ACCESS_LIMITED_MESSAGE: &str =
    "Proton temporarily limited API access for this account. Use the official Proton app and try again later.";
const APPEAL_URL: &str = "https://proton.me/support/appeal-abuse";

#[derive(Clone, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ProtonAuthStart {
    Add {
        space_id: i64,
        username: String,
        password: String,
    },
    Reconnect {
        account_id: i64,
        username: String,
        password: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum ProtonAuthResponse {
    Totp(String),
    MailboxPassword(String),
    VerificationDone,
    CaptchaToken(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[allow(clippy::large_enum_variant)]
pub enum ProtonAuthStatus {
    NeedsTotp {
        login_id: String,
        error: Option<String>,
    },
    NeedsMailboxPassword {
        login_id: String,
        error: Option<String>,
    },
    NeedsVerification {
        login_id: String,
        url: String,
        methods: Vec<String>,
        error: Option<String>,
    },
    AccessLimited {
        message: String,
        appeal_url: String,
    },
    Complete {
        account: Account,
    },
}

pub(crate) type AuthValidator =
    Arc<dyn Fn(&Account, &MailCredential) -> AppResult<()> + Send + Sync>;
type AuthSpawn = (Child, ChildStdin, Receiver<String>, Arc<Mutex<String>>);

pub struct ProtonAuthSessions {
    state: Mutex<Option<Arc<Session>>>,
    active: AtomicBool,
    validator: Mutex<AuthValidator>,
    idle_timeout_ms: AtomicU64,
}

impl ProtonAuthSessions {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(None),
            active: AtomicBool::new(false),
            validator: Mutex::new(Arc::new(|account, credential| {
                validate_proton_imap(
                    &account.imap_host,
                    account.imap_port,
                    &account.username,
                    credential,
                )?;
                validate_proton_smtp(
                    &account.smtp_host,
                    account.smtp_port,
                    &account.username,
                    credential,
                )
            })),
            idle_timeout_ms: AtomicU64::new(IDLE_TIMEOUT.as_millis() as u64),
        }
    }

    pub(crate) fn in_progress(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    fn begin(&self) -> AppResult<()> {
        if self
            .active
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(AppError::Message("Proton sign-in in progress".into()));
        }
        Ok(())
    }

    fn end(&self) {
        self.active.store(false, Ordering::SeqCst);
    }

    fn schedule_idle_timeout(self: &Arc<Self>, session: &Arc<Session>) {
        let weak = Arc::downgrade(self);
        let session = Arc::clone(session);
        let activity = session.activity.load(Ordering::SeqCst);
        let timeout = Duration::from_millis(self.idle_timeout_ms.load(Ordering::SeqCst));
        thread::spawn(move || {
            thread::sleep(timeout);
            let Some(auth) = weak.upgrade() else {
                return;
            };
            if session.activity.load(Ordering::SeqCst) != activity {
                return;
            }
            let mut state = lock(&auth.state);
            let owned = state
                .as_ref()
                .map(|current| Arc::ptr_eq(current, &session))
                .unwrap_or(false);
            if !owned {
                return;
            }
            state.take();
            drop(state);
            session.kill();
            let _ = restore_auth_snapshot(&session.auth_file, &session.auth_snapshot);
            session.release_lock();
            auth.end();
        });
    }

    pub(crate) fn validate(&self, account: &Account, credential: &MailCredential) -> AppResult<()> {
        let validator = lock(&self.validator).clone();
        validator(account, credential)
    }

    #[cfg(test)]
    pub(crate) fn set_validator(&self, validator: AuthValidator) {
        *lock(&self.validator) = validator;
    }

    #[cfg(test)]
    pub(crate) fn set_idle_timeout(&self, timeout: Duration) {
        self.idle_timeout_ms
            .store(timeout.as_millis() as u64, Ordering::SeqCst);
    }
}

impl Default for ProtonAuthSessions {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
enum Target {
    Add { space_id: i64 },
    Reconnect { account_id: i64 },
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone)]
enum PendingStep {
    Totp,
    MailboxPassword,
    Verification { url: String, methods: Vec<String> },
}

struct Session {
    login_id: String,
    target: Target,
    username: String,
    process: Mutex<ChildProcess>,
    events: Mutex<Receiver<String>>,
    stderr: Arc<Mutex<String>>,
    lock: File,
    bridge_was_running: bool,
    pending: Mutex<Option<PendingStep>>,
    activity: AtomicU64,
    continuing: AtomicBool,
    auth_file: PathBuf,
    auth_snapshot: Option<Vec<u8>>,
}

impl Session {
    fn touch(&self) {
        self.activity.fetch_add(1, Ordering::SeqCst);
    }

    fn send<T: Serialize>(&self, value: &T) -> AppResult<()> {
        let line = serde_json::to_vec(value)?;
        let mut process = lock(&self.process);
        let stdin = process.stdin.as_mut().ok_or_else(|| {
            AppError::Message("Proton sign-in ended before it was complete.".into())
        })?;
        stdin.write_all(&line)?;
        stdin.write_all(b"\n")?;
        stdin.flush()?;
        self.touch();
        Ok(())
    }

    fn finish(&self) {
        let mut process = lock(&self.process);
        process.stdin.take();
        if let Some(mut child) = process.child.take() {
            wait_for_exit(&mut child);
        }
    }

    fn kill(&self) {
        let mut process = lock(&self.process);
        process.stdin.take();
        if let Some(mut child) = process.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn release_lock(&self) {
        let _ = FileExt::unlock(&self.lock);
    }

    fn stderr(&self) -> String {
        lock(&self.stderr).clone()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.kill();
        let _ = FileExt::unlock(&self.lock);
    }
}

struct ChildProcess {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
}

struct ContinueGuard(Arc<Session>);

impl Drop for ContinueGuard {
    fn drop(&mut self) {
        self.0.continuing.store(false, Ordering::SeqCst);
    }
}

struct SidecarEvent {
    event: String,
    url: String,
    methods: Vec<String>,
    message: String,
    retryable: bool,
    appeal_url: String,
    login_username: String,
    email: String,
    display_name: String,
    bridge_password: String,
}

#[derive(Deserialize)]
struct RawSidecarEvent {
    event: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    methods: Vec<String>,
    #[serde(default)]
    message: String,
    #[serde(default)]
    retryable: bool,
    #[serde(default)]
    appeal_url: String,
    #[serde(default)]
    login_username: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    bridge_password: String,
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    op: &'static str,
    username: &'a str,
    password: &'a str,
}

#[derive(Serialize)]
struct ResponseLine {
    #[serde(skip_serializing_if = "Option::is_none")]
    totp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mailbox_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    captcha_token: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    verification_done: bool,
}

impl ResponseLine {
    fn from_response(response: ProtonAuthResponse) -> Self {
        match response {
            ProtonAuthResponse::Totp(value) => Self {
                totp: Some(value),
                mailbox_password: None,
                captcha_token: None,
                verification_done: false,
            },
            ProtonAuthResponse::MailboxPassword(value) => Self {
                totp: None,
                mailbox_password: Some(value),
                captcha_token: None,
                verification_done: false,
            },
            ProtonAuthResponse::VerificationDone => Self {
                totp: None,
                mailbox_password: None,
                captcha_token: None,
                verification_done: true,
            },
            ProtonAuthResponse::CaptchaToken(value) => Self {
                totp: None,
                mailbox_password: None,
                captcha_token: Some(value),
                verification_done: false,
            },
        }
    }
}

#[derive(Default)]
struct Rollback {
    created_account: Option<i64>,
}

impl ProtonBridge {
    pub fn start_login(&self, request: ProtonAuthStart) -> AppResult<ProtonAuthStatus> {
        let (target, username, password) = match request {
            ProtonAuthStart::Add {
                space_id,
                username,
                password,
            } => (Target::Add { space_id }, username, password),
            ProtonAuthStart::Reconnect {
                account_id,
                username,
                password,
            } => (Target::Reconnect { account_id }, username, password),
        };

        let username = username.trim().to_string();
        if username.is_empty() {
            return Err(AppError::Message("username is required".into()));
        }
        if password.is_empty() {
            return Err(AppError::Message("password is required".into()));
        }

        self.auth.begin()?;
        let result = (|| {
            if let Target::Add { space_id } = target {
                crate::spaces::by_id(self.db.as_ref(), space_id)?;
            }
            if let Target::Reconnect { account_id } = target {
                let account = account_by_id(self, account_id)?;
                if account.auth_type != AccountAuth::Proton {
                    return Err(AppError::Message("account not found".into()));
                }
                if !account.username.eq_ignore_ascii_case(&username) {
                    return Err(AppError::Message(
                        "Reconnect must use the account's Proton login username.".into(),
                    ));
                }
            }
            self.start_login_inner(target, username, password)
        })();

        if result.is_err() {
            self.auth.end();
        }
        result
    }

    pub fn continue_login(
        &self,
        login_id: &str,
        response: ProtonAuthResponse,
    ) -> AppResult<ProtonAuthStatus> {
        let session = {
            let state = lock(&self.auth.state);
            match state.as_ref() {
                Some(session) if session.login_id == login_id => Arc::clone(session),
                _ => {
                    return Err(AppError::Message(
                        "Proton sign-in is not in progress.".into(),
                    ))
                }
            }
        };
        if session
            .continuing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(AppError::Message("Proton sign-in is busy.".into()));
        }
        let _guard = ContinueGuard(Arc::clone(&session));
        session.touch();

        let line = ResponseLine::from_response(response);
        let event = session.send(&line).and_then(|_| next_event(&session));

        if !session_active(&self.auth.state, &session) {
            return Err(cancelled());
        }

        match event {
            Ok(event) => self.handle_event(session, event),
            Err(error) => {
                if !take_session(&self.auth.state, &session) {
                    return Err(cancelled());
                }
                self.fail_session(session, error)
            }
        }
    }

    pub fn cancel_login(&self, login_id: &str) -> AppResult<()> {
        self.cancel_login_optional(Some(login_id))
    }

    pub fn cancel_active_login(&self) -> AppResult<()> {
        self.cancel_login_optional(None)
    }

    fn cancel_login_optional(&self, login_id: Option<&str>) -> AppResult<()> {
        let session = {
            let mut state = lock(&self.auth.state);
            match state.as_ref() {
                Some(session) if login_id.map(|id| session.login_id == id).unwrap_or(true) => {
                    state.take()
                }
                _ => None,
            }
        };
        let Some(session) = session else {
            return Err(AppError::Message(
                "Proton sign-in is not in progress.".into(),
            ));
        };

        session.kill();
        self.stop();
        let _ = restore_auth_snapshot(&session.auth_file, &session.auth_snapshot);
        self.restore_bridge(session.bridge_was_running, true);
        session.release_lock();
        self.auth.end();
        Ok(())
    }

    pub(crate) fn shutdown_auth(&self) {
        self.stop();
        let session = lock(&self.auth.state).take();
        let Some(session) = session else {
            self.auth.end();
            return;
        };
        session.kill();
        let _ = restore_auth_snapshot(&session.auth_file, &session.auth_snapshot);
        session.release_lock();
        self.auth.end();
    }

    fn start_login_inner(
        &self,
        target: Target,
        username: String,
        password: String,
    ) -> AppResult<ProtonAuthStatus> {
        let bridge_was_running = self.is_running();
        if bridge_was_running {
            self.stop();
        }

        let lock = match acquire_auth_lock(&self.config_dir) {
            Ok(lock) => lock,
            Err(error) => {
                self.restore_bridge(bridge_was_running, false);
                return Err(error);
            }
        };

        let auth_file = self.config_dir.join("hydroxide/auth.json");
        let auth_snapshot = match read_auth_snapshot(&auth_file) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.restore_bridge(bridge_was_running, true);
                let _ = FileExt::unlock(&lock);
                return Err(error);
            }
        };

        let (child, stdin, events, stderr) = match self.spawn_auth() {
            Ok(spawned) => spawned,
            Err(error) => {
                self.restore_bridge(bridge_was_running, true);
                let _ = FileExt::unlock(&lock);
                return Err(error);
            }
        };

        let login_id = new_login_id();
        let session = Arc::new(Session {
            login_id: login_id.clone(),
            target,
            username: username.clone(),
            process: Mutex::new(ChildProcess {
                child: Some(child),
                stdin: Some(stdin),
            }),
            events: Mutex::new(events),
            stderr,
            lock,
            bridge_was_running,
            pending: Mutex::new(None),
            activity: AtomicU64::new(0),
            continuing: AtomicBool::new(false),
            auth_file,
            auth_snapshot,
        });

        if let Err(error) = session.send(&LoginRequest {
            op: "login",
            username: &username,
            password: &password,
        }) {
            return self.fail_session(session, error);
        }
        if let Err(error) = self.publish_session(&session) {
            return self.fail_session(session, error);
        }

        match next_event(&session) {
            Ok(event) => self.handle_event(session, event),
            Err(error) => {
                if !take_session(&self.auth.state, &session) {
                    return Err(cancelled());
                }
                self.fail_session(session, error)
            }
        }
    }

    fn publish_session(&self, session: &Arc<Session>) -> AppResult<()> {
        let mut state = lock(&self.auth.state);
        if state.is_some() {
            return Err(AppError::Message("Proton sign-in in progress".into()));
        }
        *state = Some(Arc::clone(session));
        Ok(())
    }

    fn handle_event(
        &self,
        session: Arc<Session>,
        event: SidecarEvent,
    ) -> AppResult<ProtonAuthStatus> {
        match event.event.as_str() {
            "totp_required" => {
                self.hold_pending(&session, PendingStep::Totp)?;
                self.auth.schedule_idle_timeout(&session);
                Ok(ProtonAuthStatus::NeedsTotp {
                    login_id: session.login_id.clone(),
                    error: None,
                })
            }
            "mailbox_password_required" => {
                self.hold_pending(&session, PendingStep::MailboxPassword)?;
                self.auth.schedule_idle_timeout(&session);
                Ok(ProtonAuthStatus::NeedsMailboxPassword {
                    login_id: session.login_id.clone(),
                    error: None,
                })
            }
            "captcha_required" => {
                let pending = PendingStep::Verification {
                    url: event.url.clone(),
                    methods: event.methods.clone(),
                };
                self.hold_pending(&session, pending.clone())?;
                self.auth.schedule_idle_timeout(&session);
                let PendingStep::Verification { url, methods } = pending else {
                    unreachable!()
                };
                Ok(ProtonAuthStatus::NeedsVerification {
                    login_id: session.login_id.clone(),
                    url,
                    methods,
                    error: None,
                })
            }
            "error" => {
                let message = safe_message(&event.message);
                if event.retryable {
                    let pending = {
                        let pending = session
                            .pending
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .clone();
                        pending
                    };
                    let pending = match pending {
                        Some(pending) => pending,
                        None => {
                            if !take_session(&self.auth.state, &session) {
                                return Err(cancelled());
                            }
                            return self.fail_session(session, AppError::Message(message));
                        }
                    };
                    self.hold_pending(&session, pending.clone())?;
                    self.auth.schedule_idle_timeout(&session);
                    Ok(status_for_pending(&session, pending, Some(message)))
                } else {
                    if !take_session(&self.auth.state, &session) {
                        return Err(cancelled());
                    }
                    self.fail_session(session, AppError::Message(message))
                }
            }
            "access_limited" => {
                let message = if event.message.trim().is_empty() {
                    ACCESS_LIMITED_MESSAGE.to_string()
                } else {
                    event.message.trim().to_string()
                };
                let appeal_url = if event.appeal_url.trim().is_empty() {
                    APPEAL_URL.to_string()
                } else {
                    event.appeal_url.trim().to_string()
                };
                if !take_session(&self.auth.state, &session) {
                    return Err(cancelled());
                }
                session.kill();
                self.stop();
                let _ = restore_auth_snapshot(&session.auth_file, &session.auth_snapshot);
                self.restore_bridge(session.bridge_was_running, true);
                session.release_lock();
                self.auth.end();
                Ok(ProtonAuthStatus::AccessLimited {
                    message,
                    appeal_url,
                })
            }
            "success" => {
                if !take_session(&self.auth.state, &session) {
                    return Err(cancelled());
                }
                self.finish_success(session, event)
            }
            _ => {
                if !take_session(&self.auth.state, &session) {
                    return Err(cancelled());
                }
                self.fail_session(
                    session,
                    AppError::Message(
                        "Proton sign-in failed. Check your connection and try again.".into(),
                    ),
                )
            }
        }
    }

    fn hold_pending(&self, session: &Arc<Session>, pending: PendingStep) -> AppResult<()> {
        let state = lock(&self.auth.state);
        if !state
            .as_ref()
            .map(|current| Arc::ptr_eq(current, session))
            .unwrap_or(false)
        {
            return Err(cancelled());
        }
        *session
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(pending);
        Ok(())
    }

    fn finish_success(
        &self,
        session: Arc<Session>,
        event: SidecarEvent,
    ) -> AppResult<ProtonAuthStatus> {
        session.finish();
        let mut rollback = Rollback::default();
        let result = self.apply_success(&session, &event, &mut rollback);
        match result {
            Ok(account) => {
                session.release_lock();
                self.auth.end();
                Ok(ProtonAuthStatus::Complete { account })
            }
            Err(error) => {
                if let Some(account_id) = rollback.created_account {
                    let username = account_by_id(self, account_id)
                        .ok()
                        .map(|account| account.username);
                    let _ = accounts::delete(self.db.as_ref(), self.secrets.as_ref(), account_id);
                    if let Some(username) = username {
                        // The session still owns the auth lock here, so remove
                        // the entry directly. The later snapshot restore keeps
                        // its existing rollback semantics.
                        let _ = super::bridge::remove_auth_entry(&session.auth_file, &username);
                    }
                }
                self.fail_session(session, error)
            }
        }
    }

    fn apply_success(
        &self,
        session: &Session,
        event: &SidecarEvent,
        rollback: &mut Rollback,
    ) -> AppResult<Account> {
        let login_username = event.login_username.trim();
        let bridge_password = event.bridge_password.trim();
        if login_username.is_empty() || bridge_password.is_empty() {
            return Err(AppError::Message(
                "Proton did not return a usable bridge account.".into(),
            ));
        }

        match session.target {
            Target::Add { space_id } => {
                let email = if event.email.trim().is_empty() {
                    login_username.to_string()
                } else {
                    event.email.trim().to_string()
                };
                let temporary = ports::free_pair()?;
                let created = accounts::create_proton(
                    self.db.as_ref(),
                    self.secrets.as_ref(),
                    ProtonAccountInput {
                        display_name: event.display_name.trim().to_string(),
                        email,
                        username: login_username.to_string(),
                        bridge_password: bridge_password.to_string(),
                        imap_host: "127.0.0.1".into(),
                        imap_port: temporary.imap,
                        smtp_host: "127.0.0.1".into(),
                        smtp_port: temporary.smtp,
                        color: "lavender".into(),
                        signature: String::new(),
                        signature_fields: SignatureFields::default(),
                        space_id,
                    },
                )?;
                rollback.created_account = Some(created.id);

                if self.start_if_needed_for_auth()?.is_none() {
                    return Err(AppError::Message(
                        "Proton bridge did not start for the new account.".into(),
                    ));
                }
                let account = account_by_id(self, created.id)?;
                self.auth.validate(
                    &account,
                    &MailCredential::Password(bridge_password.to_string()),
                )?;
                Ok(account)
            }
            Target::Reconnect { account_id } => {
                let account = account_by_id(self, account_id)?;
                if account.auth_type != AccountAuth::Proton
                    || !account.username.eq_ignore_ascii_case(&session.username)
                    || !login_username.eq_ignore_ascii_case(&session.username)
                {
                    return Err(AppError::Message(
                        "Proton signed in to a different account.".into(),
                    ));
                }

                self.restart_for_auth()?;
                let account = account_by_id(self, account_id)?;
                self.auth.validate(
                    &account,
                    &MailCredential::Password(bridge_password.to_string()),
                )?;
                self.secrets
                    .set(&account_key(account_id), bridge_password)?;

                let account = account_by_id(self, account_id)?;
                if should_refresh_display_name(&account, &event.display_name) {
                    return accounts::update_proton_profile(
                        self.db.as_ref(),
                        account_id,
                        event.display_name.trim(),
                        &account.color,
                        &account.signature,
                        &account.signature_fields,
                        account.signature_on_new,
                        account.space_id,
                    );
                }
                Ok(account)
            }
        }
    }

    fn fail_session(&self, session: Arc<Session>, error: AppError) -> AppResult<ProtonAuthStatus> {
        session.kill();
        self.stop();
        let _ = restore_auth_snapshot(&session.auth_file, &session.auth_snapshot);
        self.restore_bridge(session.bridge_was_running, true);
        session.release_lock();
        self.auth.end();
        Err(error)
    }

    fn restore_bridge(&self, was_running: bool, lock_held: bool) {
        if was_running {
            let _ = if lock_held {
                self.restart_for_auth()
            } else {
                self.restart()
            };
        } else {
            self.stop();
        }
    }

    fn spawn_auth(&self) -> AppResult<AuthSpawn> {
        let mut command = Command::new(&self.sidecar);
        command
            .arg("auth")
            .env("XDG_CONFIG_HOME", &self.config_dir)
            .env("APPDATA", &self.config_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn().map_err(|_| {
            AppError::Message("Proton support is unavailable in this build.".into())
        })?;

        let stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::Message("Proton sign-in could not start.".into()));
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::Message("Proton sign-in could not start.".into()));
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::Message("Proton sign-in could not start.".into()));
            }
        };

        Ok((child, stdin, read_events(stdout), capture_stderr(stderr)))
    }
}

fn account_by_id(bridge: &ProtonBridge, id: i64) -> AppResult<Account> {
    accounts::list(bridge.db.as_ref())?
        .into_iter()
        .find(|account| account.id == id)
        .ok_or_else(|| AppError::Message("account not found".into()))
}

fn should_refresh_display_name(account: &Account, candidate: &str) -> bool {
    let candidate = candidate.trim();
    !candidate.is_empty()
        && (account.display_name.trim().is_empty()
            || account
                .display_name
                .trim()
                .eq_ignore_ascii_case(account.email.trim()))
}

fn status_for_pending(
    session: &Session,
    pending: PendingStep,
    error: Option<String>,
) -> ProtonAuthStatus {
    match pending {
        PendingStep::Totp => ProtonAuthStatus::NeedsTotp {
            login_id: session.login_id.clone(),
            error,
        },
        PendingStep::MailboxPassword => ProtonAuthStatus::NeedsMailboxPassword {
            login_id: session.login_id.clone(),
            error,
        },
        PendingStep::Verification { url, methods } => ProtonAuthStatus::NeedsVerification {
            login_id: session.login_id.clone(),
            url,
            methods,
            error,
        },
    }
}

fn session_active(state: &Mutex<Option<Arc<Session>>>, session: &Arc<Session>) -> bool {
    lock(state)
        .as_ref()
        .map(|current| Arc::ptr_eq(current, session))
        .unwrap_or(false)
}

fn take_session(state: &Mutex<Option<Arc<Session>>>, session: &Arc<Session>) -> bool {
    let mut state = lock(state);
    if state
        .as_ref()
        .map(|current| Arc::ptr_eq(current, session))
        .unwrap_or(false)
    {
        state.take();
        true
    } else {
        false
    }
}

fn safe_message(message: &str) -> String {
    let message = message.trim();
    if message.is_empty() {
        "Proton sign-in failed. Check your connection and try again.".into()
    } else {
        message.chars().take(500).collect()
    }
}

fn cancelled() -> AppError {
    AppError::Message("Proton sign-in was cancelled.".into())
}

fn new_login_id() -> String {
    use std::sync::atomic::AtomicU64;
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        millis,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn read_auth_snapshot(path: &Path) -> AppResult<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::Io(error)),
    }
}

fn restore_auth_snapshot(path: &Path, snapshot: &Option<Vec<u8>>) -> AppResult<()> {
    match snapshot {
        Some(contents) => {
            let temporary = path.with_extension(format!("json.tmp.{}", std::process::id()));
            fs::write(&temporary, contents)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
            }
            fs::rename(&temporary, path)?;
            Ok(())
        }
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(AppError::Io(error)),
        },
    }
}

fn acquire_auth_lock(config_dir: &Path) -> AppResult<File> {
    let dir = config_dir.join("hydroxide");
    ensure_private_dir(&dir)?;
    let path = dir.join("auth.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.try_lock_exclusive().map_err(|error| {
        if error.kind() == ErrorKind::WouldBlock {
            AppError::Message("Proton sign-in in progress".into())
        } else {
            AppError::Io(error)
        }
    })?;
    Ok(file)
}

fn ensure_private_dir(dir: &Path) -> AppResult<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn read_events(stdout: ChildStdout) -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            let _ = sender.send(line);
        }
    });
    receiver
}

fn capture_stderr(stderr: std::process::ChildStderr) -> Arc<Mutex<String>> {
    let text = Arc::new(Mutex::new(String::new()));
    let target = Arc::clone(&text);
    thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else {
                break;
            };
            let mut buffer = lock(&target);
            if buffer.len() < 4096 {
                buffer.push_str(&line);
                buffer.push('\n');
            }
        }
    });
    text
}

fn next_event(session: &Session) -> AppResult<SidecarEvent> {
    loop {
        let line = {
            let receiver = lock(&session.events);
            match receiver.recv_timeout(EVENT_TIMEOUT) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => {
                    return Err(AppError::Message(
                        "Proton sign-in timed out. Try again.".into(),
                    ))
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let stderr = session.stderr();
                    if stderr
                        .to_ascii_lowercase()
                        .contains("auth lock is not held")
                    {
                        return Err(AppError::Message("Proton sign-in could not start.".into()));
                    }
                    return Err(AppError::Message(
                        "Proton sign-in ended before it was complete.".into(),
                    ));
                }
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let event: RawSidecarEvent = serde_json::from_str(&line).map_err(|_| {
            AppError::Message("Proton sign-in returned an unexpected response.".into())
        })?;
        return Ok(SidecarEvent {
            event: event.event,
            url: event.url,
            methods: event.methods,
            message: event.message,
            retryable: event.retryable,
            appeal_url: event.appeal_url,
            login_username: event.login_username,
            email: event.email,
            display_name: event.display_name,
            bridge_password: event.bridge_password,
        });
    }
}

fn wait_for_exit(child: &mut Child) {
    let deadline = Instant::now() + EXIT_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            _ => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    use fs2::FileExt;
    use tempfile::TempDir;

    use super::*;
    use crate::accounts::{self, AccountAuth, ProtonAccountInput, SignatureFields};
    use crate::db::Database;
    use crate::proton::bridge::ProtonEvents;
    use crate::proton::{ProtonAccessLimited, ProtonBridge};
    use crate::secrets::{account_key, MemoryStore, SecretStore};

    #[derive(Default)]
    struct FakeEvents;

    impl ProtonEvents for FakeEvents {
        fn access_limited(&self, _status: ProtonAccessLimited) {}
    }

    struct Fixture {
        dir: TempDir,
        db: Arc<Database>,
        secrets: Arc<MemoryStore>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                dir: tempfile::tempdir().unwrap(),
                db: Arc::new(Database::open_memory().unwrap()),
                secrets: Arc::new(MemoryStore::default()),
            }
        }

        fn config_dir(&self) -> PathBuf {
            self.dir.path().join("proton-bridge")
        }

        fn bridge(&self) -> ProtonBridge {
            let bridge = ProtonBridge::new(
                Arc::new(FakeEvents),
                Arc::clone(&self.db),
                Arc::clone(&self.secrets) as Arc<dyn SecretStore>,
                self.config_dir(),
                fake_sidecar(),
            );
            bridge.auth.set_validator(Arc::new(|_, _| Ok(())));
            bridge
        }

        fn bridge_with_validator(&self, validator: AuthValidator) -> ProtonBridge {
            let bridge = self.bridge();
            bridge.auth.set_validator(validator);
            bridge
        }

        fn add_proton(&self, email: &str, username: &str, bridge_password: &str) -> Account {
            let ports = ports::free_pair().unwrap();
            accounts::create_proton(
                self.db.as_ref(),
                self.secrets.as_ref(),
                ProtonAccountInput {
                    display_name: "Proton".into(),
                    email: email.into(),
                    username: username.into(),
                    bridge_password: bridge_password.into(),
                    imap_host: "127.0.0.1".into(),
                    imap_port: ports.imap,
                    smtp_host: "127.0.0.1".into(),
                    smtp_port: ports.smtp,
                    color: "lavender".into(),
                    signature: String::new(),
                    signature_fields: SignatureFields::default(),
                    space_id: 1,
                },
            )
            .unwrap()
        }

        fn generation(&self) -> Option<String> {
            fs::read_to_string(self.config_dir().join("sidecar-generation")).ok()
        }

        fn auth_started(&self) -> bool {
            self.config_dir().join("auth-started").exists()
        }

        fn stopped(&self) -> bool {
            self.config_dir().join("stopped").exists()
        }

        fn auth_pid(&self) -> Option<u32> {
            fs::read_to_string(self.config_dir().join("auth-pid"))
                .ok()
                .and_then(|value| value.trim().parse().ok())
        }
    }

    fn fake_sidecar() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".local-tests/proton-bridge/fake-auth-sidecar.sh")
    }

    fn adding(username: &str) -> ProtonAuthStart {
        ProtonAuthStart::Add {
            space_id: 1,
            username: username.into(),
            password: "proton-password".into(),
        }
    }

    #[test]
    fn serializes_statuses_with_camel_case_keys() {
        let totp = serde_json::to_value(ProtonAuthStatus::NeedsTotp {
            login_id: "login-1".into(),
            error: None,
        })
        .unwrap();
        assert_eq!(
            totp,
            serde_json::json!({
                "status": "needsTotp",
                "loginId": "login-1",
                "error": null
            })
        );

        let limited = serde_json::to_value(ProtonAuthStatus::AccessLimited {
            message: "Access limited".into(),
            appeal_url: "https://proton.me/support/appeal-abuse".into(),
        })
        .unwrap();
        assert_eq!(
            limited,
            serde_json::json!({
                "status": "accessLimited",
                "message": "Access limited",
                "appealUrl": "https://proton.me/support/appeal-abuse"
            })
        );
    }

    #[test]
    fn parses_add_and_reconnect_requests_with_camel_case_fields() {
        let add: ProtonAuthStart = serde_json::from_str(
            r#"{"mode":"add","spaceId":4,"username":"me@proton.me","password":"secret"}"#,
        )
        .unwrap();
        let ProtonAuthStart::Add {
            space_id,
            username,
            password,
        } = add
        else {
            panic!("expected add");
        };
        assert_eq!(space_id, 4);
        assert_eq!(username, "me@proton.me");
        assert_eq!(password, "secret");

        let reconnect: ProtonAuthStart = serde_json::from_str(
            r#"{"mode":"reconnect","accountId":7,"username":"me@proton.me","password":"secret"}"#,
        )
        .unwrap();
        let ProtonAuthStart::Reconnect {
            account_id,
            username,
            password,
        } = reconnect
        else {
            panic!("expected reconnect");
        };
        assert_eq!(account_id, 7);
        assert_eq!(username, "me@proton.me");
        assert_eq!(password, "secret");
    }

    fn account_count(fixture: &Fixture) -> usize {
        accounts::list(fixture.db.as_ref()).unwrap().len()
    }

    fn lock_is_free(fixture: &Fixture) -> bool {
        let path = fixture.config_dir().join("hydroxide/auth.lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .unwrap();
        let free = file.try_lock_exclusive().is_ok();
        if free {
            let _ = FileExt::unlock(&file);
        }
        free
    }

    fn wait_for_process_exit(pid: u32) {
        let deadline = Instant::now() + Duration::from_secs(3);
        let path = PathBuf::from(format!("/proc/{pid}"));
        while path.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!path.exists(), "auth child {pid} is still running");
    }

    #[test]
    fn totp_login_creates_the_account_and_starts_the_bridge() {
        let fixture = Fixture::new();
        let bridge = fixture.bridge();

        let status = bridge.start_login(adding("totp@proton.me")).unwrap();
        let ProtonAuthStatus::NeedsTotp { login_id, error } = status else {
            panic!("expected TOTP");
        };
        assert!(error.is_none());
        assert!(!bridge.is_running());
        assert!(fixture.auth_started());
        assert!(!lock_is_free(&fixture));

        let status = bridge
            .continue_login(&login_id, ProtonAuthResponse::Totp("123456".into()))
            .unwrap();
        let ProtonAuthStatus::Complete { account } = status else {
            panic!("expected completion");
        };
        assert_eq!(account.email, "totp@proton.me");
        assert_eq!(account.username, "totp@proton.me");
        assert_eq!(account.auth_type, AccountAuth::Proton);
        assert_ne!(account.imap_port, 0);
        assert_ne!(account.smtp_port, 0);
        assert!(bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("1"));
        assert!(lock_is_free(&fixture));
        assert_eq!(
            fixture.secrets.get(&account_key(account.id)).unwrap(),
            "bridge-password-for-totp@proton.me"
        );
    }

    #[test]
    fn mailbox_password_login_completes() {
        let fixture = Fixture::new();
        let bridge = fixture.bridge();

        let status = bridge.start_login(adding("mailbox@proton.me")).unwrap();
        let ProtonAuthStatus::NeedsMailboxPassword { login_id, error } = status else {
            panic!("expected mailbox password");
        };
        assert!(error.is_none());

        let status = bridge
            .continue_login(
                &login_id,
                ProtonAuthResponse::MailboxPassword("mailbox-password".into()),
            )
            .unwrap();
        let ProtonAuthStatus::Complete { account } = status else {
            panic!("expected completion");
        };
        assert_eq!(account.username, "mailbox@proton.me");
        assert!(bridge.is_running());
    }

    #[test]
    fn captcha_login_completes_after_verification() {
        let fixture = Fixture::new();
        let bridge = fixture.bridge();

        let status = bridge.start_login(adding("captcha@proton.me")).unwrap();
        let ProtonAuthStatus::NeedsVerification {
            login_id,
            url,
            methods,
            error,
        } = status
        else {
            panic!("expected verification");
        };
        assert!(url.starts_with("https://verify.proton.me/"));
        assert_eq!(methods, vec!["captcha".to_string()]);
        assert!(error.is_none());

        let status = bridge
            .continue_login(&login_id, ProtonAuthResponse::VerificationDone)
            .unwrap();
        let ProtonAuthStatus::Complete { account } = status else {
            panic!("expected completion");
        };
        assert_eq!(account.username, "captcha@proton.me");
    }

    #[test]
    fn retryable_error_keeps_the_totp_step() {
        let fixture = Fixture::new();
        let bridge = fixture.bridge();

        let status = bridge.start_login(adding("retry@proton.me")).unwrap();
        let ProtonAuthStatus::NeedsTotp { login_id, .. } = status else {
            panic!("expected TOTP");
        };

        let status = bridge
            .continue_login(&login_id, ProtonAuthResponse::Totp("000000".into()))
            .unwrap();
        let ProtonAuthStatus::NeedsTotp {
            login_id: same_id,
            error: Some(error),
        } = status
        else {
            panic!("expected a retryable TOTP error");
        };
        assert_eq!(same_id, login_id);
        assert_eq!(
            error,
            "That verification code was not accepted. Try the current code."
        );

        let status = bridge
            .continue_login(&login_id, ProtonAuthResponse::Totp("123456".into()))
            .unwrap();
        assert!(matches!(status, ProtonAuthStatus::Complete { .. }));
    }

    #[test]
    fn access_limited_is_terminal_and_does_not_retry() {
        let fixture = Fixture::new();
        let bridge = fixture.bridge();

        let status = bridge.start_login(adding("limited@proton.me")).unwrap();
        let ProtonAuthStatus::AccessLimited {
            message,
            appeal_url,
        } = status
        else {
            panic!("expected access limited");
        };
        assert!(message.contains("limited"));
        assert_eq!(appeal_url, "https://proton.me/support/appeal-abuse");
        assert!(!bridge.is_running());
        assert!(lock_is_free(&fixture));
        assert_eq!(account_count(&fixture), 0);
        assert!(bridge
            .continue_login("missing", ProtonAuthResponse::Totp("1".into()))
            .is_err());
    }

    #[test]
    fn terminal_error_does_not_create_an_account() {
        let fixture = Fixture::new();
        let bridge = fixture.bridge();

        let error = bridge.start_login(adding("deny@proton.me")).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Proton rejected the email address or password."
        );
        assert_eq!(account_count(&fixture), 0);
        assert!(lock_is_free(&fixture));
    }

    #[test]
    fn cancel_kills_the_child_and_restarts_the_bridge() {
        let fixture = Fixture::new();
        fixture.add_proton("cancel@proton.me", "cancel@proton.me", "old-secret");
        let bridge = fixture.bridge();
        bridge.start_if_needed().unwrap().unwrap();
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        let status = bridge.start_login(adding("cancel@proton.me")).unwrap();
        let ProtonAuthStatus::NeedsTotp { login_id, .. } = status else {
            panic!("expected TOTP");
        };
        let pid = fixture.auth_pid().unwrap();
        assert!(fixture.stopped());
        assert!(!bridge.is_running());

        bridge.cancel_login(&login_id).unwrap();

        wait_for_process_exit(pid);
        assert!(bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("2"));
        assert!(lock_is_free(&fixture));
        assert!(bridge
            .continue_login(&login_id, ProtonAuthResponse::Totp("1".into()))
            .is_err());
        assert!(bridge.cancel_login(&login_id).is_err());
    }

    #[test]
    fn failed_validation_rolls_back_the_account_keyring_and_auth_entry() {
        let fixture = Fixture::new();
        let bridge = fixture.bridge_with_validator(Arc::new(|_, _| {
            Err(AppError::Message("IMAP mailbox validation failed".into()))
        }));

        let status = bridge.start_login(adding("totp@proton.me")).unwrap();
        let ProtonAuthStatus::NeedsTotp { login_id, .. } = status else {
            panic!("expected TOTP");
        };
        let error = bridge
            .continue_login(&login_id, ProtonAuthResponse::Totp("123456".into()))
            .unwrap_err();

        assert_eq!(error.to_string(), "IMAP mailbox validation failed");
        assert_eq!(account_count(&fixture), 0);
        assert!(!bridge.is_running());
        assert!(lock_is_free(&fixture));
        assert!(fixture.secrets.get(&account_key(1)).is_err());
        let auth_file = fixture.config_dir().join("hydroxide/auth.json");
        let stored = fs::read_to_string(auth_file).unwrap_or_default();
        assert!(!stored.contains("totp@proton.me"), "{stored}");
    }

    #[test]
    fn failed_login_restarts_a_previously_running_bridge() {
        let fixture = Fixture::new();
        fixture.add_proton("existing@proton.me", "existing@proton.me", "old-secret");
        let bridge = fixture.bridge_with_validator(Arc::new(|_, _| {
            Err(AppError::Message("IMAP mailbox validation failed".into()))
        }));
        bridge.start_if_needed().unwrap().unwrap();
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        let status = bridge.start_login(adding("totp@proton.me")).unwrap();
        let ProtonAuthStatus::NeedsTotp { login_id, .. } = status else {
            panic!("expected TOTP");
        };
        assert!(!bridge.is_running());

        let error = bridge
            .continue_login(&login_id, ProtonAuthResponse::Totp("123456".into()))
            .unwrap_err();

        assert_eq!(error.to_string(), "IMAP mailbox validation failed");
        assert_eq!(account_count(&fixture), 1);
        assert!(bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("3"));
        assert!(lock_is_free(&fixture));
    }

    #[test]
    fn reconnect_replaces_the_secret_after_validation_and_restarts() {
        let fixture = Fixture::new();
        let account = fixture.add_proton("primary@proton.me", "reconnect@proton.me", "old-secret");
        let bridge = fixture.bridge();
        bridge.start_if_needed().unwrap().unwrap();
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        let status = bridge
            .start_login(ProtonAuthStart::Reconnect {
                account_id: account.id,
                username: "reconnect@proton.me".into(),
                password: "proton-password".into(),
            })
            .unwrap();
        let ProtonAuthStatus::NeedsTotp { login_id, .. } = status else {
            panic!("expected TOTP");
        };
        assert!(fixture.stopped());
        assert!(!bridge.is_running());

        let status = bridge
            .continue_login(&login_id, ProtonAuthResponse::Totp("123456".into()))
            .unwrap();
        let ProtonAuthStatus::Complete { account: updated } = status else {
            panic!("expected completion");
        };
        assert_eq!(updated.id, account.id);
        assert_eq!(updated.email, "primary@proton.me");
        assert_eq!(updated.username, "reconnect@proton.me");
        assert!(bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("2"));
        assert_eq!(
            fixture.secrets.get(&account_key(account.id)).unwrap(),
            "bridge-password-for-reconnect@proton.me"
        );
    }

    #[test]
    fn reconnect_requires_the_stored_login_username() {
        let fixture = Fixture::new();
        let account =
            fixture.add_proton("reconnect@proton.me", "reconnect@proton.me", "old-secret");
        let bridge = fixture.bridge();
        bridge.start_if_needed().unwrap().unwrap();

        let error = bridge
            .start_login(ProtonAuthStart::Reconnect {
                account_id: account.id,
                username: "other@proton.me".into(),
                password: "proton-password".into(),
            })
            .unwrap_err();

        assert!(error.to_string().contains("login username"), "{error}");
        assert!(bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("1"));
        assert_eq!(
            fixture.secrets.get(&account_key(account.id)).unwrap(),
            "old-secret"
        );
    }

    #[test]
    fn reconnect_validation_failure_keeps_the_old_secret() {
        let fixture = Fixture::new();
        let account =
            fixture.add_proton("reconnect@proton.me", "reconnect@proton.me", "old-secret");
        let bridge = fixture.bridge_with_validator(Arc::new(|_, _| {
            Err(AppError::Message("SMTP connection failed".into()))
        }));
        bridge.start_if_needed().unwrap().unwrap();

        let status = bridge
            .start_login(ProtonAuthStart::Reconnect {
                account_id: account.id,
                username: "reconnect@proton.me".into(),
                password: "proton-password".into(),
            })
            .unwrap();
        let ProtonAuthStatus::NeedsTotp { login_id, .. } = status else {
            panic!("expected TOTP");
        };
        let error = bridge
            .continue_login(&login_id, ProtonAuthResponse::Totp("123456".into()))
            .unwrap_err();

        assert_eq!(error.to_string(), "SMTP connection failed");
        assert_eq!(
            fixture.secrets.get(&account_key(account.id)).unwrap(),
            "old-secret"
        );
        assert!(bridge.is_running());
    }

    #[test]
    fn second_login_is_rejected_while_one_is_pending() {
        let fixture = Fixture::new();
        let bridge = fixture.bridge();

        let status = bridge.start_login(adding("totp@proton.me")).unwrap();
        let ProtonAuthStatus::NeedsTotp { login_id, .. } = status else {
            panic!("expected TOTP");
        };

        let error = bridge.start_login(adding("other@proton.me")).unwrap_err();
        assert_eq!(error.to_string(), "Proton sign-in in progress");
        assert!(!lock_is_free(&fixture));

        bridge.cancel_login(&login_id).unwrap();
        assert!(lock_is_free(&fixture));
    }

    #[test]
    fn start_if_needed_is_rejected_while_a_login_is_pending() {
        let fixture = Fixture::new();
        fixture.add_proton("existing@proton.me", "existing@proton.me", "old-secret");
        let bridge = fixture.bridge();
        bridge.start_if_needed().unwrap().unwrap();
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        let status = bridge.start_login(adding("totp@proton.me")).unwrap();
        let ProtonAuthStatus::NeedsTotp { login_id, .. } = status else {
            panic!("expected TOTP");
        };

        let error = bridge.start_if_needed().unwrap_err();
        assert_eq!(error.to_string(), "Proton sign-in in progress");
        assert!(!bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        bridge.cancel_login(&login_id).unwrap();
        assert!(bridge.is_running());
    }

    #[test]
    fn an_external_auth_lock_fails_without_starting_serve() {
        let fixture = Fixture::new();
        fixture.add_proton("existing@proton.me", "existing@proton.me", "old-secret");
        let bridge = fixture.bridge();
        bridge.start_if_needed().unwrap().unwrap();
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        let hydroxide = fixture.config_dir().join("hydroxide");
        fs::create_dir_all(&hydroxide).unwrap();
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(hydroxide.join("auth.lock"))
            .unwrap();
        lock.lock_exclusive().unwrap();

        let error = bridge.start_login(adding("totp@proton.me")).unwrap_err();
        assert_eq!(error.to_string(), "Proton sign-in in progress");
        assert!(!bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        let _ = FileExt::unlock(&lock);
    }

    #[test]
    fn cancel_without_a_login_id_releases_the_pending_session() {
        let fixture = Fixture::new();
        let bridge = fixture.bridge();

        let status = bridge.start_login(adding("totp@proton.me")).unwrap();
        let ProtonAuthStatus::NeedsTotp { login_id, .. } = status else {
            panic!("expected TOTP");
        };
        assert!(!lock_is_free(&fixture));

        bridge.cancel_active_login().unwrap();

        assert!(lock_is_free(&fixture));
        assert!(bridge
            .continue_login(&login_id, ProtonAuthResponse::Totp("1".into()))
            .is_err());
    }

    #[test]
    fn idle_pending_login_releases_the_lock() {
        let fixture = Fixture::new();
        let bridge = fixture.bridge();
        bridge.auth.set_idle_timeout(Duration::from_millis(150));

        let status = bridge.start_login(adding("totp@proton.me")).unwrap();
        let ProtonAuthStatus::NeedsTotp { login_id, .. } = status else {
            panic!("expected TOTP");
        };
        let deadline = Instant::now() + Duration::from_secs(3);
        while !lock_is_free(&fixture) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }

        assert!(lock_is_free(&fixture));
        assert!(!bridge.is_running());
        assert!(bridge
            .continue_login(&login_id, ProtonAuthResponse::Totp("1".into()))
            .is_err());
    }

    #[test]
    fn concurrent_continue_is_rejected() {
        let fixture = Fixture::new();
        let bridge = Arc::new(fixture.bridge());

        let status = bridge.start_login(adding("slow@proton.me")).unwrap();
        let ProtonAuthStatus::NeedsTotp { login_id, .. } = status else {
            panic!("expected TOTP");
        };

        let first = {
            let bridge = Arc::clone(&bridge);
            let login_id = login_id.clone();
            thread::spawn(move || {
                bridge.continue_login(&login_id, ProtonAuthResponse::Totp("123456".into()))
            })
        };
        thread::sleep(Duration::from_millis(150));

        let error = bridge
            .continue_login(&login_id, ProtonAuthResponse::Totp("123456".into()))
            .unwrap_err();
        assert_eq!(error.to_string(), "Proton sign-in is busy.");

        let status = first.join().unwrap().unwrap();
        assert!(matches!(status, ProtonAuthStatus::Complete { .. }));
    }

    #[test]
    fn add_rollback_preserves_a_preexisting_auth_entry() {
        let fixture = Fixture::new();
        let hydroxide = fixture.config_dir().join("hydroxide");
        fs::create_dir_all(&hydroxide).unwrap();
        let auth_file = hydroxide.join("auth.json");
        let original = r#"{"totp@proton.me":"old","other@proton.me":"keep"}"#;
        fs::write(&auth_file, original).unwrap();
        let bridge = fixture.bridge_with_validator(Arc::new(|_, _| {
            Err(AppError::Message("IMAP mailbox validation failed".into()))
        }));

        let status = bridge.start_login(adding("totp@proton.me")).unwrap();
        let ProtonAuthStatus::NeedsTotp { login_id, .. } = status else {
            panic!("expected TOTP");
        };
        let error = bridge
            .continue_login(&login_id, ProtonAuthResponse::Totp("123456".into()))
            .unwrap_err();

        assert_eq!(error.to_string(), "IMAP mailbox validation failed");
        assert_eq!(fs::read_to_string(auth_file).unwrap(), original);
        assert_eq!(account_count(&fixture), 0);
    }
}
