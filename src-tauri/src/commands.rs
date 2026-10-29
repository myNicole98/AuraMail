use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::accounts::{self, Account, AccountAuth, AccountInput, MailSecurity};
use crate::calendar::{
    self, CalendarAccount, CalendarAccountInput, CalendarEvent, CalendarEventInput,
    CalendarSummary, CalendarSyncResult,
};
use crate::ai::classification::{self, Classification};
use crate::ai::client::ChatMessage;
use crate::ai::{
    self, prompts, AiAuraAskResponse, AiAuraRail, AiChatRequest, AiChatResponse, AiComposeRequest,
    AiComposeResponse, AiConfig, AiConfigInput, AiConfigStatus, AiDraft, AiPreferences, MailContext,
};
use crate::ai::semantic::SemanticStatus;
use crate::ai::voice::AiVoiceProfile;
use crate::compose::{self, ComposeInput, Draft, DraftSummary};
use crate::contacts::{self, Contact};
use crate::db::{now, Database};
use crate::error::{AppError, AppResult};
use crate::imap::client::{self, validate_imap};
use crate::imap::sync;
use crate::links;
use crate::mail::{
    self, parse, Folder, Message, MessageDetail, MessageQuery, TabUnreadCounts, ThreadMessage,
};
use crate::mail_auth::MailCredential;
use crate::oauth;
use crate::proton::{ProtonAuthResponse, ProtonAuthStart, ProtonAuthStatus};
use crate::secrets::{SecretStore, AI_KEY};
use crate::smtp::{self, validate_smtp};
use crate::spaces::{self, Space, SpaceDeleteAction};
use crate::tabs::{self, Tab};
use crate::theme::{self, Base16Palette, ThemeSettings};
use crate::AppState;

const MAX_ATTACHMENT_SIZE: i64 = 25 * 1024 * 1024;
const MAX_ATTACHMENT_PREVIEW_SIZE: i64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct AttachmentPreview {
    pub mime_type: String,
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageSnippet {
    pub message_id: i64,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassificationOutcome {
    pub message_id: i64,
    pub classification: Option<Classification>,
    pub error: Option<String>,
    pub error_kind: Option<String>,
}

fn blocking_error() -> AppError {
    AppError::Message("operation failed".into())
}

fn error_kind(error: &AppError) -> &'static str {
    match error {
        AppError::Message(_) => "message",
        AppError::Reauth(_) => "reauth",
        AppError::Database(_) => "database",
        AppError::Network(_) => "network",
        AppError::Io(_) => "io",
        AppError::Json(_) => "json",
    }
}

fn effective_full_body(db: &Database, space_id: i64, requested: bool) -> AppResult<bool> {
    Ok(requested && ai::full_body_enabled(db, space_id)?)
}

fn space_for_message(db: &Database, message_id: i64) -> AppResult<i64> {
    let message = mail::message_by_id(db, message_id)?;
    accounts::space_for_account(db, message.account_id)
}

fn ensure_trash_delete(folder_role: &str) -> AppResult<()> {
    if folder_role == "trash" {
        Ok(())
    } else {
        Err(AppError::Message(
            "only messages in trash can be deleted permanently".into(),
        ))
    }
}

fn account_by_id(db: &Database, account_id: i64) -> AppResult<Account> {
    accounts::list(db)?
        .into_iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| AppError::Message("account not found".into()))
}

fn message_context(db: &Database, message_id: i64) -> AppResult<(Message, Folder, Account)> {
    let message = mail::message_by_id(db, message_id)?;
    let folder = mail::folder_by_id(db, message.folder_id)?
        .ok_or_else(|| AppError::Message("folder not found".into()))?;
    let account = account_by_id(db, message.account_id)?;
    Ok((message, folder, account))
}

fn plain_body(detail: &MessageDetail) -> String {
    detail
        .text_body
        .as_deref()
        .map(str::trim)
        .filter(|body| !body.is_empty())
        .or_else(|| {
            detail
                .html_body
                .as_deref()
                .map(str::trim)
                .filter(|body| !body.is_empty())
        })
        .map(str::to_string)
        .unwrap_or_else(|| detail.message.snippet.clone())
}

fn mail_context(message: &Message, body: String) -> MailContext {
    MailContext {
        from_name: message.from_name.clone(),
        from_addr: message.from_addr.clone(),
        subject: message.subject.clone(),
        body,
    }
}

fn reply_mail_context(
    message: &Message,
    detail: Option<&MessageDetail>,
    full_body: bool,
) -> MailContext {
    let body = if full_body {
        detail
            .map(plain_body)
            .unwrap_or_else(|| message.snippet.clone())
    } else {
        message.snippet.clone()
    };
    mail_context(message, body)
}

fn attachment_position(
    parsed: &[parse::ParsedAttachment],
    attachment: &mail::Attachment,
) -> Option<usize> {
    parsed
        .iter()
        .position(|candidate| {
            candidate.filename == attachment.filename
                && candidate.size as i64 == attachment.size
                && candidate.content_id == attachment.content_id
        })
        .or_else(|| {
            parsed.iter().position(|candidate| {
                candidate.filename == attachment.filename
                    && candidate.size as i64 == attachment.size
            })
        })
}

fn validate_move_target(current: &Folder, target: &Folder) -> AppResult<()> {
    if target.id == current.id {
        return Err(AppError::Message(
            "message is already in that folder".into(),
        ));
    }
    if target.account_id != current.account_id {
        return Err(AppError::Message(
            "cannot move a message to another account".into(),
        ));
    }
    if target.role == "drafts" || target.role == "all" {
        return Err(AppError::Message(
            "that folder cannot receive moved messages".into(),
        ));
    }
    Ok(())
}

fn trash_destination(folder_role: &str, trash: Option<String>) -> AppResult<String> {
    if folder_role == "trash" {
        return Err(AppError::Message(
            "message is already in trash; delete it permanently instead".into(),
        ));
    }
    trash.ok_or_else(|| AppError::Message("trash folder is unavailable".into()))
}

fn cached_attachment(path: &std::path::Path) -> AppResult<Option<String>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(AppError::Message("attachment cache path is unsafe".into()))
        }
        Ok(metadata) if metadata.is_file() => Ok(Some(path.to_string_lossy().to_string())),
        Ok(_) => Err(AppError::Message("attachment cache path is unsafe".into())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn previewable_image(mime_type: &str, size: i64) -> bool {
    let essence = mime_type
        .trim()
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let Some(subtype) = essence.strip_prefix("image/") else {
        return false;
    };
    !subtype.is_empty() && !subtype.contains("svg") && size <= MAX_ATTACHMENT_PREVIEW_SIZE
}

fn preview_data_url(mime_type: &str, bytes: &[u8]) -> String {
    use base64::Engine;

    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{mime_type};base64,{encoded}")
}

fn fetch_attachment_bytes(
    db: &Database,
    secrets: &dyn SecretStore,
    attachment_id: i64,
) -> AppResult<(mail::Attachment, Vec<u8>)> {
    let attachment = mail::attachment_by_id(db, attachment_id)?;
    if attachment.size > MAX_ATTACHMENT_SIZE {
        return Err(AppError::Message("attachment is too large".into()));
    }
    if let Some(path) = mail::attachment_cache_path(db, attachment_id)? {
        if let Some(cached) = cached_attachment(std::path::Path::new(&path))? {
            let bytes = std::fs::read(&cached)?;
            if bytes.len() as i64 > MAX_ATTACHMENT_SIZE {
                return Err(AppError::Message("attachment is too large".into()));
            }
            return Ok((attachment, bytes));
        }
    }

    let (message, folder, account) = message_context(db, attachment.message_id)?;
    let credential = crate::mail_auth::credential_for(&account, secrets)?;
    let raw = client::fetch_message(&account, &credential, &folder.path, message.uid)?;
    let parsed = parse::parse_message(&raw);
    let contents = attachment_position(&parsed.attachments, &attachment)
        .and_then(|index| parsed.attachments.get(index))
        .ok_or_else(|| AppError::Message("attachment is unavailable".into()))?;
    if contents.data.len() as i64 > MAX_ATTACHMENT_SIZE {
        return Err(AppError::Message("attachment is too large".into()));
    }
    Ok((attachment, contents.data.clone()))
}

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> AppResult<Vec<Account>> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || accounts::list(&db))
        .await
        .map_err(|_| blocking_error())?
}

#[derive(Debug, Clone, Serialize)]
pub struct SpaceUnread {
    pub space_id: i64,
    pub unread: i64,
}

