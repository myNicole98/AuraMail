use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::accounts::{self, Account};
use crate::ai::{self, classification};
use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::imap::client;
use crate::mail::{self, parse};
use crate::secrets::SecretStore;

const SYNC_RETRY_SECS: u64 = 10;
const FULL_FLAG_RECONCILE_FLOOR_SECS: i64 = 5 * 60;
const VOICE_REFRESH_SECS: i64 = 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderCursor {
    pub uid_validity: u32,
    pub highest_uid: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncAction {
    Reset,
    Incremental { from_uid: u32 },
    Noop,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub account_id: i64,
    pub state: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncNew {
    pub account_id: i64,
    pub folder_id: i64,
    pub count: u32,
    pub inserted: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFolderStatus {
    pub account_id: i64,
    pub folder_id: i64,
    pub syncing: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFolders {
    pub account_id: i64,
}

struct SyncOutcome {
    inserted: Vec<i64>,
    auto_classify: Vec<i64>,
}

pub struct SyncResult {
    pub inserted: Vec<i64>,
}

struct FolderSyncGuard<'a> {
    app: &'a AppHandle,
    account_id: i64,
    folder_id: i64,
}

impl<'a> FolderSyncGuard<'a> {
    fn new(app: &'a AppHandle, account_id: i64, folder_id: i64) -> Self {
        let _ = app.emit(
            "sync://folder",
            SyncFolderStatus {
                account_id,
                folder_id,
                syncing: true,
            },
        );
        Self {
            app,
            account_id,
            folder_id,
        }
    }
}

impl Drop for FolderSyncGuard<'_> {
    fn drop(&mut self) {
        let _ = self.app.emit(
            "sync://folder",
            SyncFolderStatus {
                account_id: self.account_id,
                folder_id: self.folder_id,
                syncing: false,
            },
        );
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationDone {
    pub account_id: i64,
    pub classified: u32,
    pub failed: u32,
    pub results: Vec<classification::ClassificationResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationStarted {
    pub account_id: i64,
    pub message_ids: Vec<i64>,
}

pub fn next_action(cursor: FolderCursor, uid_validity: u32, latest_uid: u32) -> SyncAction {
    if cursor.uid_validity != uid_validity {
        SyncAction::Reset
    } else if latest_uid > cursor.highest_uid {
        SyncAction::Incremental {
            from_uid: cursor.highest_uid + 1,
        }
    } else {
        SyncAction::Noop
    }
}

pub fn next_sync_delay(base_secs: u64, _current_secs: u64, success: bool) -> u64 {
    if success {
        base_secs.max(1)
    } else {
        SYNC_RETRY_SECS
    }
}

fn sync_order(folders: &mut [mail::Folder]) {
    folders.sort_by_key(|folder| match folder.role.as_str() {
        "inbox" => 0,
        "sent" => 1,
        _ => 2,
    });
}

pub fn sync_account(
    app: &AppHandle,
    db: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    syncing: &Mutex<HashSet<i64>>,
    account_id: i64,
) -> AppResult<SyncResult> {
    sync_account_with_report(app, db, secrets, syncing, account_id, true)
}

fn sync_account_with_report(
    app: &AppHandle,
    db: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    syncing: &Mutex<HashSet<i64>>,
    account_id: i64,
    report_failure: bool,
) -> AppResult<SyncResult> {
    let Some(account) = find_account(db.as_ref(), account_id)? else {
        return Ok(SyncResult {
            inserted: Vec::new(),
        });
    };
    if !begin_sync(syncing, account_id) {
        return Ok(SyncResult {
            inserted: Vec::new(),
        });
    }
    let _guard = SyncGuard {
        syncing,
        account_id,
    };

    emit_status(app, account_id, "syncing", None);
    match sync_account_inner(app, &db, &secrets, &account) {
        Ok(result) => {
            clear_sync_failures(account_id);
            emit_status(app, account_id, "idle", None);
            Ok(result)
        }
        Err(error) => {
            let message = error.to_string();
            if report_failure {
                if record_sync_failure(account_id) >= 3 {
                    emit_status(app, account_id, error_state(&error), Some(message.clone()));
                } else {
                    emit_status(app, account_id, "idle", None);
                }
            }
            Err(AppError::Message(message))
        }
    }
}

pub fn schedule_auto_classify(
    app: AppHandle,
    db: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    account_id: i64,
    message_ids: Vec<i64>,
) {
    let Ok(space_id) = accounts::space_for_account(&db, account_id) else {
        return;
    };
    if message_ids.is_empty()
        || !ai::space_enabled(&db, space_id).unwrap_or(false)
        || !ai::auto_classify_enabled(&db, space_id).unwrap_or(false)
    {
        return;
    }
    if !ai::status(&db, secrets.as_ref(), space_id)
        .map(|status| status.configured)
        .unwrap_or(false)
    {
        return;
    }
    app.emit(
        "classification://started",
        ClassificationStarted {
            account_id,
            message_ids: message_ids.clone(),
        },
    )
    .ok();
    tauri::async_runtime::spawn(async move {
        let batch = classification::classify_ids_detailed(&db, secrets.as_ref(), &message_ids).await;
        app.emit(
            "classification://done",
            ClassificationDone {
                account_id,
                classified: batch.classified,
                failed: batch.failed,
                results: batch.results,
            },
        )
        .ok();
    });
}

fn auto_classify_ids(action: SyncAction, inserted: &[i64]) -> Vec<i64> {
    if matches!(action, SyncAction::Incremental { .. }) {
        inserted.to_vec()
    } else {
        Vec::new()
    }
}

const MAX_AUTO_RAIL_PER_SYNC: usize = 3;

fn select_auto_rail_candidates(
    mut messages: Vec<(mail::Message, String)>,
    limit: usize,
) -> Vec<i64> {
    messages.sort_by_key(|(message, _)| Reverse((message.date, message.id)));
    let mut threads = HashSet::new();
    messages
        .into_iter()
        .filter(|(message, role)| role == "inbox" && threads.insert((message.account_id, message.thread_key.clone())))
        .take(limit)
        .map(|(message, _)| message.id)
        .collect()
}

pub fn schedule_auto_rail(
    db: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    space_id: i64,
    message_ids: Vec<i64>,
) {
    if message_ids.is_empty()
        || !ai::space_enabled(&db, space_id).unwrap_or(false)
        || !ai::auto_rail_enabled(&db, space_id).unwrap_or(false)
    {
        return;
    }
    if !ai::status(&db, secrets.as_ref(), space_id)
        .map(|status| status.configured)
        .unwrap_or(false)
    {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut messages = Vec::new();
        for message_id in message_ids {
            let Ok(message) = mail::message_by_id(&db, message_id) else {
                continue;
            };
            let Ok(Some(folder)) = mail::folder_by_id(&db, message.folder_id) else {
                continue;
            };
            messages.push((message, folder.role));
        }
        let candidates = select_auto_rail_candidates(messages, MAX_AUTO_RAIL_PER_SYNC);
        let full_body = ai::full_body_enabled(&db, space_id).unwrap_or(false);
        for message_id in candidates {
            if let Err(error) = crate::commands::generate_aura_rail(
                Arc::clone(&db),
                Arc::clone(&secrets),
                message_id,
                None,
                full_body,
            )
            .await
            {
                eprintln!("Aura rail pre-read failed for message {message_id}: {error}");
            }
        }
    });
}

fn voice_refresh_due(db: &Database, space_id: i64, stamp: i64) -> bool {
    let last = ai::setting_for_space(db, space_id, ai::voice::VOICE_LAST_ATTEMPT_KEY)
        .ok()
        .flatten()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    last == 0 || stamp.saturating_sub(last) >= VOICE_REFRESH_SECS
}

fn schedule_auto_voice(
    db: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    space_id: i64,
) {
    if !ai::space_enabled(&db, space_id).unwrap_or(false)
        || !ai::match_voice_enabled(&db, space_id).unwrap_or(false)
    {
        return;
    }
    let stamp = crate::db::now();
    if !voice_refresh_due(&db, space_id, stamp) {
        return;
    }
    let _ = db.set_setting(
        &ai::scoped_key(space_id, ai::voice::VOICE_LAST_ATTEMPT_KEY),
        &stamp.to_string(),
    );
    tauri::async_runtime::spawn(async move {
        if let Err(error) = crate::commands::refresh_voice_profile(db, secrets, space_id).await {
            eprintln!("Aura voice refresh failed: {error}");
        }
    });
}

fn sync_account_inner(
    app: &AppHandle,
    db: &Arc<Database>,
    secrets: &Arc<dyn SecretStore>,
    account: &Account,
) -> AppResult<SyncResult> {
    let credential = crate::mail_auth::credential_for(account, secrets.as_ref())?;
    let mut session = client::open_session(account, &credential)?;
    let remote_folders = client::list_folders(&mut session)?;
    let remote_paths: Vec<String> = remote_folders
        .iter()
        .map(|folder| folder.path.clone())
        .collect();
    let mut folders = Vec::new();
    for remote in remote_folders {
        let folder = mail::upsert_folder(
            db.as_ref(),
            account.id,
            &remote.path,
            &remote.name,
            &remote.role,
            &remote.delimiter,
        )?;
        folders.push(folder);
    }
    mail::reconcile_folders(db.as_ref(), account.id, &remote_paths)?;
    sync_order(&mut folders);
    app.emit(
        "sync://folders",
        SyncFolders {
            account_id: account.id,
        },
    )
    .ok();

    let result = {
        let mut first_error = None;
        let mut inserted = Vec::new();
        for folder in folders {
            match sync_folder(&mut session, app, db.as_ref(), account, &folder, true) {
                Ok(mut outcome) => {
                    let classify_ids = std::mem::take(&mut outcome.auto_classify);
                    if !classify_ids.is_empty() {
                        schedule_auto_classify(
                            app.clone(),
                            Arc::clone(db),
                            Arc::clone(secrets),
                            account.id,
                            classify_ids,
                        );
                    }
                    inserted.append(&mut outcome.inserted);
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(SyncResult { inserted }),
        }
    };
    let _ = session.logout();
    result
}

fn sync_inbox_inner(
    app: &AppHandle,
    db: &Arc<Database>,
    secrets: &Arc<dyn SecretStore>,
    account: &Account,
) -> AppResult<SyncResult> {
    let credential = crate::mail_auth::credential_for(account, secrets.as_ref())?;
    let mut session = client::open_session(account, &credential)?;
    let folders = mail::list_folders(db.as_ref(), Some(account.id), None)?
        .into_iter()
        .filter(|folder| folder.role == "inbox")
        .collect::<Vec<_>>();
    let result = {
        let mut first_error = None;
        let mut inserted = Vec::new();
        for folder in folders {
            match sync_folder(&mut session, app, db.as_ref(), account, &folder, false) {
                Ok(mut outcome) => {
                    let classify_ids = std::mem::take(&mut outcome.auto_classify);
                    if !classify_ids.is_empty() {
                        schedule_auto_classify(
                            app.clone(),
                            Arc::clone(db),
                            Arc::clone(secrets),
                            account.id,
                            classify_ids,
                        );
                    }
                    inserted.append(&mut outcome.inserted);
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(SyncResult { inserted }),
        }
    };
    let _ = session.logout();
    result
}

pub fn sync_inbox(
    app: &AppHandle,
    db: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    syncing: &Mutex<HashSet<i64>>,
    account_id: i64,
) -> AppResult<SyncResult> {
    sync_inbox_with_report(app, db, secrets, syncing, account_id, true)
}

fn sync_inbox_with_report(
    app: &AppHandle,
    db: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    syncing: &Mutex<HashSet<i64>>,
    account_id: i64,
    report_failure: bool,
) -> AppResult<SyncResult> {
    let Some(account) = find_account(db.as_ref(), account_id)? else {
        return Ok(SyncResult {
            inserted: Vec::new(),
        });
    };
    if !begin_sync(syncing, account_id) {
        return Ok(SyncResult {
            inserted: Vec::new(),
        });
    }
    let _guard = SyncGuard {
        syncing,
        account_id,
    };
    match sync_inbox_inner(app, &db, &secrets, &account) {
        Ok(result) => {
            clear_sync_failures(account_id);
            emit_status(app, account_id, "idle", None);
            Ok(result)
        }
        Err(error) => {
            let message = error.to_string();
            if report_failure {
                if record_sync_failure(account_id) >= 3 {
                    emit_status(app, account_id, error_state(&error), Some(message.clone()));
                } else {
                    emit_status(app, account_id, "idle", None);
                }
            }
            Err(AppError::Message(message))
        }
    }
}

fn store_header_batch(
    app: &AppHandle,
    db: &Database,
    account_id: i64,
    folder_id: i64,
    headers: &[mail::MessageHeader],
) -> AppResult<Vec<i64>> {
    let inserted = mail::apply_folder_headers_batch(db, account_id, folder_id, headers)?;
    if !inserted.is_empty() {
        app.emit(
            "sync://new",
            SyncNew {
                account_id,
                folder_id,
                count: inserted.len() as u32,
                inserted: inserted.len() as u32,
            },
        )
        .ok();
    }
    Ok(inserted)
}

fn sync_folder(
    session: &mut client::ImapSession,
    app: &AppHandle,
    db: &Database,
    account: &Account,
    folder: &mail::Folder,
    reconcile: bool,
) -> AppResult<SyncOutcome> {
    let state = client::folder_state(session, &folder.path)?;
    let (uid_validity, highest_uid) = mail::folder_cursor(db, folder.id)?;
    let action = next_action(
        FolderCursor {
            uid_validity,
            highest_uid,
        },
        state.uid_validity,
        state.latest_uid,
    );
    if matches!(action, SyncAction::Noop) && !reconcile {
        return Ok(SyncOutcome {
            inserted: Vec::new(),
            auto_classify: Vec::new(),
        });
    }
    let _sync_status = FolderSyncGuard::new(app, account.id, folder.id);

    let mut changed = 0;
    if reconcile && !matches!(action, SyncAction::Reset) {
        let remote = client::folder_flags(session, &folder.path)?
            .into_iter()
            .map(|(uid, flags)| {
                (
                    uid,
                    has_flag(&flags, "\\Seen"),
                    has_flag(&flags, "\\Flagged"),
                )
            })
            .collect::<Vec<_>>();
        changed = mail::reconcile_folder_messages(db, folder.id, &remote)?;
    }

    let inserted = match action {
        SyncAction::Noop => Vec::new(),
        SyncAction::Reset => {
            if state.latest_uid == 0 {
                mail::clear_folder_messages(db, folder.id)?;
                mail::set_folder_cursor(db, folder.id, state.uid_validity, 0)?;
                Vec::new()
            } else {
                let mut inserted = Vec::new();
                let mut highest = state.latest_uid;
                let mut first_batch = true;
                client::fetch_headers_batched(session, &folder.path, 1, |fetched| {
                    if first_batch {
                        mail::clear_folder_messages(db, folder.id)?;
                        first_batch = false;
                    }
                    highest = highest
                        .max(fetched.iter().map(|header| header.uid).max().unwrap_or(0));
                    let headers = build_headers(&fetched);
                    inserted.extend(store_header_batch(
                        app,
                        db,
                        account.id,
                        folder.id,
                        &headers,
                    )?);
                    Ok(())
                })?;
                if first_batch {
                    mail::clear_folder_messages(db, folder.id)?;
                }
                mail::set_folder_cursor(db, folder.id, state.uid_validity, highest)?;
                inserted
            }
        }
        SyncAction::Incremental { from_uid } => {
            let mut inserted = Vec::new();
            let mut highest = state.latest_uid;
            client::fetch_headers_batched(session, &folder.path, from_uid, |fetched| {
                highest = highest.max(fetched.iter().map(|header| header.uid).max().unwrap_or(0));
                let headers = build_headers(&fetched);
                inserted.extend(store_header_batch(
                    app,
                    db,
                    account.id,
                    folder.id,
                    &headers,
                )?);
                Ok(())
            })?;
            mail::set_folder_cursor(db, folder.id, state.uid_validity, highest)?;
            inserted
        }
    };

    if changed > 0 {
        app.emit(
            "sync://new",
            SyncNew {
                account_id: account.id,
                folder_id: folder.id,
                count: changed as u32,
                inserted: 0,
            },
        )
        .ok();
    }
    if !inserted.is_empty() || changed > 0 {
        crate::desktop::refresh_tray_unread(app, db);
    }
    if folder.role == "inbox"
        && !inserted.is_empty()
        && matches!(action, SyncAction::Incremental { .. })
    {
        crate::desktop::notify_new_mail(app, db, account, &inserted);
        crate::desktop::play_new_mail_if_enabled(db, account.space_id);
    }
    let auto_classify = auto_classify_ids(action, &inserted);
    Ok(SyncOutcome {
        inserted,
        auto_classify,
    })
}

fn build_headers(headers: &[client::FetchedHeader]) -> Vec<mail::MessageHeader> {
    headers
        .iter()
        .map(|header| mail::MessageHeader {
            uid: header.uid,
            headers: parse::parse_headers_with_preview(&header.raw, &header.preview),
            is_seen: has_flag(&header.flags, "\\Seen"),
            is_starred: has_flag(&header.flags, "\\Flagged"),
        })
        .collect()
}

fn has_flag(flags: &[String], expected: &str) -> bool {
    flags.iter().any(|flag| flag.eq_ignore_ascii_case(expected))
}

fn find_account(db: &Database, account_id: i64) -> AppResult<Option<Account>> {
    Ok(accounts::list(db)?
        .into_iter()
        .find(|account| account.id == account_id))
}

fn begin_sync(syncing: &Mutex<HashSet<i64>>, account_id: i64) -> bool {
    syncing
        .lock()
        .map(|mut accounts| accounts.insert(account_id))
        .unwrap_or(false)
}

fn sync_failures() -> &'static Mutex<HashMap<i64, u32>> {
    static FAILURES: OnceLock<Mutex<HashMap<i64, u32>>> = OnceLock::new();
    FAILURES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn record_sync_failure(account_id: i64) -> u32 {
    let mut failures = sync_failures().lock().unwrap();
    let count = failures.entry(account_id).or_insert(0);
    *count = count.saturating_add(1);
    *count
}

fn clear_sync_failures(account_id: i64) {
    if let Ok(mut failures) = sync_failures().lock() {
        failures.remove(&account_id);
    }
}

fn emit_status(app: &AppHandle, account_id: i64, state: &str, message: Option<String>) {
    app.emit(
        "sync://status",
        SyncStatus {
            account_id,
            state: state.to_string(),
            message,
        },
    )
    .ok();
}

fn error_state(error: &AppError) -> &'static str {
    if error.is_reauth() {
        return "reauth";
    }
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("connection")
        || message.contains("timed out")
        || message.contains("tls handshake")
    {
        "offline"
    } else {
        "error"
    }
}

pub fn start_polling(
    app: AppHandle,
    db: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    syncing: Arc<Mutex<HashSet<i64>>>,
) {
    tauri::async_runtime::spawn(async move {
        let mut started = HashSet::new();
        let sync_gate = Arc::new(tokio::sync::Mutex::new(()));
        loop {
            if let Ok(accounts) = accounts::list(&db) {
                for account in accounts {
                    if !started.insert(account.id) {
                        continue;
                    }
                    let app = app.clone();
                    let db = Arc::clone(&db);
                    let secrets = Arc::clone(&secrets);
                    let syncing = Arc::clone(&syncing);
                    let gate = Arc::clone(&sync_gate);
                    let account_id = account.id;
                    let full_interval = account
                        .sync_interval_secs
                        .max(FULL_FLAG_RECONCILE_FLOOR_SECS)
                        .clamp(60, 3600) as u64;
                    let fast = accounts::fast_sync_interval(&db)
                        .unwrap_or(30)
                        .clamp(15, 3600) as u64;
                    tauri::async_runtime::spawn(async move {
                        let mut since_full = 0u64;
                        let mut delay = fast;
                        loop {
                            tokio::time::sleep(Duration::from_secs(delay)).await;
                            if find_account(&db, account_id).ok().flatten().is_none() {
                                break;
                            }
                            since_full = since_full.saturating_add(delay);
                            let full = since_full >= full_interval;
                            let task_app = app.clone();
                            let task_db = Arc::clone(&db);
                            let task_secrets = Arc::clone(&secrets);
                            let task_syncing = Arc::clone(&syncing);
                            let _gate = gate.lock().await;
                            let result = tauri::async_runtime::spawn_blocking(move || {
                                if full {
                                    sync_account(
                                        &task_app,
                                        task_db,
                                        task_secrets,
                                        &task_syncing,
                                        account_id,
                                    )
                                } else {
                                    sync_inbox(
                                        &task_app,
                                        task_db,
                                        task_secrets,
                                        &task_syncing,
                                        account_id,
                                    )
                                }
                            })
                            .await;
                            let (inserted, success) = match result {
                                Ok(Ok(result)) => (result.inserted, true),
                                _ => (Vec::new(), false),
                            };
                            if full && success {
                                since_full = 0;
                            }
                            delay = if success {
                                fast
                            } else {
                                next_sync_delay(fast, delay, false)
                            };
                            if !inserted.is_empty() {
                                let sent_inserted = inserted.iter().any(|id| {
                                    mail::message_by_id(&db, *id)
                                        .ok()
                                        .and_then(|message| {
                                            mail::folder_by_id(&db, message.folder_id)
                                                .ok()
                                                .flatten()
                                        })
                                        .map(|folder| folder.role == "sent")
                                        .unwrap_or(false)
                                });
                                mail::preload::schedule_preload(
                                    Arc::clone(&db),
                                    Arc::clone(&secrets),
                                    inserted.clone(),
                                );
                                let rail_ids = inserted.clone();
                                schedule_auto_rail(
                                    Arc::clone(&db),
                                    Arc::clone(&secrets),
                                    account.space_id,
                                    rail_ids,
                                );
                                if sent_inserted {
                                    schedule_auto_voice(
                                        Arc::clone(&db),
                                        Arc::clone(&secrets),
                                        account.space_id,
                                    );
                                }
                                ai::semantic::schedule_index(
                                    Arc::clone(&db),
                                    Arc::clone(&secrets),
                                    account.space_id,
                                );
                            }
                        }
                    });
                }
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

struct SyncGuard<'a> {
    syncing: &'a Mutex<HashSet<i64>>,
    account_id: i64,
}

impl Drop for SyncGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut accounts) = self.syncing.lock() {
            accounts.remove(&self.account_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_folder(id: i64, name: &str, role: &str) -> mail::Folder {
        mail::Folder {
            id,
            account_id: 1,
            path: name.into(),
            name: name.into(),
            role: role.into(),
            delimiter: "/".into(),
            uid_validity: 1,
            highest_uid: 0,
            unread_count: 0,
        }
    }

    fn sample_message(id: i64, date: i64, thread_key: &str) -> mail::Message {
        mail::Message {
            id,
            account_id: 1,
            folder_id: 1,
            uid: id as u32,
            message_id: Some(format!("<{id}@example.com>")),
            references: None,
            thread_key: thread_key.into(),
            from_addr: "ada@example.com".into(),
            from_name: "Ada".into(),
            to_addrs: vec!["me@example.com".into()],
            cc_addrs: Vec::new(),
            subject: format!("Subject {id}"),
            date,
            snippet: "Snippet".into(),
            is_seen: false,
            is_starred: false,
            has_attachments: false,
            body_cached: false,
            tab_id: None,
        }
    }

    #[test]
    fn voice_refresh_is_throttled() {
        let db = Database::open_memory().unwrap();
        let space_id = crate::spaces::default_id(&db).unwrap();
        assert!(voice_refresh_due(&db, space_id, 10_000));

        db.set_setting(
            &ai::scoped_key(space_id, ai::voice::VOICE_LAST_ATTEMPT_KEY),
            "10000",
        )
        .unwrap();
        assert!(!voice_refresh_due(
            &db,
            space_id,
            10_000 + VOICE_REFRESH_SECS - 1
        ));
        assert!(voice_refresh_due(
            &db,
            space_id,
            10_000 + VOICE_REFRESH_SECS
        ));
    }

    #[test]
    fn initial_import_is_not_auto_classified() {
        assert!(auto_classify_ids(SyncAction::Reset, &[10, 11]).is_empty());
    }

    #[test]
    fn incremental_import_is_auto_classified() {
        assert_eq!(
            auto_classify_ids(SyncAction::Incremental { from_uid: 10 }, &[11, 12]),
            vec![11, 12]
        );
    }

    #[test]
    fn auto_rail_candidates_are_newest_inbox_threads() {
        let messages = vec![
            (sample_message(1, 10, "t1"), "inbox".to_string()),
            (sample_message(2, 30, "t1"), "inbox".to_string()),
            (sample_message(3, 20, "t2"), "inbox".to_string()),
            (sample_message(4, 40, "t3"), "archive".to_string()),
            (sample_message(5, 5, "t4"), "inbox".to_string()),
        ];
        assert_eq!(
            select_auto_rail_candidates(messages, 3),
            vec![2, 3, 5]
        );
    }

    #[test]
    fn reset_when_uid_validity_changes() {
        let cursor = FolderCursor {
            uid_validity: 10,
            highest_uid: 50,
        };
        assert!(matches!(next_action(cursor, 11, 60), SyncAction::Reset));
    }

    #[test]
    fn increment_from_highest_uid() {
        let cursor = FolderCursor {
            uid_validity: 10,
            highest_uid: 50,
        };
        assert!(matches!(
            next_action(cursor, 10, 60),
            SyncAction::Incremental { from_uid: 51 }
        ));
    }

    #[test]
    fn noop_when_nothing_new() {
        let cursor = FolderCursor {
            uid_validity: 10,
            highest_uid: 60,
        };
        assert!(matches!(next_action(cursor, 10, 60), SyncAction::Noop));
    }

    #[test]
    fn noop_when_latest_uid_is_behind_cursor() {
        let cursor = FolderCursor {
            uid_validity: 10,
            highest_uid: 60,
        };
        assert!(matches!(next_action(cursor, 10, 42), SyncAction::Noop));
    }

    #[test]
    fn sync_failures_retry_after_ten_seconds() {
        assert_eq!(next_sync_delay(30, 30, false), 10);
        assert_eq!(next_sync_delay(30, 10, false), 10);
        assert_eq!(next_sync_delay(30, 30, true), 30);
    }

    #[test]
    fn syncs_inbox_before_other_folders() {
        let mut folders = vec![
            sample_folder(1, "Sent", "sent"),
            sample_folder(2, "All Mail", "all"),
            sample_folder(3, "INBOX", "inbox"),
            sample_folder(4, "Archive", "archive"),
        ];
        sync_order(&mut folders);
        assert_eq!(
            folders.iter().map(|folder| folder.role.as_str()).collect::<Vec<_>>(),
            vec!["inbox", "sent", "all", "archive"]
        );
    }

    #[test]
    fn offline_state_is_selected_for_network_failures() {
        assert_eq!(
            error_state(&AppError::Message("IMAP connection timed out".into())),
            "offline"
        );
        assert_eq!(
            error_state(&AppError::Message("IMAP authentication failed".into())),
            "error"
        );
    }

    #[test]
    fn failures_are_counted_until_a_success_resets_them() {
        let account_id = 9_999_991;
        clear_sync_failures(account_id);
        assert_eq!(record_sync_failure(account_id), 1);
        assert_eq!(record_sync_failure(account_id), 2);
        assert_eq!(record_sync_failure(account_id), 3);
        clear_sync_failures(account_id);
        assert_eq!(record_sync_failure(account_id), 1);
        clear_sync_failures(account_id);
    }

    #[test]
    fn authorization_expiry_uses_the_reauth_state() {
        let error = AppError::Reauth("Google authorization expired. Reconnect this account.".into());
        assert_eq!(error_state(&error), "reauth");
        assert_eq!(
            error_state(&AppError::Message("IMAP authentication failed".into())),
            "error"
        );
    }
}
