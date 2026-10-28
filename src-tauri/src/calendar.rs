use std::sync::Once;
use std::time::Duration;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use fast_dav_rs::webdav::sync::SyncEntry;
use fast_dav_rs::webdav::Privilege;
use fast_dav_rs::CalDavClient;
use icalendar::{Calendar as ICalendar, CalendarDateTime, Component, DatePerhapsTime, EventLike};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{self, Database};
use crate::error::{AppError, AppResult};
use crate::secrets::{calendar_account_key, SecretStore};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CalendarAccount {
    pub id: i64,
    pub display_name: String,
    pub base_url: String,
    pub username: String,
    pub space_id: i64,
    pub position: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarAccountInput {
    #[serde(default)]
    pub display_name: String,
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub space_id: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSummary {
    pub id: i64,
    pub calendar_account_id: i64,
    pub href: String,
    pub name: String,
    pub color: Option<String>,
    pub sync_token: Option<String>,
    pub last_synced_at: Option<i64>,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: i64,
    pub calendar_id: i64,
    pub calendar_name: String,
    pub calendar_color: Option<String>,
    pub href: String,
    pub etag: Option<String>,
    pub uid: String,
    pub read_only: bool,
    pub summary: String,
    pub description: String,
    pub location: String,
    pub starts_at: i64,
    pub ends_at: Option<i64>,
    pub all_day: bool,
    pub recurrence: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSyncResult {
    pub calendars: usize,
    pub events: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEventInput {
    pub summary: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub location: String,
    pub starts_at: i64,
    pub ends_at: Option<i64>,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default)]
    pub recurrence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CalendarTarget {
    id: i64,
    href: String,
    sync_token: Option<String>,
    base_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CalendarWriteTarget {
    href: String,
    base_url: String,
    calendar_account_id: i64,
    read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EventWriteTarget {
    calendar_account_id: i64,
    href: String,
    etag: Option<String>,
    read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemoteCalendar {
    pub href: String,
    pub name: String,
    pub color: Option<String>,
    pub components: Vec<String>,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemoteEvent {
    pub href: String,
    pub etag: Option<String>,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedEvent {
    uid: String,
    summary: String,
    description: String,
    location: String,
    starts_at: i64,
    ends_at: Option<i64>,
    all_day: bool,
    recurrence: String,
}

fn calendar_account_by_id(db: &Database, id: i64) -> AppResult<CalendarAccount> {
    db.read()
        .query_row(
            "select id, display_name, base_url, username, space_id, position, created_at
             from calendar_accounts where id = ?1",
            params![id],
            |row| {
                Ok(CalendarAccount {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    base_url: row.get(2)?,
                    username: row.get(3)?,
                    space_id: row.get(4)?,
                    position: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::Message("calendar account not found".into()))
}

pub fn list_accounts(db: &Database, space_id: i64) -> AppResult<Vec<CalendarAccount>> {
    let conn = db.read();
    let mut stmt = conn.prepare(
        "select id, display_name, base_url, username, space_id, position, created_at
         from calendar_accounts where space_id = ?1
         order by position, id",
    )?;
    let rows = stmt.query_map(params![space_id], |row| {
        Ok(CalendarAccount {
            id: row.get(0)?,
            display_name: row.get(1)?,
            base_url: row.get(2)?,
            username: row.get(3)?,
            space_id: row.get(4)?,
            position: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn reorder_accounts(
    db: &Database,
    space_id: i64,
    ids: &[i64],
) -> AppResult<Vec<CalendarAccount>> {
    let current = list_accounts(db, space_id)?;
    let current_ids = current
        .iter()
        .map(|account| account.id)
        .collect::<std::collections::HashSet<_>>();
    let requested_ids = ids.iter().copied().collect::<std::collections::HashSet<_>>();
    if current_ids != requested_ids || current_ids.len() != ids.len() {
        return Err(AppError::Message(
            "calendar account order must include every account in the space exactly once".into(),
        ));
    }
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    for (position, id) in ids.iter().enumerate() {
        tx.execute(
            "update calendar_accounts set position = ?1 where id = ?2 and space_id = ?3",
            params![position as i64, id, space_id],
        )?;
    }
    tx.commit()?;
    drop(conn);
    list_accounts(db, space_id)
}

pub fn move_account_to_space(
    db: &Database,
    id: i64,
    space_id: i64,
) -> AppResult<CalendarAccount> {
    crate::spaces::by_id(db, space_id)?;
    let conn = db.conn.lock().unwrap();
    let current_space: Option<i64> = conn
        .query_row(
            "select space_id from calendar_accounts where id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    let current_space =
        current_space.ok_or_else(|| AppError::Message("calendar account not found".into()))?;
    if current_space == space_id {
        drop(conn);
        return calendar_account_by_id(db, id);
    }
    let position: i64 = conn.query_row(
        "select coalesce(max(position) + 1, 0) from calendar_accounts where space_id = ?1",
        params![space_id],
        |row| row.get(0),
    )?;
    conn.execute(
        "update calendar_accounts set space_id = ?1, position = ?2 where id = ?3",
        params![space_id, position, id],
    )?;
    drop(conn);
    calendar_account_by_id(db, id)
}

fn calendar_password(secrets: &dyn SecretStore, account_id: i64) -> AppResult<String> {
    secrets.get(&calendar_account_key(account_id))
}

fn calendar_error(error: fast_dav_rs::Error) -> AppError {
    AppError::Message(format!("calendar server error: {error}"))
}

fn clean_base_url(value: &str) -> AppResult<String> {
    let url = value.trim().trim_end_matches('/');
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(AppError::Message(
            "calendar URL must start with https:// or http://".into(),
        ));
    }
    if url.chars().any(char::is_whitespace) {
        return Err(AppError::Message("calendar URL is invalid".into()));
    }
    Ok(url.to_string())
}

fn install_crypto_provider() {
    static INSTALL_PROVIDER: Once = Once::new();
    INSTALL_PROVIDER.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

fn client_for_base(
    base_url: &str,
    username: &str,
    password: &str,
) -> AppResult<CalDavClient> {
    install_crypto_provider();
    CalDavClient::builder(base_url)
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .max_retries(2)
        .user_agent(format!("AuraMail/{}", env!("CARGO_PKG_VERSION")))
        .require_https(base_url.starts_with("https://"))
        .basic_auth(username, password)
        .build()
        .map_err(calendar_error)
}

fn client_for(account: &CalendarAccount, password: &str) -> AppResult<CalDavClient> {
    client_for_base(&account.base_url, &account.username, password)
}

fn discovery_candidates(base_url: &str) -> Vec<String> {
    let base = base_url.trim_end_matches('/');
    let origin_end = base
        .find("://")
        .map(|index| index + 3)
        .and_then(|start| {
            base[start..]
                .find('/')
                .map(|offset| start + offset)
                .or(Some(base.len()))
        })
        .unwrap_or(base.len());
    let origin = &base[..origin_end];
    let mut candidates = vec![
        format!("{origin}/.well-known/caldav"),
        format!("{base}/"),
    ];
    candidates.dedup();
    candidates
}

fn privileges_allow_write(privileges: &[Privilege]) -> bool {
    privileges.iter().any(|privilege| match privilege {
        Privilege::All | Privilege::Write | Privilege::WriteContent => true,
        Privilege::Other(name) => matches!(
            name.to_ascii_lowercase().as_str(),
            "all" | "write" | "write-content"
        ),
        _ => false,
    })
}

fn remote_calendars(calendars: Vec<fast_dav_rs::caldav::types::CalendarInfo>) -> Vec<RemoteCalendar> {
    calendars
        .into_iter()
        .map(|calendar| {
            let read_only =
                !calendar.privileges.is_empty() && !privileges_allow_write(&calendar.privileges);
            RemoteCalendar {
                href: calendar.href,
                name: calendar
                    .displayname
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| "Calendar".into()),
                color: calendar.color,
                components: calendar.supported_components,
                read_only,
            }
        })
        .collect()
}

async fn discover_calendars(client: &CalDavClient) -> AppResult<Vec<RemoteCalendar>> {
    let principal = client
        .discover_current_user_principal()
        .await
        .map_err(calendar_error)?
        .ok_or_else(|| AppError::Message("calendar server did not return a principal".into()))?;
    let homes = client
        .discover_calendar_home_set(&principal)
        .await
        .map_err(calendar_error)?;
    let home = homes
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Message("calendar home set not found".into()))?;
    let calendars = client
        .list_calendars(&home)
        .await
        .map_err(calendar_error)?;
    Ok(remote_calendars(calendars))
}

fn remote_events(entries: &[SyncEntry]) -> (Vec<RemoteEvent>, Vec<String>) {
    let mut events = Vec::new();
    let mut errors = Vec::new();
    for entry in entries {
        match entry.data.as_ref() {
            Some(data) => events.push(RemoteEvent {
                href: entry.href.clone(),
                etag: entry.etag.clone(),
                data: data.clone(),
            }),
            None => errors.push(format!("{} did not include calendar data", entry.href)),
        }
    }
    (events, errors)
}

fn calendar_targets(db: &Database, account_id: i64) -> AppResult<Vec<CalendarTarget>> {
    let conn = db.read();
    let mut stmt = conn.prepare(
        "select c.id, c.href, c.sync_token, ca.base_url
         from calendars c
         join calendar_accounts ca on ca.id = c.calendar_account_id
         where ca.id = ?1 and c.unsubscribed = 0
         order by c.id",
    )?;
    let rows = stmt.query_map(params![account_id], |row| {
        Ok(CalendarTarget {
            id: row.get(0)?,
            href: row.get(1)?,
            sync_token: row.get(2)?,
            base_url: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn calendar_write_target(db: &Database, calendar_id: i64) -> AppResult<CalendarWriteTarget> {
    db.read()
        .query_row(
            "select c.href, ca.base_url, ca.id, c.read_only
             from calendars c
             join calendar_accounts ca on ca.id = c.calendar_account_id
             where c.id = ?1 and c.unsubscribed = 0",
            params![calendar_id],
            |row| {
                Ok(CalendarWriteTarget {
                    href: row.get(0)?,
                    base_url: row.get(1)?,
                    calendar_account_id: row.get(2)?,
                    read_only: row.get::<_, i64>(3)? != 0,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::Message("calendar not found".into()))
}

fn event_write_target(db: &Database, event_id: i64) -> AppResult<EventWriteTarget> {
    db.read()
        .query_row(
            "select ca.id, e.href, e.etag, c.read_only
             from calendar_events e
             join calendars c on c.id = e.calendar_id
             join calendar_accounts ca on ca.id = c.calendar_account_id
             where e.id = ?1 and c.unsubscribed = 0",
            params![event_id],
            |row| {
                Ok(EventWriteTarget {
                    calendar_account_id: row.get(0)?,
                    href: row.get(1)?,
                    etag: row.get(2)?,
                    read_only: row.get::<_, i64>(3)? != 0,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::Message("calendar event not found".into()))
}

fn escape_ics(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\r', "")
        .replace('\n', "\\n")
}

fn utc_stamp(timestamp: i64) -> AppResult<String> {
    DateTime::<Utc>::from_timestamp(timestamp, 0)
        .map(|value| value.format("%Y%m%dT%H%M%SZ").to_string())
        .ok_or_else(|| AppError::Message("calendar date is out of range".into()))
}

fn utc_date(timestamp: i64) -> AppResult<String> {
    DateTime::<Utc>::from_timestamp(timestamp, 0)
        .map(|value| value.format("%Y%m%d").to_string())
        .ok_or_else(|| AppError::Message("calendar date is out of range".into()))
}

fn event_path(calendar_href: &str, uid: &str) -> String {
    if calendar_href.ends_with('/') {
        format!("{calendar_href}{uid}.ics")
    } else {
        format!("{calendar_href}/{uid}.ics")
    }
}

fn recurrence_rule(value: &str) -> AppResult<Option<String>> {
    let value = value.trim().to_ascii_uppercase();
    if value.is_empty() {
        return Ok(None);
    }
    let valid = value.starts_with("FREQ=")
        && value.chars().all(|character| {
            character.is_ascii_uppercase()
                || character.is_ascii_digit()
                || matches!(character, '=' | ';' | ',' | ':' | '+' | '-')
        });
    if !valid {
        return Err(AppError::Message("invalid repeat rule".into()));
    }
    Ok(Some(value))
}

fn build_event_ics(input: &CalendarEventInput, uid: &str) -> AppResult<String> {
    let summary = input.summary.trim();
    if summary.is_empty() {
        return Err(AppError::Message("event title is required".into()));
    }
    let ends_at = input
        .ends_at
        .unwrap_or(if input.all_day {
            input.starts_at + 86_400
        } else {
            input.starts_at + 3_600
        });
    if ends_at <= input.starts_at {
        return Err(AppError::Message("event end must be after its start".into()));
    }
    let dtstamp = utc_stamp(db::now())?;
    let (start_line, end_line) = if input.all_day {
        (
            format!("DTSTART;VALUE=DATE:{}", utc_date(input.starts_at)?),
            format!("DTEND;VALUE=DATE:{}", utc_date(ends_at)?),
        )
    } else {
        (
            format!("DTSTART:{}", utc_stamp(input.starts_at)?),
            format!("DTEND:{}", utc_stamp(ends_at)?),
        )
    };
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".to_string(),
        "PRODID:-//AuraMail//AuraMail Calendar//EN".to_string(),
        "CALSCALE:GREGORIAN".to_string(),
        "BEGIN:VEVENT".to_string(),
        format!("UID:{uid}"),
        format!("DTSTAMP:{dtstamp}"),
        start_line,
        end_line,
        format!("SUMMARY:{}", escape_ics(summary)),
        "STATUS:CONFIRMED".to_string(),
    ];
    if !input.description.trim().is_empty() {
        lines.push(format!(
            "DESCRIPTION:{}",
            escape_ics(input.description.trim())
        ));
    }
    if !input.location.trim().is_empty() {
        lines.push(format!("LOCATION:{}", escape_ics(input.location.trim())));
    }
    if let Some(rule) = recurrence_rule(&input.recurrence)? {
        lines.push(format!("RRULE:{rule}"));
    }
    lines.push("END:VEVENT".to_string());
    lines.push("END:VCALENDAR".to_string());
    Ok(lines.join("\r\n") + "\r\n")
}

pub async fn create_event(
    db: &Database,
    secrets: &dyn SecretStore,
    calendar_id: i64,
    input: CalendarEventInput,
) -> AppResult<CalendarSyncResult> {
    let target = calendar_write_target(db, calendar_id)?;
    if target.read_only {
        return Err(AppError::Message("this calendar is read-only".into()));
    }
    let account = calendar_account_by_id(db, target.calendar_account_id)?;
    let password = calendar_password(secrets, account.id)?;
    let client = client_for(&account, &password)?;
    let uid = format!("{}-{}@auramail", db::now(), rand::random::<u64>());
    let body = build_event_ics(&input, &uid)?;
    let response = client
        .put_if_none_match(&event_path(&target.href, &uid), Bytes::from(body))
        .await
        .map_err(calendar_error)?;
    if !response.status().is_success() {
        return Err(AppError::Message(format!(
            "calendar server rejected the event ({})",
            response.status()
        )));
    }
    sync_account(db, secrets, target.calendar_account_id).await
}

pub async fn update_event(
    db: &Database,
    secrets: &dyn SecretStore,
    event_id: i64,
    input: CalendarEventInput,
) -> AppResult<CalendarSyncResult> {
    let target = event_write_target(db, event_id)?;
    if target.read_only {
        return Err(AppError::Message("this calendar is read-only".into()));
    }
    let etag = target
        .etag
        .clone()
        .ok_or_else(|| AppError::Message("event has no ETag".into()))?;
    let uid = db
        .read()
        .query_row(
            "select uid from calendar_events where id = ?1",
            params![event_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| AppError::Message("event not found".into()))?;
    let account = calendar_account_by_id(db, target.calendar_account_id)?;
    let password = calendar_password(secrets, account.id)?;
    let client = client_for(&account, &password)?;
    let body = build_event_ics(&input, &uid)?;
    let response = client
        .put_if_match(&target.href, Bytes::from(body), &etag)
        .await
        .map_err(calendar_error)?;
    if !response.status().is_success() {
        return Err(AppError::Message(format!(
            "calendar server rejected the update ({})",
            response.status()
        )));
    }
    sync_account(db, secrets, target.calendar_account_id).await
}

pub async fn delete_event(
    db: &Database,
    secrets: &dyn SecretStore,
    event_id: i64,
) -> AppResult<CalendarSyncResult> {
    let target = event_write_target(db, event_id)?;
    if target.read_only {
        return Err(AppError::Message("this calendar is read-only".into()));
    }
    let etag = target
        .etag
        .ok_or_else(|| AppError::Message("event has no ETag".into()))?;
    let account = calendar_account_by_id(db, target.calendar_account_id)?;
    let password = calendar_password(secrets, account.id)?;
    let client = client_for(&account, &password)?;
    let response = client
        .delete_if_match(&target.href, &etag)
        .await
        .map_err(calendar_error)?;
    if !response.status().is_success() {
        return Err(AppError::Message(format!(
            "calendar server rejected the delete ({})",
            response.status()
        )));
    }
    sync_account(db, secrets, target.calendar_account_id).await
}

fn set_sync_token(db: &Database, calendar_id: i64, token: Option<&str>) -> AppResult<()> {
    db.conn.lock().unwrap().execute(
        "update calendars set sync_token = ?1, last_synced_at = ?2 where id = ?3",
        params![token, db::now(), calendar_id],
    )?;
    Ok(())
}

pub async fn add_account(
    db: &Database,
    secrets: &dyn SecretStore,
    input: CalendarAccountInput,
) -> AppResult<CalendarAccount> {
    let base_url = clean_base_url(&input.base_url)?;
    let username = input.username.trim();
    if username.is_empty() {
        return Err(AppError::Message("calendar username is required".into()));
    }
    if input.password.is_empty() {
        return Err(AppError::Message("calendar password is required".into()));
    }
    crate::spaces::by_id(db, input.space_id)?;
    let display_name = if input.display_name.trim().is_empty() {
        username.to_string()
    } else {
        input.display_name.trim().to_string()
    };
    let mut errors = Vec::new();
    let mut discovered = None;
    for candidate in discovery_candidates(&base_url) {
        let client = client_for_base(&candidate, username, &input.password)?;
        match discover_calendars(&client).await {
            Ok(remote) => {
                discovered = Some((candidate, remote));
                break;
            }
            Err(error) => errors.push(error.to_string()),
        }
    }
    let (base_url, remote) = discovered.ok_or_else(|| {
        if errors.iter().any(|error| error.contains("401")) {
            AppError::Message(
                "calendar authentication failed (401 Unauthorized). Use an app-specific \
                 password/token if your provider requires one."
                    .into(),
            )
        } else {
            AppError::Message(format!("calendar discovery failed: {}", errors.join("; ")))
        }
    })?;
    let created_at = db::now();
    let (id, position) = {
        let conn = db.conn.lock().unwrap();
        let position: i64 = conn.query_row(
            "select coalesce(max(position) + 1, 0) from calendar_accounts where space_id = ?1",
            params![input.space_id],
            |row| row.get(0),
        )?;
        conn.execute(
            "insert into calendar_accounts
                (display_name, base_url, username, space_id, position, created_at)
             values (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                display_name,
                base_url,
                username,
                input.space_id,
                position,
                created_at
            ],
        )?;
        (conn.last_insert_rowid(), position)
    };

    if let Err(error) = secrets.set(&calendar_account_key(id), &input.password) {
        let _ = db.conn.lock().unwrap().execute(
            "delete from calendar_accounts where id = ?1",
            params![id],
        );
        return Err(error);
    }

    let account = CalendarAccount {
        id,
        display_name,
        base_url,
        username: username.to_string(),
        space_id: input.space_id,
        position,
        created_at,
    };
    if let Err(error) = save_calendars(db, id, &remote) {
        let _ = secrets.delete(&calendar_account_key(id));
        let _ = db
            .conn
            .lock()
            .unwrap()
            .execute("delete from calendar_accounts where id = ?1", params![id]);
        return Err(error);
    }
    Ok(account)
}

pub fn unsubscribe_calendar(db: &Database, calendar_id: i64) -> AppResult<()> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    let changed = tx.execute(
        "update calendars set unsubscribed = 1, sync_token = null where id = ?1",
        params![calendar_id],
    )?;
    if changed == 0 {
        return Err(AppError::Message("calendar not found".into()));
    }
    tx.execute(
        "delete from calendar_events where calendar_id = ?1",
        params![calendar_id],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn remove_account(db: &Database, secrets: &dyn SecretStore, account_id: i64) -> AppResult<()> {
    calendar_account_by_id(db, account_id)?;
    secrets.delete(&calendar_account_key(account_id))?;
    db.conn.lock().unwrap().execute(
        "delete from calendar_accounts where id = ?1",
        params![account_id],
    )?;
    Ok(())
}

pub async fn sync_account(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: i64,
) -> AppResult<CalendarSyncResult> {
    let account = calendar_account_by_id(db, account_id)?;
    let password = calendar_password(secrets, account_id)?;
    let client = client_for(&account, &password)?;
    let mut result = CalendarSyncResult {
        calendars: 0,
        events: 0,
        errors: Vec::new(),
    };

    // Refresh collection metadata as well as events so server-side changes to
    // read-only status and calendar names reach the existing account.
    match discover_calendars(&client).await {
        Ok(remote) => save_calendars(db, account_id, &remote)?,
        Err(error) => result.errors.push(error.to_string()),
    }

    let targets = calendar_targets(db, account_id)?;
    if targets.is_empty() {
        return Err(AppError::Message("no calendars connected".into()));
    }

    for target in targets {
        let session = client
            .sync_session(&target.href)
            .with_sync_token(target.sync_token.as_deref());
        let (events, token) = if target.sync_token.is_none() {
            match session.initial().await {
                Ok(snapshot) => {
                    let (events, errors) = remote_events(&snapshot.items);
                    result.errors.extend(errors);
                    apply_snapshot(db, target.id, &events)?;
                    (events, snapshot.sync_token)
                }
                Err(error) => {
                    result
                        .errors
                        .push(format!("{}: {error}", target.href));
                    continue;
                }
            }
        } else {
            match session.incremental().await {
                Ok(delta) => {
                    let (added, added_errors) = remote_events(&delta.added);
                    let (modified, modified_errors) = remote_events(&delta.modified);
                    result.errors.extend(added_errors);
                    result.errors.extend(modified_errors);
                    if delta.resynced {
                        apply_snapshot(db, target.id, &added)?;
                    } else {
                        apply_delta(db, target.id, &added, &modified, &delta.deleted)?;
                    }
                    let mut all = added;
                    all.extend(modified);
                    (all, delta.sync_token)
                }
                Err(error) => {
                    result
                        .errors
                        .push(format!("{}: {error}", target.href));
                    continue;
                }
            }
        };
        set_sync_token(db, target.id, token.as_deref())?;
        result.calendars += 1;
        result.events += events.len();
    }
    Ok(result)
}

fn row_to_calendar(row: &rusqlite::Row<'_>) -> rusqlite::Result<CalendarSummary> {
    Ok(CalendarSummary {
        id: row.get(0)?,
        calendar_account_id: row.get(1)?,
        href: row.get(2)?,
        name: row.get(3)?,
        color: row.get(4)?,
        sync_token: row.get(5)?,
        last_synced_at: row.get(6)?,
        read_only: row.get::<_, i64>(7)? != 0,
    })
}

pub fn list_calendars(db: &Database, space_id: i64) -> AppResult<Vec<CalendarSummary>> {
    let conn = db.read();
    let mut stmt = conn.prepare(
        "select c.id, c.calendar_account_id, c.href, c.name,
                c.color, c.sync_token, c.last_synced_at, c.read_only
         from calendars c
         join calendar_accounts ca on ca.id = c.calendar_account_id
         where ca.space_id = ?1 and c.unsubscribed = 0
         order by c.name collate nocase, c.id",
    )?;
    let rows = stmt.query_map(params![space_id], row_to_calendar)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn list_events(
    db: &Database,
    space_id: i64,
    from: i64,
    to: i64,
    limit: usize,
) -> AppResult<Vec<CalendarEvent>> {
    let conn = db.read();
    let mut stmt = conn.prepare(
        "select e.id, e.calendar_id, c.name, c.color, e.href,
                e.etag, e.uid, e.summary, e.description, e.location,
                e.starts_at, e.ends_at, e.all_day, c.read_only, e.recurrence
         from calendar_events e
         join calendars c on c.id = e.calendar_id
         join calendar_accounts ca on ca.id = c.calendar_account_id
         where ca.space_id = ?1
           and c.unsubscribed = 0
           and e.starts_at < ?3
           and (coalesce(e.ends_at, e.starts_at) >= ?2 or e.recurrence <> '')
         order by e.starts_at, e.id
         limit ?4",
    )?;
    let rows = stmt.query_map(
        params![space_id, from, to, limit.clamp(1, 500) as i64],
        |row| {
            Ok(CalendarEvent {
                id: row.get(0)?,
                calendar_id: row.get(1)?,
                calendar_name: row.get(2)?,
                calendar_color: row.get(3)?,
                href: row.get(4)?,
                etag: row.get(5)?,
                uid: row.get(6)?,
                summary: row.get(7)?,
                description: row.get(8)?,
                location: row.get(9)?,
                starts_at: row.get(10)?,
                ends_at: row.get(11)?,
                all_day: row.get::<_, i64>(12)? != 0,
                read_only: row.get::<_, i64>(13)? != 0,
                recurrence: row.get(14)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub(crate) fn save_calendars(
    db: &Database,
    calendar_account_id: i64,
    calendars: &[RemoteCalendar],
) -> AppResult<()> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    for calendar in calendars {
        let components = serde_json::to_string(&calendar.components)?;
        tx.execute(
            "insert into calendars
                (calendar_account_id, href, name, color, supported_components, read_only)
             values (?1, ?2, ?3, ?4, ?5, ?6)
             on conflict(calendar_account_id, href) do update set
                name = excluded.name,
                color = excluded.color,
                supported_components = excluded.supported_components,
                read_only = excluded.read_only",
            params![
                calendar_account_id,
                calendar.href,
                calendar.name,
                calendar.color,
                components,
                calendar.read_only as i64
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn apply_snapshot(
    db: &Database,
    calendar_id: i64,
    events: &[RemoteEvent],
) -> AppResult<()> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    tx.execute(
        "delete from calendar_events where calendar_id = ?1",
        params![calendar_id],
    )?;
    for event in events {
        upsert_event(&tx, calendar_id, event)?;
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn apply_delta(
    db: &Database,
    calendar_id: i64,
    added: &[RemoteEvent],
    modified: &[RemoteEvent],
    deleted: &[String],
) -> AppResult<()> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    for href in deleted {
        tx.execute(
            "delete from calendar_events where calendar_id = ?1 and href = ?2",
            params![calendar_id, href],
        )?;
    }
    for event in added.iter().chain(modified) {
        upsert_event(&tx, calendar_id, event)?;
    }
    tx.commit()?;
    Ok(())
}

fn upsert_event(
    tx: &rusqlite::Transaction<'_>,
    calendar_id: i64,
    event: &RemoteEvent,
) -> AppResult<()> {
    let parsed = parse_event(&event.data).unwrap_or_else(|_| ParsedEvent {
        uid: event.href.clone(),
        summary: "Untitled event".into(),
        description: String::new(),
        location: String::new(),
        starts_at: 0,
        ends_at: None,
        all_day: false,
        recurrence: String::new(),
    });
    tx.execute(
        "insert into calendar_events
            (calendar_id, href, etag, uid, summary, description, location,
             starts_at, ends_at, all_day, ical, updated_at, recurrence)
         values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         on conflict(calendar_id, href) do update set
            etag = excluded.etag,
            uid = excluded.uid,
            summary = excluded.summary,
            description = excluded.description,
            location = excluded.location,
            starts_at = excluded.starts_at,
            ends_at = excluded.ends_at,
            all_day = excluded.all_day,
            ical = excluded.ical,
            updated_at = excluded.updated_at,
            recurrence = excluded.recurrence",
        params![
            calendar_id,
            event.href,
            event.etag,
            parsed.uid,
            parsed.summary,
            parsed.description,
            parsed.location,
            parsed.starts_at,
            parsed.ends_at,
            parsed.all_day as i64,
            event.data,
            db::now(),
            parsed.recurrence,
        ],
    )?;
    Ok(())
}

fn date_value(value: &DatePerhapsTime) -> (i64, bool) {
    match value {
        DatePerhapsTime::Date(date) => (
            date.and_hms_opt(0, 0, 0)
                .map(|date_time| date_time.and_utc().timestamp())
                .unwrap_or(0),
            true,
        ),
        DatePerhapsTime::DateTime(CalendarDateTime::Utc(date_time)) => {
            (date_time.timestamp(), false)
        }
        DatePerhapsTime::DateTime(CalendarDateTime::Floating(date_time)) => {
            (date_time.and_utc().timestamp(), false)
        }
        DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone { date_time, .. }) => {
            (date_time.and_utc().timestamp(), false)
        }
    }
}

fn parse_event(ics: &str) -> AppResult<ParsedEvent> {
    let calendar: ICalendar = ics
        .parse()
        .map_err(|error| AppError::Message(format!("invalid calendar event: {error}")))?;
    let event = calendar
        .events()
        .next()
        .ok_or_else(|| AppError::Message("calendar resource has no VEVENT".into()))?;
    let (starts_at, all_day) = event
        .get_start()
        .as_ref()
        .map(date_value)
        .unwrap_or((0, false));
    let ends_at = event.get_end().as_ref().map(|value| date_value(value).0);
    Ok(ParsedEvent {
        uid: event.get_uid().unwrap_or_default().to_string(),
        summary: event.get_summary().unwrap_or("Untitled event").to_string(),
        description: event.get_description().unwrap_or_default().to_string(),
        location: event.get_location().unwrap_or_default().to_string(),
        starts_at,
        ends_at,
        all_day,
        recurrence: event.property_value("RRULE").unwrap_or_default().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::secrets::MemoryStore;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TIMED_EVENT: &str = "\
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//AuraMail//Test//EN
BEGIN:VEVENT
UID:event-1
SUMMARY:Team standup
DTSTART:20261028T090000Z
DTEND:20261028T093000Z
LOCATION:Room 2
DESCRIPTION:Daily sync
END:VEVENT
END:VCALENDAR
";

    const ALL_DAY_EVENT: &str = "\
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//AuraMail//Test//EN
BEGIN:VEVENT
UID:event-2
SUMMARY:Holiday
DTSTART;VALUE=DATE:20261029
DTEND;VALUE=DATE:20261030
END:VEVENT
END:VCALENDAR
";

    fn seed_account(db: &Database) -> (i64, i64) {
        let space_id = crate::spaces::ensure_default(db).unwrap().id;
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "insert into calendar_accounts
                (display_name, base_url, username, space_id, created_at)
             values ('Me', 'https://dav.example.com', 'me@example.com', ?1, 0)",
            rusqlite::params![space_id],
        )
        .unwrap();
        (space_id, conn.last_insert_rowid())
    }

    fn seed_calendar(db: &Database, space_id: i64, account_id: i64) -> i64 {
        save_calendars(
            db,
            account_id,
            &[RemoteCalendar {
                href: "/calendars/me/work/".into(),
                name: "Work".into(),
                color: Some("#6da4ff".into()),
                components: vec!["VEVENT".into()],
                read_only: false,
            }],
        )
        .unwrap();
        list_calendars(db, space_id).unwrap()[0].id
    }

    #[test]
    fn builds_standard_discovery_candidates() {
        assert_eq!(
            discovery_candidates("https://dav.example.com/"),
            vec![
                "https://dav.example.com/.well-known/caldav".to_string(),
                "https://dav.example.com/".to_string(),
            ]
        );
        assert_eq!(
            discovery_candidates("https://dav.example.com/remote.php/dav/"),
            vec![
                "https://dav.example.com/.well-known/caldav".to_string(),
                "https://dav.example.com/remote.php/dav/".to_string(),
            ]
        );
    }

    #[test]
    fn reorders_calendar_accounts_within_a_space() {
        let db = Database::open_memory().unwrap();
        let (space_id, first) = seed_account(&db);
        let (_, second) = seed_account(&db);

        let reordered = reorder_accounts(&db, space_id, &[second, first]).unwrap();

        assert_eq!(
            reordered.iter().map(|account| account.id).collect::<Vec<_>>(),
            vec![second, first]
        );
    }

    #[test]
    fn detects_read_only_privilege_sets() {
        assert!(!privileges_allow_write(&[
            Privilege::Read,
            Privilege::WriteProperties,
            Privilege::Other("read-acl".into()),
        ]));
        assert!(privileges_allow_write(&[
            Privilege::Read,
            Privilege::Write,
            Privilege::WriteContent,
        ]));
        assert!(privileges_allow_write(&[Privilege::Other("write".into())]));
    }

    #[test]
    fn parses_a_timed_event() {
        let event = parse_event(TIMED_EVENT).unwrap();

        assert_eq!(event.uid, "event-1");
        assert_eq!(event.summary, "Team standup");
        assert_eq!(event.location, "Room 2");
        assert_eq!(event.starts_at, 1_793_178_000);
        assert_eq!(event.ends_at, Some(1_793_179_800));
        assert!(!event.all_day);
    }

    #[test]
    fn builds_escaped_event_ics() {
        let ics = build_event_ics(
            &CalendarEventInput {
                summary: "Planning, review".into(),
                description: "Line one\nLine two".into(),
                location: "Room; 2".into(),
                starts_at: 1_793_178_000,
                ends_at: Some(1_793_179_800),
                all_day: false,
                recurrence: "FREQ=WEEKLY;BYDAY=MO".into(),
            },
            "event-3",
        )
        .unwrap();

        assert!(ics.contains("UID:event-3"));
        assert!(ics.contains("SUMMARY:Planning\\, review"));
        assert!(ics.contains("LOCATION:Room\\; 2"));
        assert!(ics.contains("DESCRIPTION:Line one\\nLine two"));
        assert!(ics.contains("DTSTART:20261028T090000Z"));
        assert!(ics.contains("DTEND:20261028T093000Z"));
        assert!(ics.contains("RRULE:FREQ=WEEKLY;BYDAY=MO"));
    }

    #[test]
    fn parses_an_all_day_event() {
        let event = parse_event(ALL_DAY_EVENT).unwrap();

        assert_eq!(event.summary, "Holiday");
        assert_eq!(event.starts_at, 1_793_232_000);
        assert_eq!(event.ends_at, Some(1_793_318_400));
        assert!(event.all_day);
    }

    #[test]
    fn stores_calendars_and_applies_sync_deltas() {
        let db = Database::open_memory().unwrap();
        let (space_id, account_id) = seed_account(&db);
        let calendar_id = seed_calendar(&db, space_id, account_id);

        apply_snapshot(
            &db,
            calendar_id,
            &[RemoteEvent {
                href: "/calendars/me/work/event-1.ics".into(),
                etag: Some("\"etag-1\"".into()),
                data: TIMED_EVENT.into(),
            }],
        )
        .unwrap();

        let events = list_events(&db, space_id, 1_793_000_000, 1_794_000_000, 50).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Team standup");
        assert_eq!(events[0].calendar_name, "Work");

        apply_delta(
            &db,
            calendar_id,
            &[],
            &[],
            &["/calendars/me/work/event-1.ics".into()],
        )
        .unwrap();

        assert!(list_events(&db, space_id, 1_793_000_000, 1_794_000_000, 50)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn unsubscribing_hides_a_calendar_and_its_events() {
        let db = Database::open_memory().unwrap();
        let (space_id, account_id) = seed_account(&db);
        let calendar_id = seed_calendar(&db, space_id, account_id);

        unsubscribe_calendar(&db, calendar_id).unwrap();

        assert!(list_calendars(&db, space_id).unwrap().is_empty());
        assert!(list_events(&db, space_id, 0, i64::MAX, 50)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn event_listing_is_limited_to_the_requested_window() {
        let db = Database::open_memory().unwrap();
        let (space_id, account_id) = seed_account(&db);
        let calendar_id = seed_calendar(&db, space_id, account_id);
        apply_snapshot(
            &db,
            calendar_id,
            &[RemoteEvent {
                href: "/calendars/me/work/event-1.ics".into(),
                etag: None,
                data: TIMED_EVENT.into(),
            }],
        )
        .unwrap();

        let events = list_events(&db, space_id, 1_793_200_000, 1_793_300_000, 50).unwrap();
        assert!(events.is_empty());
    }

    async fn mount_caldav_fixture(server: &MockServer) {
        let principal = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/</D:href>
    <D:propstat>
      <D:prop>
        <D:current-user-principal><D:href>/principals/me/</D:href></D:current-user-principal>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let homes = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/principals/me/</D:href>
    <D:propstat>
      <D:prop>
        <C:calendar-home-set><D:href>/calendars/me/</D:href></C:calendar-home-set>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let calendars = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:A="http://apple.com/ns/ical/">
  <D:response>
    <D:href>/calendars/me/work/</D:href>
    <D:propstat>
      <D:prop>
        <D:displayname>Work</D:displayname>
        <D:resourcetype><D:collection/><C:calendar/></D:resourcetype>
        <A:calendar-color>#6da4ff</A:calendar-color>
        <C:supported-calendar-component-set><C:comp name="VEVENT"/></C:supported-calendar-component-set>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let reports = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/calendars/me/work/</D:href>
    <D:propstat>
      <D:prop>
        <D:supported-report-set>
          <D:supported-report><D:report><C:calendar-query/></D:report></D:supported-report>
          <D:supported-report><D:report><D:sync-collection/></D:report></D:supported-report>
        </D:supported-report-set>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let sync = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/calendars/me/work/event-1.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-1"</D:getetag>
        <C:calendar-data>BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//AuraMail//Test//EN
BEGIN:VEVENT
UID:event-1
SUMMARY:Team standup
DTSTART:20261028T090000Z
DTEND:20261028T093000Z
END:VEVENT
END:VCALENDAR</C:calendar-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:sync-token>sync-token-1</D:sync-token>
</D:multistatus>"#;

        for (route, body) in [
            ("/", principal),
            ("/principals/me/", homes),
            ("/calendars/me/", calendars),
            ("/calendars/me/work/", reports),
        ] {
            Mock::given(method("PROPFIND"))
                .and(path(route))
                .respond_with(
                    ResponseTemplate::new(207)
                        .insert_header("content-type", "application/xml; charset=utf-8")
                        .set_body_string(body),
                )
                .mount(server)
                .await;
        }
        Mock::given(method("REPORT"))
            .and(path("/calendars/me/work/"))
            .respond_with(
                ResponseTemplate::new(207)
                    .insert_header("content-type", "application/xml; charset=utf-8")
                    .set_body_string(sync),
            )
            .mount(server)
            .await;
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(201))
            .mount(server)
            .await;
        Mock::given(method("DELETE"))
            .respond_with(ResponseTemplate::new(204))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn connects_and_syncs_against_a_caldav_server() {
        let server = MockServer::start().await;
        mount_caldav_fixture(&server).await;

        let db = Database::open_memory().unwrap();
        let space_id = crate::spaces::ensure_default(&db).unwrap().id;
        let secrets = MemoryStore::default();
        let account = add_account(
            &db,
            &secrets,
            CalendarAccountInput {
                display_name: "Me".into(),
                base_url: server.uri(),
                username: "me@example.com".into(),
                password: "secret".into(),
                space_id,
            },
        )
        .await
        .expect("add calendar account");
        let calendars = list_calendars(&db, space_id).expect("list calendars");
        assert_eq!(calendars.len(), 1);
        assert_eq!(calendars[0].name, "Work");

        let result = sync_account(&db, &secrets, account.id)
            .await
            .expect("sync calendar");
        assert_eq!(result.calendars, 1);
        assert_eq!(result.events, 1);
        assert!(result.errors.is_empty());

        let events = list_events(
            &db,
            space_id,
            1_793_000_000,
            1_794_000_000,
            50,
        )
        .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Team standup");
        assert_eq!(events[0].etag.as_deref(), Some("etag-1"));

        let created = create_event(
            &db,
            &secrets,
            calendars[0].id,
            CalendarEventInput {
                summary: "Planning".into(),
                description: String::new(),
                location: String::new(),
                starts_at: 1_793_178_000,
                ends_at: Some(1_793_179_800),
                all_day: false,
                recurrence: String::new(),
            },
        )
        .await
        .expect("create event");
        assert_eq!(created.events, 1);

        let event_id = events[0].id;
        let updated = update_event(
            &db,
            &secrets,
            event_id,
            CalendarEventInput {
                summary: "Team standup updated".into(),
                description: "Updated".into(),
                location: String::new(),
                starts_at: 1_793_178_000,
                ends_at: Some(1_793_179_800),
                all_day: false,
                recurrence: "FREQ=WEEKLY".into(),
            },
        )
        .await
        .expect("update event");
        assert_eq!(updated.events, 1);

        delete_event(&db, &secrets, event_id)
            .await
            .expect("delete event");

        let requests = server.received_requests().await.expect("requests");
        assert!(requests.iter().any(|request| request.method.as_str() == "PUT"));
        assert!(requests
            .iter()
            .any(|request| request.method.as_str() == "DELETE"));
    }
}