#[tauri::command]
pub async fn list_spaces(state: State<'_, AppState>) -> AppResult<Vec<Space>> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || spaces::list(&db))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn space_unread_counts(
    state: State<'_, AppState>,
) -> AppResult<Vec<SpaceUnread>> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || {
        Ok(spaces::list(&db)?
            .into_iter()
            .map(|space| {
                let unread = mail::unread_inbox_count_for_space(&db, space.id).unwrap_or(0);
                SpaceUnread {
                    space_id: space.id,
                    unread,
                }
            })
            .collect())
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub fn create_space(
    state: State<'_, AppState>,
    name: String,
    icon: Option<String>,
) -> AppResult<Space> {
    spaces::create_with_icon(&state.db, &name, icon.as_deref())
}

#[tauri::command]
pub fn rename_space(
    state: State<'_, AppState>,
    id: i64,
    name: String,
    icon: Option<String>,
) -> AppResult<Space> {
    spaces::rename_with_icon(&state.db, id, &name, icon.as_deref())
}

#[tauri::command]
pub async fn delete_space(
    state: State<'_, AppState>,
    id: i64,
    action: SpaceDeleteAction,
) -> AppResult<Vec<Space>> {
    let target_space_id = match &action {
        SpaceDeleteAction::Migrate { target_space_id } => Some(*target_space_id),
        SpaceDeleteAction::DeleteAccounts => None,
    };
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    let remaining = tauri::async_runtime::spawn_blocking(move || {
        spaces::delete(&db, secrets.as_ref(), id, action)
    })
    .await
    .map_err(|_| blocking_error())??;
    if let Some(target_space_id) = target_space_id {
        if ai::space_enabled(&state.db, target_space_id).unwrap_or(false) {
            if let Ok(accounts) = accounts::list_for_space(&state.db, target_space_id) {
                for account in accounts {
                    let db = Arc::clone(&state.db);
                    let secrets = Arc::clone(&state.secrets);
                    tauri::async_runtime::spawn(async move {
                        let _ = classification::classify_backlog_in_space(
                            &db,
                            secrets.as_ref(),
                            Some(account.id),
                            None,
                            Some(target_space_id),
                            25,
                        )
                        .await;
                    });
                }
                ai::semantic::schedule_index(
                    Arc::clone(&state.db),
                    Arc::clone(&state.secrets),
                    target_space_id,
                );
            }
        }
    }
    Ok(remaining)
}

#[tauri::command]
pub fn reorder_accounts(
    state: State<'_, AppState>,
    space_id: i64,
    ids: Vec<i64>,
) -> AppResult<Vec<Account>> {
    accounts::reorder(&state.db, space_id, &ids)
}

#[tauri::command]
pub async fn move_account(
    state: State<'_, AppState>,
    id: i64,
    space_id: i64,
) -> AppResult<Account> {
    let db = Arc::clone(&state.db);
    let account = tauri::async_runtime::spawn_blocking(move || {
        accounts::move_to_space(&db, id, space_id)
    })
    .await
    .map_err(|_| blocking_error())??;
    if ai::space_enabled(&state.db, space_id).unwrap_or(false) {
        let db = Arc::clone(&state.db);
        let secrets = Arc::clone(&state.secrets);
        tauri::async_runtime::spawn(async move {
            let _ = classification::classify_backlog_in_space(
                &db,
                secrets.as_ref(),
                Some(id),
                None,
                Some(space_id),
                25,
            )
            .await;
        });
        ai::semantic::schedule_index(
            Arc::clone(&state.db),
            Arc::clone(&state.secrets),
            space_id,
        );
    }
    Ok(account)
}

#[derive(Clone, Serialize)]
pub struct OAuthUiStatus {
    pub phase: String,
    pub url: Option<String>,
}

#[tauri::command]
pub fn google_oauth_status() -> oauth::google::GoogleOAuthStatus {
    oauth::google::google_status()
}

#[tauri::command]
pub fn microsoft_oauth_status() -> oauth::microsoft::MicrosoftOAuthStatus {
    oauth::microsoft::microsoft_status()
}

#[tauri::command]
pub async fn start_google_oauth(
    app: AppHandle,
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<Account> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    let flow = Arc::clone(&state.oauth_flow);
    tauri::async_runtime::spawn_blocking(move || {
        let emit_phase = move |phase: &str, url: Option<&str>| {
            let _ = app.emit(
                "oauth://status",
                OAuthUiStatus {
                    phase: phase.into(),
                    url: url.map(str::to_string),
                },
            );
        };
        oauth::account::start_google_oauth(&db, secrets.as_ref(), &flow, space_id, &emit_phase)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn reconnect_google_account(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<Account> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    let flow = Arc::clone(&state.oauth_flow);
    tauri::async_runtime::spawn_blocking(move || {
        let emit_phase = move |phase: &str, url: Option<&str>| {
            let _ = app.emit(
                "oauth://status",
                OAuthUiStatus {
                    phase: phase.into(),
                    url: url.map(str::to_string),
                },
            );
        };
        oauth::account::reconnect_google_account(&db, secrets.as_ref(), &flow, id, &emit_phase)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub fn cancel_google_oauth(state: State<'_, AppState>) {
    state.oauth_flow.cancel();
}

#[tauri::command]
pub async fn start_microsoft_oauth(
    app: AppHandle,
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<Account> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    let flow = Arc::clone(&state.oauth_flow);
    tauri::async_runtime::spawn_blocking(move || {
        let emit_phase = move |phase: &str, url: Option<&str>| {
            let _ = app.emit(
                "oauth://status",
                OAuthUiStatus {
                    phase: phase.into(),
                    url: url.map(str::to_string),
                },
            );
        };
        oauth::account::start_microsoft_oauth(
            &db,
            secrets.as_ref(),
            &flow,
            space_id,
            &emit_phase,
        )
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn reconnect_microsoft_account(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<Account> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    let flow = Arc::clone(&state.oauth_flow);
    tauri::async_runtime::spawn_blocking(move || {
        let emit_phase = move |phase: &str, url: Option<&str>| {
            let _ = app.emit(
                "oauth://status",
                OAuthUiStatus {
                    phase: phase.into(),
                    url: url.map(str::to_string),
                },
            );
        };
        oauth::account::reconnect_microsoft_account(
            &db,
            secrets.as_ref(),
            &flow,
            id,
            &emit_phase,
        )
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub fn cancel_microsoft_oauth(state: State<'_, AppState>) {
    state.oauth_flow.cancel();
}

#[tauri::command]
pub async fn get_ai_config(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<AiConfigStatus> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        ai::status(&db, secrets.as_ref(), space_id)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn set_ai_enabled(
    state: State<'_, AppState>,
    space_id: i64,
    enabled: bool,
) -> AppResult<AiConfigStatus> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        ai::set_enabled(&db, space_id, enabled)?;
        ai::status(&db, secrets.as_ref(), space_id)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn discover_ai_providers() -> Vec<ai::discovery::DiscoveredProvider> {
    ai::discovery::discover().await
}

#[tauri::command]
pub async fn list_ai_models(
    state: State<'_, AppState>,
    space_id: i64,
    base_url: String,
    api_key: Option<String>,
) -> AppResult<Vec<String>> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(AppError::Message("AI base URL is required".into()));
    }
    let url = reqwest::Url::parse(&format!("{base}/models"))
        .map_err(|_| AppError::Message("AI base URL is invalid".into()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::Message(
            "AI base URL must start with http:// or https://".into(),
        ));
    }
    let key = api_key
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .or_else(|| {
            ai::stored_key_for(&state.db, state.secrets.as_ref(), space_id, &base_url)
        });
    let headers = ai::client::provider_headers(&base_url, key.as_deref().unwrap_or(""))?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let response = client.get(url).headers(headers).send().await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        let detail = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(|message| message.as_str())
                    .map(str::to_string)
            })
            .map(|message| format!(": {message}"))
            .unwrap_or_default();
        return Err(AppError::Message(format!(
            "Could not load models ({}){detail}",
            status.as_u16()
        )));
    }
    Ok(ai::discovery::parse_openai(&body))
}

#[tauri::command]
pub async fn ai_status(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<bool> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || ai::space_enabled(&db, space_id))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub fn get_ai_preferences(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<AiPreferences> {
    ai::preferences(&state.db, space_id)
}

#[tauri::command]
pub fn set_ai_preferences(
    state: State<'_, AppState>,
    space_id: i64,
    preferences: AiPreferences,
) -> AppResult<AiPreferences> {
    let previous = ai::preferences(&state.db, space_id)?;
    ai::set_preferences(&state.db, space_id, &preferences)?;
    if previous.match_voice != preferences.match_voice {
        ai::rail_cache::clear_space(&state.db, space_id)?;
    }
    ai::preferences(&state.db, space_id)
}

#[tauri::command]
pub fn get_ai_voice_profile(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<Option<AiVoiceProfile>> {
    ai::voice::get_voice_profile(&state.db, space_id)
}

#[tauri::command]
pub fn clear_ai_voice_profile(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<()> {
    ai::voice::clear_voice_profile(&state.db, space_id)?;
    ai::rail_cache::clear_space(&state.db, space_id)
}

fn voice_samples(db: &Database, space_id: i64) -> AppResult<Vec<(String, String)>> {
    let messages = mail::list_messages(
        db,
        &MessageQuery {
            unified: Some("sent".into()),
            space_id: Some(space_id),
            limit: Some(12),
            ..MessageQuery::default()
        },
    )?;
    let mut samples = Vec::new();
    for message in messages {
        let body = if message.body_cached {
            mail::get_message_detail(db, message.id)
                .map(|detail| plain_body(&detail))
                .unwrap_or_else(|_| message.snippet.clone())
        } else {
            message.snippet.clone()
        };
        let body = body.trim();
        if body.chars().count() < 20 {
            continue;
        }
        samples.push((message.subject, body.to_string()));
        if samples.len() == 12 {
            break;
        }
    }
    Ok(samples)
}

pub(crate) async fn refresh_voice_profile(
    db: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    space_id: i64,
) -> AppResult<AiVoiceProfile> {
    let sample_db = Arc::clone(&db);
    let profile_db = Arc::clone(&db);
    let (samples, client) = tauri::async_runtime::spawn_blocking(move || {
        let samples = voice_samples(&sample_db, space_id)?;
        if samples.len() < 3 {
            return Err(AppError::Message("Not enough Sent mail yet".into()));
        }
        let (_, client) =
            ai::configured_client(&sample_db, secrets.as_ref(), space_id)?;
        Ok::<_, AppError>((samples, client))
    })
    .await
    .map_err(|_| blocking_error())??;

    let prompt = prompts::voice_profile_prompt(&samples);
    let raw = client.chat(&prompt).await?;
    let text: String = raw.trim().chars().take(2000).collect();
    if text.is_empty() {
        return Err(AppError::Message(
            "Could not build a writing style".into(),
        ));
    }
    let profile = AiVoiceProfile {
        text,
        updated_at: now(),
        sample_count: samples.len() as u32,
    };
    let saved = profile.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ai::voice::set_voice_profile(&profile_db, space_id, &saved)?;
        ai::rail_cache::clear_space(&profile_db, space_id)?;
        Ok::<_, AppError>(saved)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn learn_ai_voice(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<AiVoiceProfile> {
    refresh_voice_profile(
        Arc::clone(&state.db),
        Arc::clone(&state.secrets),
        space_id,
    )
    .await
}

#[tauri::command]
pub fn ai_has_key(
    state: State<'_, AppState>,
    space_id: i64,
    base_url: String,
) -> AppResult<bool> {
    Ok(ai::stored_key_for(
        &state.db,
        state.secrets.as_ref(),
        space_id,
        &base_url,
    )
    .is_some())
}

#[tauri::command]
pub async fn set_ai_config(
    state: State<'_, AppState>,
    space_id: i64,
    input: AiConfigInput,
) -> AppResult<AiConfigStatus> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    let index_db = Arc::clone(&state.db);
    let index_secrets = Arc::clone(&state.secrets);
    let status = tauri::async_runtime::spawn_blocking(move || {
        let previous = AiConfig::load(&db, space_id)?;
        let config = AiConfig {
            base_url: input.base_url.trim().to_string(),
            model: input.model.trim().to_string(),
            embedding_base_url: input.embedding_base_url.trim().to_string(),
            embedding_model: input.embedding_model.trim().to_string(),
            extra_headers: input
                .extra_headers
                .into_iter()
                .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
                .collect(),
        };
        config.save(&db, space_id)?;
        ai::set_enabled(&db, space_id, true)?;
        if previous.embedding_base_url != config.embedding_base_url
            || previous.embedding_model != config.embedding_model
        {
            db.conn.lock().unwrap().execute(
                "delete from message_embeddings where message_id in (
                    select m.id from messages m
                    where m.account_id in (select id from accounts where space_id = ?1)
                 )",
                rusqlite::params![space_id],
            )?;
        }
        let key = ai::key_name(space_id, &config.base_url);
        let owner_key = ai::scoped_key(space_id, ai::KEY_URL_KEY);
        let legacy_owner = db.setting(&owner_key).ok().flatten();
        match input.api_key.as_deref().map(str::trim) {
            Some("") => {
                secrets.delete(&key)?;
                if legacy_owner
                    .as_deref()
                    .map(|owner| owner.trim().trim_end_matches('/') == config.base_url.trim().trim_end_matches('/'))
                    .unwrap_or(false)
                {
                    let _ = secrets.delete(AI_KEY);
                    let _ = db.set_setting(&owner_key, "");
                }
            }
            Some(value) => {
                secrets.set(&key, value)?;
                if legacy_owner
                    .as_deref()
                    .map(|owner| owner.trim().trim_end_matches('/') == config.base_url.trim().trim_end_matches('/'))
                    .unwrap_or(false)
                {
                    let _ = secrets.delete(AI_KEY);
                    let _ = db.set_setting(&owner_key, "");
                }
            }
            None => {}
        }
        if !config.embedding_base_url.trim().is_empty() {
            let embedding_key = ai::key_name(space_id, &config.embedding_base_url);
            match input.embedding_api_key.as_deref().map(str::trim) {
                Some("") => {
                    secrets.delete(&embedding_key)?;
                }
                Some(value) => {
                    secrets.set(&embedding_key, value)?;
                }
                None => {}
            }
        }
        ai::status(&db, secrets.as_ref(), space_id)
    })
    .await
    .map_err(|_| blocking_error())??;
    ai::semantic::schedule_index(index_db, index_secrets, space_id);
    Ok(status)
}

#[tauri::command]
pub fn get_semantic_status(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<SemanticStatus> {
    ai::semantic::status(&state.db, space_id)
}

#[tauri::command]
pub fn start_semantic_index(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<SemanticStatus> {
    ai::semantic::schedule_index(
        Arc::clone(&state.db),
        Arc::clone(&state.secrets),
        space_id,
    );
    ai::semantic::status(&state.db, space_id)
}

#[tauri::command]
pub async fn test_ai(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<String> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    let client = tauri::async_runtime::spawn_blocking(move || {
        ai::configured_client(&db, secrets.as_ref(), space_id)
            .map(|(_, client)| client)
    })
    .await
    .map_err(|_| blocking_error())??;
    let messages = vec![ChatMessage::user(
        "Reply with the single word OK and nothing else.",
    )];
    client.chat_limited(&messages, 256).await?;
    Ok("ok".into())
}

#[tauri::command]
pub async fn ai_reply(
    state: State<'_, AppState>,
    message_id: i64,
    guidance: Option<String>,
    full_body: Option<bool>,
) -> AppResult<AiDraft> {
    let requested_full_body = full_body.unwrap_or(false);
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    let (mail, client, full_body) = tauri::async_runtime::spawn_blocking(move || {
        let space_id = space_for_message(&db, message_id)?;
        let full_body = effective_full_body(&db, space_id, requested_full_body)?;
        let message = mail::message_by_id(&db, message_id)?;
        let detail = if full_body {
            Some(mail::preload::load_detail(
                &db,
                secrets.as_ref(),
                message_id,
            )?)
        } else {
            None
        };
        let mail = reply_mail_context(&message, detail.as_ref(), full_body);
        let (_, client) = ai::configured_client(&db, secrets.as_ref(), space_id)?;
        Ok::<_, AppError>((mail, client, full_body))
    })
    .await
    .map_err(|_| blocking_error())??;

    let prompt = prompts::reply_prompt_with_privacy(&mail, guidance.as_deref(), full_body);
    let text = client.chat(&prompt).await?;
    Ok(AiDraft { text })
}

const RAIL_CACHE_VERSION: &str = "reply-only-v1";

fn aura_rail_cache_model(model: &str) -> String {
    format!("{model}::{RAIL_CACHE_VERSION}")
}

enum PreparedRail {
    Cached(AiAuraRail),
    Generate {
        thread: Vec<(MailContext, bool)>,
        client: ai::client::AiClient,
        total: u32,
        latest_id: i64,
        latest_date: i64,
        model: String,
        full_body: bool,
        voice: Option<String>,
    },
}

pub(crate) async fn generate_aura_rail(
    db: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    message_id: i64,
    answer: Option<String>,
    requested_full_body: bool,
) -> AppResult<AiAuraRail> {
    let answer = answer.filter(|value| !value.trim().is_empty());
    let cacheable = answer.is_none();
    let cache_db = Arc::clone(&db);
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        let message = mail::message_by_id(&cache_db, message_id)?;
        let space_id = accounts::space_for_account(&cache_db, message.account_id)?;
        let mut thread_messages =
            mail::list_thread_messages(&cache_db, message.account_id, &message.thread_key)?;
        if !thread_messages.iter().any(|item| item.id == message_id) {
            thread_messages.push(message.clone());
        }
        let total = thread_messages.len() as u32;
        let (latest_date, latest_id) = thread_messages
            .iter()
            .map(|item| (item.date, item.id))
            .max()
            .unwrap_or((0, 0));
        let full_body = effective_full_body(&cache_db, space_id, requested_full_body)?;
        let config = AiConfig::load(&cache_db, space_id)?;
        if cacheable {
            let cache_model = aura_rail_cache_model(&config.model);
            if let Some(cached) = ai::rail_cache::load(
                &cache_db,
                message_id,
                &cache_model,
                full_body,
                latest_id,
                latest_date,
            )? {
                return Ok(PreparedRail::Cached(cached));
            }
        }
        let window = ai::aura::thread_window(&thread_messages, message_id);
        let mut thread = Vec::with_capacity(window.len());
        for item in window {
            let body = if full_body {
                mail::get_message_detail(&cache_db, item.id)
                    .map(|detail| plain_body(&detail))
                    .unwrap_or_else(|_| item.snippet.clone())
            } else {
                item.snippet.clone()
            };
            thread.push((mail_context(&item, body), item.id == message_id));
        }
        let voice = ai::voice::voice_prompt_text(&cache_db, space_id)?;
        let (_, client) =
            ai::configured_client(&cache_db, secrets.as_ref(), space_id)?;
        Ok::<_, AppError>(PreparedRail::Generate {
            thread,
            client,
            total,
            latest_id,
            latest_date,
            model: config.model,
            full_body,
            voice,
        })
    })
    .await
    .map_err(|_| blocking_error())??;

    match prepared {
        PreparedRail::Cached(cached) => Ok(cached),
        PreparedRail::Generate {
            thread,
            client,
            total,
            latest_id,
            latest_date,
            model,
            full_body,
            voice,
        } => {
            let prompt = prompts::aura_rail_prompt(&thread, answer.as_deref(), voice.as_deref());
            let raw = client.chat(&prompt).await?;
            let rail = ai::aura::parse_aura_rail(&raw, total, answer.is_some())?;
            if cacheable {
                ai::rail_cache::store(
                    &db,
                    message_id,
                    &aura_rail_cache_model(&model),
                    full_body,
                    latest_id,
                    latest_date,
                    &rail,
                )?;
            }
            Ok(rail)
        }
    }
}

#[tauri::command]
pub async fn ai_aura_rail(
    state: State<'_, AppState>,
    message_id: i64,
    answer: Option<String>,
    full_body: Option<bool>,
) -> AppResult<AiAuraRail> {
    generate_aura_rail(
        Arc::clone(&state.db),
        Arc::clone(&state.secrets),
        message_id,
        answer,
        full_body.unwrap_or(false),
    )
    .await
}

#[tauri::command]
pub async fn ai_aura_ask(
    state: State<'_, AppState>,
    message_id: i64,
    question: String,
    history: Vec<ChatMessage>,
    existing: Vec<String>,
    full_body: Option<bool>,
) -> AppResult<AiAuraAskResponse> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err(AppError::Message("question is required".into()));
    }
    let requested_full_body = full_body.unwrap_or(false);
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    let (thread, client, voice, thread_key, full_body, embedding, space_id) =
        tauri::async_runtime::spawn_blocking(move || {
            let message = mail::message_by_id(&db, message_id)?;
            let space_id = accounts::space_for_account(&db, message.account_id)?;
            let thread_key = message.thread_key.clone();
            let mut thread_messages =
                mail::list_thread_messages(&db, message.account_id, &message.thread_key)?;
            if !thread_messages.iter().any(|item| item.id == message_id) {
                thread_messages.push(message);
            }
            let full_body = effective_full_body(&db, space_id, requested_full_body)?;
            let window = ai::aura::thread_window(&thread_messages, message_id);
            let mut thread = Vec::with_capacity(window.len());
            for item in window {
                let body = if full_body {
                    mail::get_message_detail(&db, item.id)
                        .map(|detail| plain_body(&detail))
                        .unwrap_or_else(|_| item.snippet.clone())
                } else {
                    item.snippet.clone()
                };
                thread.push((mail_context(&item, body), item.id == message_id));
            }
            let voice = ai::voice::voice_prompt_text(&db, space_id)?;
            let (_, client) =
                ai::configured_client(&db, secrets.as_ref(), space_id)?;
            let embedding =
                ai::configured_embedding_client(&db, secrets.as_ref(), space_id).ok();
            Ok::<_, AppError>((
                thread,
                client,
                voice,
                thread_key,
                full_body,
                embedding,
                space_id,
            ))
        })
        .await
        .map_err(|_| blocking_error())??;

    let mut related: Vec<MailContext> = Vec::new();
    if let Some((embedding_model, embedding_client)) = embedding {
        if let Ok(mut vectors) = embedding_client
            .embed(&embedding_model, std::slice::from_ref(&question))
            .await
        {
            if let Some(query) = vectors.pop() {
                let retrieval_db = Arc::clone(&state.db);
                let model = embedding_model.clone();
                let exclude_thread = thread_key.clone();
                related = tauri::async_runtime::spawn_blocking(move || {
                    let messages = ai::semantic::related(
                        &retrieval_db,
                        space_id,
                        &model,
                        &query,
                        Some(&exclude_thread),
                        4,
                    )?;
                    let mut related = Vec::with_capacity(messages.len());
                    for item in messages {
                        let body = if full_body {
                            mail::get_message_detail(&retrieval_db, item.id)
                                .map(|detail| plain_body(&detail))
                                .unwrap_or_else(|_| item.snippet.clone())
                        } else {
                            item.snippet.clone()
                        };
                        related.push(mail_context(&item, body));
                    }
                    Ok::<_, AppError>(related)
                })
                .await
                .map_err(|_| blocking_error())?
                .unwrap_or_default();
            }
        }
    }

    let prompt = prompts::aura_ask_prompt(
        &thread,
        &existing,
        &question,
        &history,
        voice.as_deref(),
        &related,
    );
    let raw = client.chat(&prompt).await?;
    ai::aura::parse_aura_ask(&raw)
}

#[tauri::command]
pub async fn ai_compose(
    state: State<'_, AppState>,
    space_id: i64,
    request: AiComposeRequest,
) -> AppResult<AiComposeResponse> {
    let mode = request.mode.trim().to_ascii_lowercase();
    if mode != "rewrite" && mode != "generate" {
        return Err(AppError::Message("AI compose mode is invalid".into()));
    }
    if mode == "generate" && request.instruction.trim().is_empty() {
        return Err(AppError::Message("Describe what to generate first".into()));
    }
    if mode == "rewrite" && request.text.trim().is_empty() {
        return Err(AppError::Message(
            "Write something before rewriting it".into(),
        ));
    }
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    let client = tauri::async_runtime::spawn_blocking(move || {
        ai::configured_client(&db, secrets.as_ref(), space_id)
            .map(|(_, client)| client)
    })
    .await
    .map_err(|_| blocking_error())??;
    let prompt =
        prompts::compose_prompt(&mode, &request.instruction, &request.text, &request.subject);
    let text = client.chat(&prompt).await?;
    Ok(AiComposeResponse { text })
}

#[tauri::command]
pub async fn list_contacts(
    state: State<'_, AppState>,
    query: Option<String>,
    limit: Option<usize>,
) -> AppResult<Vec<Contact>> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || {
        contacts::list_contacts(&db, query.as_deref(), limit)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn open_compose_window(
    app: AppHandle,
    mode: String,
    message_id: Option<i64>,
    draft_id: Option<i64>,
) -> AppResult<()> {
    let mode = match mode.trim() {
        "new" | "reply" | "reply-all" | "forward" => mode.trim(),
        _ => "new",
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let label = format!("compose-{stamp}");
    let mut query = format!("window=compose&mode={mode}");
    if let Some(message_id) = message_id {
        query.push_str(&format!("&messageId={message_id}"));
    }
    if let Some(draft_id) = draft_id {
        query.push_str(&format!("&draftId={draft_id}"));
    }
    WebviewWindowBuilder::new(
        &app,
        label,
        WebviewUrl::App(format!("index.html#compose?{query}").into()),
    )
    .title("AuraMail — Compose")
    .inner_size(760.0, 720.0)
    .min_inner_size(520.0, 500.0)
    .center()
    .disable_drag_drop_handler()
    .build()
    .map_err(|error| AppError::Message(format!("could not open compose window: {error}")))?;
    Ok(())
}

#[tauri::command]
pub async fn ai_chat(
    state: State<'_, AppState>,
    space_id: i64,
    request: AiChatRequest,
) -> AppResult<AiChatResponse> {
    let question = request.question.trim().to_string();
    if question.is_empty() {
        return Err(AppError::Message("question is required".into()));
    }
    let context_kind = request.context;
    let message_id = request.message_id;
    let requested_full_body = request.full_body;
    let history = request.history;

    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    let (context, client) = tauri::async_runtime::spawn_blocking(move || {
        let ai_space = if matches!(context_kind.as_str(), "email" | "thread") {
            let message_id = message_id.ok_or_else(|| {
                AppError::Message("message is required for this context".into())
            })?;
            space_for_message(&db, message_id)?
        } else {
            space_id
        };
        let (_, client) = ai::configured_client(&db, secrets.as_ref(), ai_space)?;
        let full_body = effective_full_body(&db, ai_space, requested_full_body)?;
        let context = match context_kind.as_str() {
            "email" => {
                let message_id = message_id.ok_or_else(|| {
                    AppError::Message("message is required for email context".into())
                })?;
                let message = mail::message_by_id(&db, message_id)?;
                let body = if full_body {
                    mail::preload::load_detail(&db, secrets.as_ref(), message_id)
                        .map(|detail| plain_body(&detail))
                        .unwrap_or_else(|_| message.snippet.clone())
                } else {
                    message.snippet.clone()
                };
                prompts::email_context(&mail_context(&message, body), full_body)
            }
            "thread" => {
                let message_id = message_id.ok_or_else(|| {
                    AppError::Message("message is required for thread context".into())
                })?;
                let message = mail::message_by_id(&db, message_id)?;
                let mut thread_messages =
                    mail::list_thread_messages(&db, message.account_id, &message.thread_key)?;
                if !thread_messages.iter().any(|item| item.id == message_id) {
                    thread_messages.push(message);
                }
                let window = ai::aura::thread_window(&thread_messages, message_id);
                let mut mails = Vec::with_capacity(window.len());
                for item in window {
                    let body = if full_body {
                        mail::get_message_detail(&db, item.id)
                            .map(|detail| plain_body(&detail))
                            .unwrap_or_else(|_| item.snippet.clone())
                    } else {
                        item.snippet.clone()
                    };
                    mails.push(mail_context(&item, body));
                }
                prompts::thread_context(&mails)
            }
            "inbox" => {
                let messages = mail::list_messages(
                    &db,
                    &MessageQuery {
                        space_id: Some(ai_space),
                        limit: Some(30),
                        ..MessageQuery::default()
                    },
                )?;
                prompts::inbox_context(&messages, false)
            }
            "none" => String::new(),
            _ => return Err(AppError::Message("AI context is invalid".into())),
        };
        Ok::<_, AppError>((context, client))
    })
    .await
    .map_err(|_| blocking_error())??;

    let prompt = prompts::ask_prompt_with_history(&context, &question, &history);
    let answer = client.chat(&prompt).await?;
    Ok(AiChatResponse { answer })
}

#[tauri::command]
pub async fn add_account(state: State<'_, AppState>, input: AccountInput) -> AppResult<Account> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        input.validate()?;
        let imap_security = MailSecurity::parse(&input.imap_security)?;
        let smtp_security = MailSecurity::parse(&input.smtp_security)?;
        let credential = MailCredential::Password(input.password.clone());
        validate_imap(
            &input.imap_host,
            input.imap_port,
            imap_security,
            &input.username,
            &credential,
        )?;
        validate_smtp(
            &input.smtp_host,
            input.smtp_port,
            smtp_security,
            &input.username,
            &credential,
        )?;
        accounts::create(&db, secrets.as_ref(), input)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub fn set_account_color(
    state: State<'_, AppState>,
    id: i64,
    color: String,
) -> AppResult<Account> {
    accounts::set_color(&state.db, id, &color)
}

#[tauri::command]
pub async fn update_account(
    state: State<'_, AppState>,
    id: i64,
    input: AccountInput,
) -> AppResult<Account> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        let existing = accounts::list(&db)?
            .into_iter()
            .find(|account| account.id == id)
            .ok_or_else(|| AppError::Message("account not found".into()))?;
        if existing.auth_type == AccountAuth::Proton {
            return accounts::update_proton_profile(
                &db,
                id,
                &input.display_name,
                &input.color,
                &input.signature,
                &input.signature_fields,
                input.signature_on_new,
                input.space_id,
            );
        }
        if existing.auth_type == AccountAuth::OAuth {
            let updated = accounts::update_profile(
                &db,
                id,
                &input.display_name,
                &input.color,
                &input.signature,
                &input.signature_fields,
                input.signature_on_new,
            )?;
            if existing.space_id != input.space_id {
                return accounts::move_to_space(&db, id, input.space_id);
            }
            return Ok(updated);
        }
        input.validate_update()?;
        let connection_changed = !input.password.trim().is_empty()
            || input.username.trim() != existing.username
            || input.imap_host.trim() != existing.imap_host
            || input.imap_port != existing.imap_port
            || input.imap_security != existing.imap_security
            || input.smtp_host.trim() != existing.smtp_host
            || input.smtp_port != existing.smtp_port
            || input.smtp_security != existing.smtp_security;
        if connection_changed {
            let credential = if input.password.trim().is_empty() {
                crate::mail_auth::credential_for(&existing, secrets.as_ref())?
            } else {
                MailCredential::Password(input.password.clone())
            };
            let imap_security = MailSecurity::parse(&input.imap_security)?;
            let smtp_security = MailSecurity::parse(&input.smtp_security)?;
            validate_imap(
                &input.imap_host,
                input.imap_port,
                imap_security,
                &input.username,
                &credential,
            )?;
            validate_smtp(
                &input.smtp_host,
                input.smtp_port,
                smtp_security,
                &input.username,
                &credential,
            )?;
        }
        accounts::update(&db, secrets.as_ref(), id, input)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn discover_account_settings(username: String) -> AppResult<accounts::AccountDiscovery> {
    accounts::discover(&username).await
}

fn delete_account_record(
    db: &Database,
    secrets: &dyn SecretStore,
    id: i64,
) -> AppResult<()> {
    let account = accounts::list(db)?
        .into_iter()
        .find(|account| account.id == id)
        .ok_or_else(|| AppError::Message("account not found".into()))?;
    if account.auth_type == AccountAuth::Proton {
        // Fail before mutating the database or keyring if a sign-in owns the
        // auth lock; the account must stay intact for a retry.
        crate::proton::runtime::forget_account(&account.username)?;
    }
    let refresh_token = oauth::account::oauth_refresh_token(&account, secrets);
    accounts::delete(db, secrets, id)?;
    match account.auth_type {
        AccountAuth::OAuth => {
            if let Some(token) = refresh_token {
                oauth::account::revoke_account_token(&account, &token);
            }
        }
        AccountAuth::Proton => {
            if let Err(error) = crate::proton::runtime::reconcile() {
                eprintln!("Proton bridge reconcile failed after account deletion: {error}");
            }
        }
        AccountAuth::Password => {}
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_account(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        delete_account_record(&db, secrets.as_ref(), id)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn start_proton_login(request: ProtonAuthStart) -> AppResult<ProtonAuthStatus> {
    let bridge = crate::proton::runtime::current()?;
    tauri::async_runtime::spawn_blocking(move || bridge.start_login(request))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn continue_proton_login(
    login_id: String,
    response: ProtonAuthResponse,
) -> AppResult<ProtonAuthStatus> {
    let bridge = crate::proton::runtime::current()?;
    tauri::async_runtime::spawn_blocking(move || bridge.continue_login(&login_id, response))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn cancel_proton_login(login_id: Option<String>) -> AppResult<()> {
    let bridge = crate::proton::runtime::current()?;
    tauri::async_runtime::spawn_blocking(move || match login_id {
        Some(login_id) => bridge.cancel_login(&login_id),
        None => bridge.cancel_active_login(),
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub fn set_sync_interval(state: State<'_, AppState>, seconds: i64) -> AppResult<Vec<Account>> {
    accounts::set_sync_interval(&state.db, seconds)
}

#[tauri::command]
pub fn get_desktop_preferences(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<crate::desktop::DesktopPreferences> {
    crate::desktop::get_for_space(&state.db, space_id)
}

#[tauri::command]
pub fn set_desktop_preferences(
    state: State<'_, AppState>,
    space_id: i64,
    preferences: crate::desktop::DesktopPreferences,
) -> AppResult<crate::desktop::DesktopPreferences> {
    crate::desktop::set_for_space(&state.db, space_id, preferences)
}

#[tauri::command]
pub fn set_active_space(state: State<'_, AppState>, space_id: i64) -> AppResult<()> {
    crate::spaces::by_id(&state.db, space_id)?;
    crate::desktop::set_active_space(space_id);
    Ok(())
}

#[tauri::command]
pub async fn refresh_tray_unread(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || {
        crate::desktop::refresh_tray_unread(&app, &db);
        Ok(())
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub fn get_fast_sync_interval(state: State<'_, AppState>) -> AppResult<i64> {
    accounts::fast_sync_interval(&state.db)
}

#[tauri::command]
pub fn set_fast_sync_interval(state: State<'_, AppState>, seconds: i64) -> AppResult<i64> {
    accounts::set_fast_sync_interval(&state.db, seconds)
}

#[tauri::command]
pub async fn sync_account(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: i64,
) -> AppResult<()> {
    let sync_db = Arc::clone(&state.db);
    let sync_secrets = Arc::clone(&state.secrets);
    let syncing = Arc::clone(&state.syncing);
    let sync_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        sync::sync_account(&sync_app, sync_db, sync_secrets, &syncing, account_id)
    })
    .await
    .map_err(|_| blocking_error())??;
    let recent: Vec<i64> = result
        .inserted
        .iter()
        .rev()
        .take(100)
        .rev()
        .copied()
        .collect();
    if !recent.is_empty() {
        mail::preload::schedule_preload(
            Arc::clone(&state.db),
            Arc::clone(&state.secrets),
            recent.clone(),
        );
        if let Ok(space_id) = accounts::space_for_account(&state.db, account_id) {
            sync::schedule_auto_rail(
                Arc::clone(&state.db),
                Arc::clone(&state.secrets),
                space_id,
                recent,
            );
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn list_calendars(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<Vec<CalendarSummary>> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || calendar::list_calendars(&db, space_id))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn list_calendar_events(
    state: State<'_, AppState>,
    space_id: i64,
    from: i64,
    to: i64,
    limit: Option<usize>,
) -> AppResult<Vec<CalendarEvent>> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || {
        calendar::list_events(&db, space_id, from, to, limit.unwrap_or(200))
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn list_calendar_accounts(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<Vec<CalendarAccount>> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || calendar::list_accounts(&db, space_id))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn reorder_calendar_accounts(
    state: State<'_, AppState>,
    space_id: i64,
    ids: Vec<i64>,
) -> AppResult<Vec<CalendarAccount>> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || {
        calendar::reorder_accounts(&db, space_id, &ids)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn move_calendar_account(
    state: State<'_, AppState>,
    account_id: i64,
    space_id: i64,
) -> AppResult<CalendarAccount> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || {
        calendar::move_account_to_space(&db, account_id, space_id)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn add_calendar_account(
    state: State<'_, AppState>,
    input: CalendarAccountInput,
) -> AppResult<CalendarAccount> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    calendar::add_account(&db, secrets.as_ref(), input).await
}

#[tauri::command]
pub async fn unsubscribe_calendar(
    state: State<'_, AppState>,
    calendar_id: i64,
) -> AppResult<()> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || calendar::unsubscribe_calendar(&db, calendar_id))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn remove_calendar_account(
    state: State<'_, AppState>,
    account_id: i64,
) -> AppResult<()> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        calendar::remove_account(&db, secrets.as_ref(), account_id)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn sync_calendar(
    state: State<'_, AppState>,
    account_id: i64,
) -> AppResult<CalendarSyncResult> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    calendar::sync_account(&db, secrets.as_ref(), account_id).await
}

#[tauri::command]
pub async fn create_calendar_event(
    state: State<'_, AppState>,
    calendar_id: i64,
    input: CalendarEventInput,
) -> AppResult<CalendarSyncResult> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    calendar::create_event(&db, secrets.as_ref(), calendar_id, input).await
}

#[tauri::command]
pub async fn update_calendar_event(
    state: State<'_, AppState>,
    event_id: i64,
    input: CalendarEventInput,
) -> AppResult<CalendarSyncResult> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    calendar::update_event(&db, secrets.as_ref(), event_id, input).await
}

#[tauri::command]
pub async fn delete_calendar_event(
    state: State<'_, AppState>,
    event_id: i64,
) -> AppResult<CalendarSyncResult> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    calendar::delete_event(&db, secrets.as_ref(), event_id).await
}

#[tauri::command]
pub async fn list_folders(
    state: State<'_, AppState>,
    account_id: Option<i64>,
    space_id: Option<i64>,
) -> AppResult<Vec<Folder>> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || mail::list_folders(&db, account_id, space_id))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn list_tabs(state: State<'_, AppState>, space_id: i64) -> AppResult<Vec<Tab>> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || tabs::list(&db, space_id))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn tab_unread_counts(
    state: State<'_, AppState>,
    folder_id: Option<i64>,
    account_id: Option<i64>,
    space_id: Option<i64>,
    unified: Option<String>,
) -> AppResult<TabUnreadCounts> {
    let db = Arc::clone(&state.db);
    let query = MessageQuery {
        folder_id,
        account_id,
        space_id,
        unified,
        ..MessageQuery::default()
    };
    tauri::async_runtime::spawn_blocking(move || mail::tab_unread_counts(&db, &query))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub fn create_tab(
    state: State<'_, AppState>,
    space_id: i64,
    name: String,
    description: String,
    color: Option<String>,
    icon: Option<String>,
) -> AppResult<Tab> {
    tabs::create(
        &state.db,
        space_id,
        &name,
        &description,
        color.as_deref(),
        icon.as_deref(),
    )
}

#[tauri::command]
pub fn update_tab(
    state: State<'_, AppState>,
    space_id: i64,
    id: i64,
    name: String,
    description: String,
    color: Option<String>,
    icon: Option<String>,
) -> AppResult<Tab> {
    tabs::update(
        &state.db,
        space_id,
        id,
        &name,
        &description,
        color.as_deref(),
        icon.as_deref(),
    )
}

#[tauri::command]
pub fn delete_tab(state: State<'_, AppState>, space_id: i64, id: i64) -> AppResult<()> {
    tabs::delete(&state.db, space_id, id)
}

#[tauri::command]
pub fn reorder_tabs(
    state: State<'_, AppState>,
    space_id: i64,
    ids: Vec<i64>,
) -> AppResult<Vec<Tab>> {
    tabs::reorder(&state.db, space_id, &ids)
}

#[tauri::command]
pub async fn classify_message(
    state: State<'_, AppState>,
    message_id: i64,
    reclassify: Option<bool>,
) -> AppResult<Classification> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    classification::classify(&db, secrets.as_ref(), message_id, reclassify.unwrap_or(false)).await
}

#[tauri::command]
pub async fn classify_messages(
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> AppResult<Vec<ClassificationOutcome>> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    let mut outcomes = Vec::with_capacity(ids.len());
    for message_id in ids {
        let result = classification::classify(&db, secrets.as_ref(), message_id, false).await;
        outcomes.push(match result {
            Ok(classification) => ClassificationOutcome {
                message_id,
                classification: Some(classification),
                error: None,
                error_kind: None,
            },
            Err(error) => ClassificationOutcome {
                message_id,
                classification: None,
                error: Some(error.to_string()),
                error_kind: Some(error_kind(&error).into()),
            },
        });
    }
    Ok(outcomes)
}

#[tauri::command]
pub async fn classify_unclassified(
    state: State<'_, AppState>,
    account_id: Option<i64>,
    folder_id: Option<i64>,
    space_id: Option<i64>,
) -> AppResult<(u32, u32)> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    classification::classify_backlog_in_space(
        &db,
        secrets.as_ref(),
        account_id,
        folder_id,
        space_id,
        25,
    )
    .await
}

#[tauri::command(rename_all = "snake_case")]
#[allow(clippy::too_many_arguments)]
pub async fn list_messages(
    state: State<'_, AppState>,
    folder_id: Option<i64>,
    account_id: Option<i64>,
    space_id: Option<i64>,
    tab_id: Option<i64>,
    unified: Option<String>,
    search: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    order: Option<String>,
) -> AppResult<Vec<Message>> {
    let db = Arc::clone(&state.db);
    let query = MessageQuery {
        folder_id,
        account_id,
        space_id,
        tab_id,
        unified,
        search,
        limit,
        offset,
        order,
    };
    tauri::async_runtime::spawn_blocking(move || mail::list_messages(&db, &query))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command(rename_all = "snake_case")]
#[allow(clippy::too_many_arguments)]
pub async fn list_threads(
    state: State<'_, AppState>,
    folder_id: Option<i64>,
    account_id: Option<i64>,
    space_id: Option<i64>,
    tab_id: Option<i64>,
    unified: Option<String>,
    search: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    order: Option<String>,
) -> AppResult<Vec<ThreadMessage>> {
    let db = Arc::clone(&state.db);
    let query = MessageQuery {
        folder_id,
        account_id,
        space_id,
        tab_id,
        unified,
        search,
        limit,
        offset,
        order,
    };
    tauri::async_runtime::spawn_blocking(move || mail::list_threads(&db, &query))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub fn list_thread_messages(
    state: State<'_, AppState>,
    account_id: i64,
    thread_key: String,
) -> AppResult<Vec<Message>> {
    mail::list_thread_messages(&state.db, account_id, &thread_key)
}

#[tauri::command]
pub async fn get_message(state: State<'_, AppState>, message_id: i64) -> AppResult<MessageDetail> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        mail::preload::load_detail(&db, secrets.as_ref(), message_id)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn mark_read(
    state: State<'_, AppState>,
    message_id: i64,
    read: bool,
) -> AppResult<Message> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        let (message, folder, account) = message_context(&db, message_id)?;
        let credential = crate::mail_auth::credential_for(&account, secrets.as_ref())?;
        client::set_seen(&account, &credential, &folder.path, message.uid, read)?;
        mail::set_message_seen(&db, message_id, read)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn toggle_star(state: State<'_, AppState>, message_id: i64) -> AppResult<Message> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        let (message, folder, account) = message_context(&db, message_id)?;
        let credential = crate::mail_auth::credential_for(&account, secrets.as_ref())?;
        let starred = !message.is_starred;
        client::set_starred(&account, &credential, &folder.path, message.uid, starred)?;
        mail::set_message_starred(&db, message_id, starred)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn trash_message(state: State<'_, AppState>, message_id: i64) -> AppResult<()> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        let (message, folder, account) = message_context(&db, message_id)?;
        let credential = crate::mail_auth::credential_for(&account, secrets.as_ref())?;
        let trash = trash_destination(&folder.role, mail::trash_folder_path(&db, account.id)?)?;
        client::move_to_trash(&account, &credential, &folder.path, message.uid, &trash)?;
        mail::remove_message(&db, message_id)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn move_message_to_folder(
    state: State<'_, AppState>,
    message_id: i64,
    folder_id: i64,
) -> AppResult<()> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        let (message, folder, account) = message_context(&db, message_id)?;
        let target = mail::folder_by_id(&db, folder_id)?
            .ok_or_else(|| AppError::Message("target folder not found".into()))?;
        validate_move_target(&folder, &target)?;
        let credential = crate::mail_auth::credential_for(&account, secrets.as_ref())?;
        client::move_to_folder(&account, &credential, &folder.path, message.uid, &target.path)?;
        mail::remove_message(&db, message_id)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub fn set_message_classification(
    state: State<'_, AppState>,
    message_id: i64,
    tab_id: Option<i64>,
) -> AppResult<Classification> {
    classification::set_manual(&state.db, message_id, tab_id)
}

#[tauri::command]
pub async fn fill_message_snippets(
    state: State<'_, AppState>,
    message_ids: Vec<i64>,
) -> AppResult<Vec<MessageSnippet>> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        type SnippetGroup = (Account, Folder, Vec<(i64, u32)>);
        let mut groups: HashMap<(i64, i64), SnippetGroup> = HashMap::new();
        for message_id in message_ids.into_iter().take(80) {
            let (message, folder, account) = message_context(&db, message_id)?;
            let entry = groups
                .entry((account.id, folder.id))
                .or_insert_with(|| (account, folder, Vec::new()));
            entry.2.push((message_id, message.uid));
        }
        let mut out = Vec::new();
        for (_, (account, folder, items)) in groups {
            if items.is_empty() {
                continue;
            }
            let Ok(credential) = crate::mail_auth::credential_for(&account, secrets.as_ref()) else {
                continue;
            };
            let Ok(mut session) = client::open_session(&account, &credential) else {
                continue;
            };
            if session.select(&folder.path).is_err() {
                let _ = session.logout();
                continue;
            }
            let by_uid: HashMap<u32, i64> = items.iter().map(|(id, uid)| (*uid, *id)).collect();
            let uids = items
                .iter()
                .map(|(_, uid)| uid.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let fetches =
                match session.uid_fetch(uids, "(UID BODY.PEEK[HEADER] BODY.PEEK[TEXT]<0.8192>)") {
                    Ok(fetches) => fetches,
                    Err(_) => {
                        let _ = session.logout();
                        continue;
                    }
                };
            for fetch in fetches.iter() {
                let (Some(uid), Some(header)) = (fetch.uid, fetch.header()) else {
                    continue;
                };
                let Some(message_id) = by_uid.get(&uid).copied() else {
                    continue;
                };
                let preview = fetch.text().unwrap_or_default();
                let snippet = parse::parse_headers_with_preview(header, preview).snippet;
                if snippet.trim().is_empty() {
                    continue;
                }
                if mail::set_message_snippet(&db, message_id, &snippet).is_ok() {
                    out.push(MessageSnippet {
                        message_id,
                        snippet,
                    });
                }
            }
            let _ = session.logout();
        }
        Ok(out)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn archive_message(state: State<'_, AppState>, message_id: i64) -> AppResult<()> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        let (message, folder, account) = message_context(&db, message_id)?;
        let credential = crate::mail_auth::credential_for(&account, secrets.as_ref())?;
        let archive = mail::archive_folder_path(&db, account.id)?;
        let all = mail::all_folder_path(&db, account.id)?;
        client::archive(
            &account,
            &credential,
            &folder.path,
            message.uid,
            archive.as_deref(),
            all.as_deref(),
        )?;
        mail::remove_message(&db, message_id)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn archive_thread(
    state: State<'_, AppState>,
    account_id: i64,
    thread_key: String,
) -> AppResult<()> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        let ids = mail::thread_message_ids_in_role(&db, account_id, &thread_key, "inbox")?;
        if ids.is_empty() {
            return Ok(());
        }
        let account = account_by_id(&db, account_id)?;
        let credential = crate::mail_auth::credential_for(&account, secrets.as_ref())?;
        let archive = mail::archive_folder_path(&db, account_id)?;
        let all = mail::all_folder_path(&db, account_id)?;
        for message_id in ids {
            let (message, folder, _) = message_context(&db, message_id)?;
            client::archive(
                &account,
                &credential,
                &folder.path,
                message.uid,
                archive.as_deref(),
                all.as_deref(),
            )?;
            mail::remove_message(&db, message_id)?;
        }
        Ok(())
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn delete_message(state: State<'_, AppState>, message_id: i64) -> AppResult<()> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        let (message, folder, account) = message_context(&db, message_id)?;
        ensure_trash_delete(&folder.role)?;
        let credential = crate::mail_auth::credential_for(&account, secrets.as_ref())?;
        client::delete_permanently(&account, &credential, &folder.path, message.uid)?;
        mail::remove_message(&db, message_id)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    input: ComposeInput,
    draft_id: Option<i64>,
) -> AppResult<()> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        let account = account_by_id(&db, input.account_id)?;
        let credential = crate::mail_auth::credential_for(&account, secrets.as_ref())?;
        smtp::send(&account, &credential, &input)?;
        crate::desktop::play_sent_if_enabled(&db, account.space_id);
        if let Some(draft_id) = draft_id {
            let _ = compose::delete_draft(&db, draft_id);
        }
        Ok(())
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn save_draft(
    state: State<'_, AppState>,
    input: ComposeInput,
    draft_id: Option<i64>,
) -> AppResult<Draft> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || compose::save_draft(&db, &input, draft_id))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn list_drafts(
    state: State<'_, AppState>,
    account_id: Option<i64>,
    space_id: Option<i64>,
) -> AppResult<Vec<DraftSummary>> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || {
        compose::list_drafts_for_space(&db, account_id, space_id)
    })
    .await
    .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn get_draft(state: State<'_, AppState>, draft_id: i64) -> AppResult<Draft> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || compose::get_draft(&db, draft_id))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn delete_draft(state: State<'_, AppState>, draft_id: i64) -> AppResult<()> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || compose::delete_draft(&db, draft_id))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn get_sender_avatar(
    state: State<'_, AppState>,
    email: String,
    space_id: i64,
) -> AppResult<Option<String>> {
    let avatar = crate::avatars::resolve(&state.db, &email, space_id).await?;
    Ok(avatar.map(|avatar| crate::avatars::data_url(&avatar)))
}

#[tauri::command]
pub async fn load_remote_images(state: State<'_, AppState>, message_id: i64) -> AppResult<String> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        mail::preload::load_detail(&db, secrets.as_ref(), message_id)
    })
    .await
    .map_err(|_| blocking_error())??;
    mail::preload::load_images(&state.db, message_id).await
}

#[tauri::command]
pub async fn download_attachment(
    app: AppHandle,
    state: State<'_, AppState>,
    attachment_id: i64,
) -> AppResult<String> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        let attachment = mail::attachment_by_id(&db, attachment_id)?;
        if attachment.size > MAX_ATTACHMENT_SIZE {
            return Err(AppError::Message("attachment is too large".into()));
        }
        let download_dir = app
            .path()
            .download_dir()
            .map_err(|_| AppError::Message("download directory is unavailable".into()))?;
        std::fs::create_dir_all(&download_dir)?;

        if let Some(cached) = mail::attachment_cache_path(&db, attachment_id)? {
            let cached = PathBuf::from(cached);
            if cached.parent() == Some(download_dir.as_path()) {
                if let Some(cached) = cached_attachment(&cached)? {
                    return Ok(cached);
                }
            }
        }

        let filename = mail::safe_attachment_name(&attachment.filename);
        let path = unique_download_path(&download_dir, &filename);
        let (_, bytes) = fetch_attachment_bytes(&db, secrets.as_ref(), attachment_id)?;
        std::fs::write(&path, &bytes)?;
        mail::set_attachment_cache_path(&db, attachment_id, &path.to_string_lossy())?;
        Ok(path.to_string_lossy().to_string())
    })
    .await
    .map_err(|_| blocking_error())?
}

fn unique_download_path(dir: &Path, filename: &str) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }

    let name = Path::new(filename);
    let stem = name
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("attachment");
    let extension = name.extension().and_then(|value| value.to_str());
    for index in 1.. {
        let next = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = dir.join(next);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

#[tauri::command]
pub async fn attachment_preview(
    state: State<'_, AppState>,
    attachment_id: i64,
) -> AppResult<Option<AttachmentPreview>> {
    let db = Arc::clone(&state.db);
    let secrets = Arc::clone(&state.secrets);
    tauri::async_runtime::spawn_blocking(move || {
        let attachment = mail::attachment_by_id(&db, attachment_id)?;
        if !previewable_image(&attachment.mime_type, attachment.size) {
            return Ok(None);
        }
        let (attachment, bytes) = fetch_attachment_bytes(&db, secrets.as_ref(), attachment_id)?;
        if bytes.len() as i64 > MAX_ATTACHMENT_PREVIEW_SIZE {
            return Ok(None);
        }
        Ok(Some(AttachmentPreview {
            mime_type: attachment.mime_type.clone(),
            data_url: preview_data_url(&attachment.mime_type, &bytes),
        }))
    })
    .await
    .map_err(|_| blocking_error())?
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailPreferences {
    pub trusted_senders: Vec<String>,
    pub always_load_images: bool,
    pub profile_pictures: bool,
}

#[tauri::command]
pub async fn get_mail_preferences(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<MailPreferences> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || mail_preferences(&db, space_id))
        .await
        .map_err(|_| blocking_error())?
}

#[tauri::command]
pub async fn set_mail_preferences(
    state: State<'_, AppState>,
    space_id: i64,
    preferences: MailPreferences,
) -> AppResult<MailPreferences> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || {
        mail::preload::set_preferences(
            &db,
            space_id,
            &preferences.trusted_senders,
            preferences.always_load_images,
            preferences.profile_pictures,
        )?;
        mail_preferences(&db, space_id)
    })
    .await
    .map_err(|_| blocking_error())?
}

fn mail_preferences(db: &Database, space_id: i64) -> AppResult<MailPreferences> {
    Ok(MailPreferences {
        trusted_senders: mail::preload::trusted_senders(db, space_id)?,
        always_load_images: mail::preload::always_load_images(db, space_id)?,
        profile_pictures: mail::preload::profile_pictures(db, space_id)?,
    })
}

fn parse_color_scheme(value: &str) -> Option<bool> {
    let value = value.to_lowercase();
    if value.contains("prefer-dark") {
        Some(true)
    } else if value.contains("prefer-light") {
        Some(false)
    } else {
        None
    }
}

fn parse_dms_mode(raw: &str) -> Option<bool> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    match value["mode"].as_str()? {
        "dark" => Some(true),
        "light" => Some(false),
        _ => None,
    }
}

fn dms_mode() -> Option<bool> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cache"))
        })?;
    let path = cache.join("DankMaterialShell").join("dms-colors.json");
    let raw = std::fs::read_to_string(path).ok()?;
    parse_dms_mode(&raw)
}

#[tauri::command]
pub fn open_external(url: String) -> AppResult<()> {
    links::open_external_url(&url)
}

#[tauri::command]
pub fn system_color_scheme() -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        if let Some(mode) = dms_mode() {
            return Some(mode);
        }
        let output = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "color-scheme"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_color_scheme(&String::from_utf8_lossy(&output.stdout))
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[tauri::command]
pub fn get_theme_settings(
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<ThemeSettings> {
    theme::load_settings(&state.db, space_id)
}

#[tauri::command]
pub fn set_theme_settings(
    state: State<'_, AppState>,
    space_id: i64,
    settings: ThemeSettings,
) -> AppResult<ThemeSettings> {
    theme::save_settings(&state.db, space_id, &settings)?;
    theme::load_settings(&state.db, space_id)
}

#[tauri::command]
pub fn set_active_theme_space(
    app: AppHandle,
    state: State<'_, AppState>,
    space_id: i64,
) -> AppResult<ThemeSettings> {
    theme::set_active_theme_space(&state.db, &app, space_id)?;
    theme::load_settings(&state.db, space_id)
}

#[tauri::command]
pub fn reload_theme(app: AppHandle) -> AppResult<Option<Base16Palette>> {
    let path = theme::theme_path(&app)?;
    if !path.exists() {
        return Ok(None);
    }
    theme::read_palette(&path).map(Some)
}

#[cfg(test)]
mod tests {
    use super::{
        attachment_position, cached_attachment, delete_account_record, effective_full_body,
        ensure_trash_delete, error_kind, parse_color_scheme, parse_dms_mode, preview_data_url,
        previewable_image, reply_mail_context, trash_destination, unique_download_path,
        validate_move_target, voice_samples,
    };
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use tempfile::TempDir;

    use crate::accounts::{self, Account, AccountInput, ProtonAccountInput, SignatureFields};
    use crate::db::Database;
    use crate::error::AppError;
    use crate::mail::{parse, Attachment, Message, MessageDetail};
    use crate::proton::{runtime, ProtonAccessLimited, ProtonBridge, ProtonEvents};
    use crate::secrets::{account_key, MemoryStore, SecretStore};
    use crate::spaces::{self, SpaceDeleteAction};

    #[test]
    fn parses_system_color_scheme() {
        assert_eq!(parse_color_scheme("'prefer-dark'\n"), Some(true));
        assert_eq!(parse_color_scheme("'prefer-light'\n"), Some(false));
        assert_eq!(parse_color_scheme("'default'\n"), None);
        assert_eq!(parse_dms_mode(r#"{"mode":"dark"}"#), Some(true));
        assert_eq!(parse_dms_mode(r#"{"mode":"light"}"#), Some(false));
        assert_eq!(parse_dms_mode(r#"{"mode":"auto"}"#), None);
    }

    #[test]
    fn voice_samples_only_use_substantial_sent_mail() {
        let db = Database::open_memory().unwrap();
        let space_id = crate::spaces::default_id(&db).unwrap();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "insert into accounts (id, display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, sync_interval_secs, space_id, created_at) values (1, 'Me', 'me@example.com', 'me', 'imap', 993, 'tls', 'smtp', 465, 'tls', 60, ?1, 0)",
                rusqlite::params![space_id],
            )
            .unwrap();
            conn.execute_batch(
                "insert into folders (id, account_id, path, name, role) values (1, 1, 'INBOX', 'Inbox', 'inbox');
                 insert into folders (id, account_id, path, name, role) values (2, 1, 'Sent', 'Sent', 'sent');
                 insert into messages (account_id, folder_id, uid, subject, snippet, date, is_seen) values (1, 1, 1, 'Inbox note', 'This message should not train the voice profile.', 100, 1);
                 insert into messages (account_id, folder_id, uid, subject, snippet, date, is_seen) values (1, 2, 2, 'Short', 'too short', 200, 1);
                 insert into messages (account_id, folder_id, uid, subject, snippet, date, is_seen) values (1, 2, 3, 'Agenda', 'Happy to join. I will send the agenda tomorrow.', 300, 1);
                 insert into messages (account_id, folder_id, uid, subject, snippet, date, is_seen) values (1, 2, 4, 'Invoice', 'Thanks for the update. The invoice is attached.', 400, 1);",
            )
            .unwrap();
        }

        let samples = voice_samples(&db, space_id).unwrap();
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].0, "Invoice");
        assert_eq!(samples[1].0, "Agenda");
    }

    fn attachment(filename: &str, size: i64, content_id: Option<&str>) -> Attachment {
        Attachment {
            id: 9,
            message_id: 1,
            filename: filename.into(),
            mime_type: "application/octet-stream".into(),
            size,
            content_id: content_id.map(str::to_string),
        }
    }

    fn parsed(filename: &str, size: usize, content_id: Option<&str>) -> parse::ParsedAttachment {
        parse::ParsedAttachment {
            filename: filename.into(),
            mime_type: "application/octet-stream".into(),
            size,
            content_id: content_id.map(str::to_string),
            is_inline: false,
            data: Vec::new(),
        }
    }

    fn message() -> Message {
        Message {
            id: 1,
            account_id: 1,
            folder_id: 1,
            uid: 1,
            message_id: None,
            references: None,
            thread_key: "thread-1".into(),
            from_addr: "ada@example.com".into(),
            from_name: "Ada".into(),
            to_addrs: vec!["me@example.com".into()],
            cc_addrs: Vec::new(),
            subject: "Hello".into(),
            date: 1,
            snippet: "cached snippet".into(),
            is_seen: false,
            is_starred: false,
            has_attachments: false,
            body_cached: true,
            tab_id: None,
        }
    }

    fn detail() -> MessageDetail {
        MessageDetail {
            message: message(),
            text_body: Some("FULL BODY".into()),
            html_body: None,
            image_html: None,
            attachments: Vec::new(),
        }
    }

    #[test]
    fn classification_error_kinds_are_preserved() {
        assert_eq!(error_kind(&AppError::Message("no tab".into())), "message");
        assert_eq!(
            error_kind(&AppError::Io(std::io::Error::other("disk"))),
            "io"
        );
    }

    #[test]
    fn reply_context_respects_the_full_body_privacy_setting() {
        let message = message();
        let detail = detail();

        let private = reply_mail_context(&message, Some(&detail), false);
        assert_eq!(private.body, "cached snippet");

        let shared = reply_mail_context(&message, Some(&detail), true);
        assert_eq!(shared.body, "FULL BODY");
    }

    #[test]
    fn reply_context_falls_back_to_the_snippet_without_a_fetched_body() {
        let context = reply_mail_context(&message(), None, true);
        assert_eq!(context.body, "cached snippet");
    }

    #[test]
    fn backend_full_body_setting_overrides_renderer_requests() {
        let db = crate::db::Database::open_memory().unwrap();
        let space_id = crate::spaces::default_id(&db).unwrap();
        assert!(!effective_full_body(&db, space_id, true).unwrap());
        crate::ai::set_preferences(
            &db,
            space_id,
            &crate::ai::AiPreferences {
                auto_classify: false,
                auto_rail: false,
                full_body_context: true,
                match_voice: false,
            },
        )
        .unwrap();
        assert!(effective_full_body(&db, space_id, true).unwrap());
        assert!(!effective_full_body(&db, space_id, false).unwrap());
    }

    #[test]
    fn permanent_delete_is_restricted_to_trash() {
        assert!(ensure_trash_delete("trash").is_ok());
        assert!(ensure_trash_delete("inbox").is_err());
        assert!(ensure_trash_delete("sent").is_err());
    }

    #[test]
    fn trash_destination_requires_an_existing_trash_folder() {
        assert!(trash_destination("inbox", None).is_err());
        assert!(trash_destination("trash", Some("Trash".into())).is_err());
        assert_eq!(
            trash_destination("inbox", Some("Trash".into())).unwrap(),
            "Trash"
        );
    }

    #[test]
    fn move_target_must_be_another_real_folder_in_the_same_account() {
        use crate::mail::Folder;

        fn folder(id: i64, account_id: i64, role: &str) -> Folder {
            Folder {
                id,
                account_id,
                path: format!("folder-{id}"),
                name: format!("Folder {id}"),
                role: role.into(),
                delimiter: "/".into(),
                uid_validity: 0,
                highest_uid: 0,
                unread_count: 0,
            }
        }

        let current = folder(1, 1, "inbox");
        assert!(validate_move_target(&current, &folder(2, 1, "archive")).is_ok());
        assert!(validate_move_target(&current, &folder(1, 1, "inbox")).is_err());
        assert!(validate_move_target(&current, &folder(2, 2, "archive")).is_err());
        assert!(validate_move_target(&current, &folder(2, 1, "drafts")).is_err());
        assert!(validate_move_target(&current, &folder(2, 1, "all")).is_err());
    }

    #[test]
    fn attachment_position_matches_metadata_not_order() {
        let parsed = vec![
            parsed("other.pdf", 5, None),
            parsed("report.pdf", 7, Some("cid-1")),
        ];
        assert_eq!(
            attachment_position(&parsed, &attachment("report.pdf", 7, Some("cid-1"))),
            Some(1)
        );
        assert_eq!(
            attachment_position(&parsed, &attachment("missing.pdf", 7, None)),
            None
        );
    }

    #[test]
    fn attachment_position_falls_back_to_name_and_size() {
        let parsed = vec![
            parsed("a.pdf", 3, Some("x")),
            parsed("report.pdf", 7, Some("different")),
        ];
        assert_eq!(
            attachment_position(&parsed, &attachment("report.pdf", 7, Some("cid-1"))),
            Some(1)
        );
    }

    #[test]
    fn cached_attachment_reuses_only_regular_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("1-report.pdf");
        assert_eq!(cached_attachment(&file).unwrap(), None);

        std::fs::write(&file, b"data").unwrap();
        assert_eq!(
            cached_attachment(&file).unwrap(),
            Some(file.to_string_lossy().to_string())
        );

        let folder = dir.path().join("2-folder");
        std::fs::create_dir(&folder).unwrap();
        assert!(cached_attachment(&folder).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn cached_attachment_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, b"data").unwrap();
        let link = dir.path().join("3-link");
        symlink(&target, &link).unwrap();
        assert!(cached_attachment(&link).is_err());
    }

    #[test]
    fn download_paths_never_overwrite_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("report.pdf"), b"one").unwrap();
        assert_eq!(
            unique_download_path(dir.path(), "report.pdf"),
            dir.path().join("report (1).pdf")
        );

        std::fs::write(dir.path().join("report (1).pdf"), b"two").unwrap();
        assert_eq!(
            unique_download_path(dir.path(), "report.pdf"),
            dir.path().join("report (2).pdf")
        );

        std::fs::write(dir.path().join("notes"), b"one").unwrap();
        assert_eq!(
            unique_download_path(dir.path(), "notes"),
            dir.path().join("notes (1)")
        );
    }

    #[test]
    fn preview_accepts_small_raster_images_only() {
        assert!(previewable_image("image/png", 1024));
        assert!(previewable_image("image/png; charset=binary", 1024));
        assert!(previewable_image("image/jpeg", 4 * 1024 * 1024));
        assert!(!previewable_image("image/svg", 1024));
        assert!(!previewable_image("image/svg+xml", 1024));
        assert!(!previewable_image("image/svg+xml; charset=utf-8", 1024));
        assert!(!previewable_image("application/pdf", 1024));
        assert!(!previewable_image("image/png", 4 * 1024 * 1024 + 1));
    }

    #[test]
    fn preview_data_url_is_base64_and_keeps_mime() {
        assert_eq!(
            preview_data_url("image/png", b"abc"),
            "data:image/png;base64,YWJj"
        );
    }

    struct NoEvents;

    impl ProtonEvents for NoEvents {
        fn access_limited(&self, _status: ProtonAccessLimited) {}
    }

    struct ProtonFixture {
        dir: TempDir,
        db: Arc<Database>,
        secrets: Arc<MemoryStore>,
    }

    impl ProtonFixture {
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

        fn install(&self) -> Arc<ProtonBridge> {
            let bridge = Arc::new(ProtonBridge::new(
                Arc::new(NoEvents),
                Arc::clone(&self.db),
                Arc::clone(&self.secrets) as Arc<dyn SecretStore>,
                self.config_dir(),
                sidecar(),
            ));
            runtime::install(Arc::clone(&bridge));
            bridge
        }

        fn add_proton(&self, email: &str, space_id: i64) -> Account {
            let ports = crate::proton::ports::free_pair().unwrap();
            accounts::create_proton(
                &self.db,
                self.secrets.as_ref(),
                ProtonAccountInput {
                    display_name: "Proton".into(),
                    email: email.into(),
                    username: email.into(),
                    bridge_password: "bridge-password".into(),
                    imap_host: "127.0.0.1".into(),
                    imap_port: ports.imap,
                    smtp_host: "127.0.0.1".into(),
                    smtp_port: ports.smtp,
                    color: "lavender".into(),
                    signature: String::new(),
                    signature_fields: SignatureFields::default(),
                    space_id,
                },
            )
            .unwrap()
        }

        fn add_password(&self, email: &str, space_id: i64) -> Account {
            accounts::create(
                &self.db,
                self.secrets.as_ref(),
                AccountInput {
                    display_name: "Manual".into(),
                    email: email.into(),
                    username: email.into(),
                    password: "p@ss".into(),
                    imap_host: "imap.example.com".into(),
                    imap_port: 993,
                    imap_security: "tls".into(),
                    smtp_host: "smtp.example.com".into(),
                    smtp_port: 465,
                    smtp_security: "tls".into(),
                    color: "accent".into(),
                    signature: String::new(),
                    signature_fields: SignatureFields::default(),
                    signature_on_new: true,
                    space_id,
                },
            )
            .unwrap()
        }

        fn write_auth(&self, entries: serde_json::Value) {
            let hydroxide = self.config_dir().join("hydroxide");
            std::fs::create_dir_all(&hydroxide).unwrap();
            std::fs::write(hydroxide.join("auth.json"), entries.to_string()).unwrap();
        }

        fn auth_entries(&self) -> serde_json::Value {
            let path = self.config_dir().join("hydroxide/auth.json");
            match std::fs::read_to_string(path) {
                Ok(contents) => serde_json::from_str(&contents).unwrap(),
                Err(_) => serde_json::json!({}),
            }
        }

        fn generation(&self) -> Option<String> {
            std::fs::read_to_string(self.config_dir().join("sidecar-generation")).ok()
        }
    }

    fn sidecar() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".local-tests/proton-bridge/fake-sidecar.sh")
    }

    fn hold_auth_lock(fixture: &ProtonFixture) -> std::fs::File {
        use fs2::FileExt;

        let hydroxide = fixture.config_dir().join("hydroxide");
        std::fs::create_dir_all(&hydroxide).unwrap();
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(hydroxide.join("auth.lock"))
            .unwrap();
        lock.lock_exclusive().unwrap();
        lock
    }

    #[test]
    fn deleting_a_proton_account_forgets_its_auth_and_stops_the_bridge() {
        let _guard = runtime::test_guard();
        let fixture = ProtonFixture::new();
        let bridge = fixture.install();
        let space_id = crate::spaces::default_id(&fixture.db).unwrap();
        let account = fixture.add_proton("me@proton.me", space_id);
        fixture.write_auth(serde_json::json!({
            "me@proton.me": "first",
            "other@proton.me": "second",
        }));
        bridge.start_if_needed().unwrap().unwrap();

        delete_account_record(&fixture.db, fixture.secrets.as_ref(), account.id).unwrap();

        assert!(!bridge.is_running());
        let entries = fixture.auth_entries();
        assert_eq!(entries.get("me@proton.me"), None);
        assert_eq!(entries["other@proton.me"], "second");
        assert!(accounts::list(&fixture.db).unwrap().is_empty());
        runtime::stop();
    }

    #[test]
    fn deleting_a_non_last_proton_account_restarts_the_bridge() {
        let _guard = runtime::test_guard();
        let fixture = ProtonFixture::new();
        let bridge = fixture.install();
        let space_id = crate::spaces::default_id(&fixture.db).unwrap();
        let first = fixture.add_proton("one@proton.me", space_id);
        fixture.add_proton("two@proton.me", space_id);
        fixture.write_auth(serde_json::json!({
            "one@proton.me": "first",
            "two@proton.me": "second",
        }));
        bridge.start_if_needed().unwrap().unwrap();
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        delete_account_record(&fixture.db, fixture.secrets.as_ref(), first.id).unwrap();

        assert!(bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("2"));
        let entries = fixture.auth_entries();
        assert_eq!(entries.get("one@proton.me"), None);
        assert_eq!(entries["two@proton.me"], "second");
        runtime::stop();
    }

    #[test]
    fn deleting_a_proton_account_aborts_while_sign_in_holds_the_auth_lock() {
        let _guard = runtime::test_guard();
        let fixture = ProtonFixture::new();
        fixture.install();
        let space_id = crate::spaces::default_id(&fixture.db).unwrap();
        let account = fixture.add_proton("me@proton.me", space_id);
        let lock = hold_auth_lock(&fixture);

        let error =
            delete_account_record(&fixture.db, fixture.secrets.as_ref(), account.id).unwrap_err();

        assert_eq!(error.to_string(), "Proton sign-in in progress");
        assert_eq!(accounts::list(&fixture.db).unwrap().len(), 1);
        assert_eq!(
            fixture.secrets.get(&account_key(account.id)).unwrap(),
            "bridge-password"
        );
        fs2::FileExt::unlock(&lock).unwrap();
        runtime::stop();
    }

    #[test]
    fn deleting_a_non_proton_account_leaves_the_bridge_running() {
        let _guard = runtime::test_guard();
        let fixture = ProtonFixture::new();
        let bridge = fixture.install();
        let space_id = crate::spaces::default_id(&fixture.db).unwrap();
        fixture.add_proton("me@proton.me", space_id);
        let manual = fixture.add_password("me@example.com", space_id);
        bridge.start_if_needed().unwrap().unwrap();
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        delete_account_record(&fixture.db, fixture.secrets.as_ref(), manual.id).unwrap();

        assert!(bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("1"));
        assert_eq!(accounts::list(&fixture.db).unwrap().len(), 1);
        runtime::stop();
    }

    #[test]
    fn deleting_a_space_forgets_every_proton_account_once() {
        let _guard = runtime::test_guard();
        let fixture = ProtonFixture::new();
        let bridge = fixture.install();
        let work = spaces::create(&fixture.db, "Work").unwrap();
        fixture.add_proton("one@proton.me", work.id);
        fixture.add_proton("two@proton.me", work.id);
        fixture.write_auth(serde_json::json!({
            "one@proton.me": "first",
            "two@proton.me": "second",
        }));
        bridge.start_if_needed().unwrap().unwrap();
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        spaces::delete(
            &fixture.db,
            fixture.secrets.as_ref(),
            work.id,
            SpaceDeleteAction::DeleteAccounts,
        )
        .unwrap();

        assert!(!bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("1"));
        assert_eq!(fixture.auth_entries(), serde_json::json!({}));
        assert!(accounts::list(&fixture.db).unwrap().is_empty());
        runtime::stop();
    }

    #[test]
    fn deleting_a_space_aborts_while_sign_in_holds_the_auth_lock() {
        let _guard = runtime::test_guard();
        let fixture = ProtonFixture::new();
        fixture.install();
        let work = spaces::create(&fixture.db, "Work").unwrap();
        let account = fixture.add_proton("me@proton.me", work.id);
        let lock = hold_auth_lock(&fixture);

        let error = spaces::delete(
            &fixture.db,
            fixture.secrets.as_ref(),
            work.id,
            SpaceDeleteAction::DeleteAccounts,
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "Proton sign-in in progress");
        assert!(spaces::by_id(&fixture.db, work.id).is_ok());
        assert_eq!(accounts::list(&fixture.db).unwrap().len(), 1);
        assert_eq!(
            fixture.secrets.get(&account_key(account.id)).unwrap(),
            "bridge-password"
        );
        fs2::FileExt::unlock(&lock).unwrap();
        runtime::stop();
    }

    #[test]
    fn deleting_a_space_with_other_proton_accounts_restarts_the_bridge() {
        let _guard = runtime::test_guard();
        let fixture = ProtonFixture::new();
        let bridge = fixture.install();
        let personal = crate::spaces::default_id(&fixture.db).unwrap();
        let work = spaces::create(&fixture.db, "Work").unwrap();
        fixture.add_proton("personal@proton.me", personal);
        fixture.add_proton("work@proton.me", work.id);
        fixture.write_auth(serde_json::json!({
            "personal@proton.me": "personal-secret",
            "work@proton.me": "work-secret",
        }));
        bridge.start_if_needed().unwrap().unwrap();
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        spaces::delete(
            &fixture.db,
            fixture.secrets.as_ref(),
            work.id,
            SpaceDeleteAction::DeleteAccounts,
        )
        .unwrap();

        assert!(bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("2"));
        assert_eq!(
            fixture.auth_entries(),
            serde_json::json!({"personal@proton.me": "personal-secret"})
        );
        assert_eq!(accounts::list(&fixture.db).unwrap().len(), 1);
        runtime::stop();
    }

    #[test]
    fn deleting_a_non_proton_space_leaves_the_bridge_running() {
        let _guard = runtime::test_guard();
        let fixture = ProtonFixture::new();
        let bridge = fixture.install();
        let personal = crate::spaces::default_id(&fixture.db).unwrap();
        let work = spaces::create(&fixture.db, "Work").unwrap();
        fixture.add_proton("personal@proton.me", personal);
        fixture.add_password("work@example.com", work.id);
        bridge.start_if_needed().unwrap().unwrap();
        assert_eq!(fixture.generation().as_deref(), Some("1"));

        spaces::delete(
            &fixture.db,
            fixture.secrets.as_ref(),
            work.id,
            SpaceDeleteAction::DeleteAccounts,
        )
        .unwrap();

        assert!(bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("1"));
        assert_eq!(accounts::list(&fixture.db).unwrap().len(), 1);
        runtime::stop();
    }

}
