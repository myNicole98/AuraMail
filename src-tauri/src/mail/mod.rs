pub mod parse;
pub mod preload;
pub mod render;

use std::collections::HashMap;

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::error::{AppError, AppResult};
use parse::ParsedHeaders;

const MESSAGE_COLUMNS: &str = "m.id, m.account_id, m.folder_id, m.uid, m.message_id, m.from_addr, m.from_name, m.to_addrs, m.cc_addrs, m.subject, m.date, m.snippet, m.is_seen, m.is_starred, m.has_attachments, m.body_cached, m.references_header, m.thread_key, (select c.tab_id from classifications c where c.message_id = m.id) as tab_id";

const SEARCH_MATCH: &str = r"lower(m.from_addr || ' ' || m.from_name || ' ' || m.to_addrs || ' ' || m.cc_addrs || ' ' || m.subject || ' ' || m.snippet || ' ' || coalesce(mb.text_body, '') || ' ' || coalesce(mb.html_body, '')) like ? escape '\'";

const DEDUPED_MESSAGES: &str = "select m.*, row_number() over (partition by m.account_id, coalesce(nullif(m.message_id, ''), 'id:' || m.id) order by case f.role when 'inbox' then 0 when 'sent' then 1 when 'archive' then 2 when 'all' then 9 else 5 end, m.body_cached desc, m.id) rn from messages m join folders f on f.id = m.folder_id";

/// Archive is the archive folders plus All Mail messages that do not have a
/// copy in another actionable folder. Keep the correlated lookup indexed by
/// (account_id, message_id); putting the folder lookup first forces a scan for
/// every All Mail row instead.
const ARCHIVE_MATCH: &str = "(f.role = 'archive' OR (f.role = 'all' AND NOT EXISTS (select 1 from messages mi join folders fi on fi.id = mi.folder_id where mi.account_id = m.account_id and mi.message_id = m.message_id and fi.role in ('inbox', 'sent', 'drafts', 'trash', 'junk'))))";

const FOLDER_COLUMNS: &str =
    "f.id, f.account_id, f.path, f.name, f.role, f.delimiter, f.uid_validity, f.highest_uid";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Folder {
    pub id: i64,
    pub account_id: i64,
    pub path: String,
    pub name: String,
    pub role: String,
    pub delimiter: String,
    pub uid_validity: u32,
    pub highest_uid: u32,
    pub unread_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Message {
    pub id: i64,
    pub account_id: i64,
    pub folder_id: i64,
    pub uid: u32,
    pub message_id: Option<String>,
    pub references: Option<String>,
    pub thread_key: String,
    pub from_addr: String,
    pub from_name: String,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub subject: String,
    pub date: i64,
    pub snippet: String,
    pub is_seen: bool,
    pub is_starred: bool,
    pub has_attachments: bool,
    pub body_cached: bool,
    pub tab_id: Option<i64>,
}

impl Message {
    /// Stable identity for duplicates of the same mail across folders.
    pub fn identity(&self) -> String {
        self.message_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("id:{}", self.id))
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Attachment {
    pub id: i64,
    pub message_id: i64,
    pub filename: String,
    pub mime_type: String,
    pub size: i64,
    pub content_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ThreadMessage {
    #[serde(flatten)]
    pub message: Message,
    pub thread_count: i64,
    pub thread_unread_count: i64,
    pub thread_has_inbox: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MessageDetail {
    pub message: Message,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
    pub image_html: Option<String>,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MessageQuery {
    pub folder_id: Option<i64>,
    pub account_id: Option<i64>,
    pub space_id: Option<i64>,
    pub tab_id: Option<i64>,
    pub unified: Option<String>,
    pub search: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub order: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TabUnreadCount {
    pub tab_id: i64,
    pub unread: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TabUnreadCounts {
    pub all: i64,
    pub tabs: Vec<TabUnreadCount>,
}

#[derive(Debug, Clone)]
pub struct MessageHeader {
    pub uid: u32,
    pub headers: ParsedHeaders,
    pub is_seen: bool,
    pub is_starred: bool,
}

pub fn upsert_folder(
    db: &Database,
    account_id: i64,
    path: &str,
    name: &str,
    role: &str,
    delimiter: &str,
) -> AppResult<Folder> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "insert into folders (account_id, path, name, role, delimiter, uid_validity, highest_uid) values (?1, ?2, ?3, ?4, ?5, 0, 0) on conflict(account_id, path) do update set name = excluded.name, role = excluded.role, delimiter = excluded.delimiter",
        params![account_id, path, name, role, delimiter],
    )?;
    let id: i64 = conn.query_row(
        "select id from folders where account_id = ?1 and path = ?2",
        params![account_id, path],
        |row| row.get(0),
    )?;
    folder_by_id_conn(&conn, id)?.ok_or_else(|| AppError::Message("folder not found".into()))
}

pub fn reconcile_folders(db: &Database, account_id: i64, remote_paths: &[String]) -> AppResult<()> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    let local: Vec<(i64, String)> = {
        let mut stmt = tx.prepare("select id, path from folders where account_id = ?1")?;
        let rows = stmt
            .query_map(params![account_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (id, path) in local {
        if !remote_paths.iter().any(|remote| remote == &path) {
            tx.execute("delete from folders where id = ?1", params![id])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn folder_by_id(db: &Database, id: i64) -> AppResult<Option<Folder>> {
    let conn = db.conn.lock().unwrap();
    folder_by_id_conn(&conn, id)
}

fn folder_by_id_conn(conn: &Connection, id: i64) -> AppResult<Option<Folder>> {
    let sql = format!(
        "select {FOLDER_COLUMNS}, coalesce(sum(case when m.is_seen = 0 then 1 else 0 end), 0) from folders f left join messages m on m.folder_id = f.id where f.id = ?1 group by f.id"
    );
    let folder = conn
        .query_row(&sql, params![id], row_to_folder)
        .optional()?;
    Ok(folder)
}

pub fn list_folders(
    db: &Database,
    account_id: Option<i64>,
    space_id: Option<i64>,
) -> AppResult<Vec<Folder>> {
    let conn = db.conn.lock().unwrap();
    let sql = format!(
        "select {FOLDER_COLUMNS}, coalesce(sum(case when m.is_seen = 0 then 1 else 0 end), 0) from folders f left join messages m on m.folder_id = f.id where (?1 is null or f.account_id = ?1) and (?2 is null or f.account_id in (select id from accounts where space_id = ?2)) group by f.id order by f.account_id, case f.role when 'inbox' then 0 when 'drafts' then 1 when 'sent' then 2 when 'trash' then 3 else 4 end, f.name collate nocase"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![account_id, space_id], row_to_folder)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn folder_cursor(db: &Database, folder_id: i64) -> AppResult<(u32, u32)> {
    let conn = db.conn.lock().unwrap();
    let cursor = conn.query_row(
        "select uid_validity, highest_uid from folders where id = ?1",
        params![folder_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(cursor)
}

pub fn apply_folder_headers(
    db: &Database,
    account_id: i64,
    folder_id: i64,
    headers: &[MessageHeader],
    uid_validity: u32,
    highest_uid: u32,
) -> AppResult<Vec<i64>> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    let mut inserted = Vec::new();
    for header in headers {
        if let Some(id) = write_message(
            &tx,
            account_id,
            folder_id,
            header.uid,
            &header.headers,
            header.is_seen,
            header.is_starred,
        )? {
            inserted.push(id);
        }
    }
    write_cursor(&tx, folder_id, uid_validity, highest_uid)?;
    tx.commit()?;
    Ok(inserted)
}

pub fn replace_folder_messages(
    db: &Database,
    account_id: i64,
    folder_id: i64,
    headers: &[MessageHeader],
    uid_validity: u32,
    highest_uid: u32,
) -> AppResult<Vec<i64>> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    tx.execute(
        "delete from messages where folder_id = ?1",
        params![folder_id],
    )?;
    let mut inserted = Vec::new();
    for header in headers {
        if let Some(id) = write_message(
            &tx,
            account_id,
            folder_id,
            header.uid,
            &header.headers,
            header.is_seen,
            header.is_starred,
        )? {
            inserted.push(id);
        }
    }
    write_cursor(&tx, folder_id, uid_validity, highest_uid)?;
    tx.commit()?;
    Ok(inserted)
}

fn write_message(
    conn: &Connection,
    account_id: i64,
    folder_id: i64,
    uid: u32,
    headers: &ParsedHeaders,
    is_seen: bool,
    is_starred: bool,
) -> AppResult<Option<i64>> {
    let exists: bool = conn.query_row(
        "select exists(select 1 from messages where folder_id = ?1 and uid = ?2)",
        params![folder_id, uid],
        |row| row.get(0),
    )?;
    let to_addrs = serde_json::to_string(&headers.to_addrs)?;
    let cc_addrs = serde_json::to_string(&headers.cc_addrs)?;
    let thread_key =
        crate::db::message_thread_key(headers.message_id.as_deref(), headers.references.as_deref())
            .unwrap_or_else(|| format!("local:{account_id}:{folder_id}:{uid}"));
    conn.execute(
        "insert into messages (account_id, folder_id, uid, message_id, references_header, thread_key, from_addr, from_name, to_addrs, cc_addrs, subject, date, snippet, is_seen, is_starred, has_attachments, body_cached) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 0) on conflict(folder_id, uid) do update set account_id = excluded.account_id, message_id = excluded.message_id, references_header = excluded.references_header, thread_key = excluded.thread_key, from_addr = excluded.from_addr, from_name = excluded.from_name, to_addrs = excluded.to_addrs, cc_addrs = excluded.cc_addrs, subject = excluded.subject, date = excluded.date, snippet = excluded.snippet, is_seen = excluded.is_seen, is_starred = excluded.is_starred, has_attachments = excluded.has_attachments",
        params![
            account_id,
            folder_id,
            uid,
            headers.message_id,
            headers.references,
            thread_key,
            headers.from_addr,
            headers.from_name,
            to_addrs,
            cc_addrs,
            headers.subject,
            headers.date,
            headers.snippet,
            is_seen as i64,
            is_starred as i64,
            headers.has_attachments as i64,
        ],
    )?;
    Ok((!exists).then(|| conn.last_insert_rowid()))
}

fn write_cursor(
    conn: &Connection,
    folder_id: i64,
    uid_validity: u32,
    highest_uid: u32,
) -> AppResult<()> {
    conn.execute(
        "update folders set uid_validity = ?1, highest_uid = ?2 where id = ?3",
        params![uid_validity, highest_uid, folder_id],
    )?;
    Ok(())
}

pub fn unread_inbox_count(db: &Database) -> AppResult<i64> {
    let conn = db.conn.lock().unwrap();
    Ok(conn.query_row(
        "select count(*) from messages m join folders f on f.id = m.folder_id where f.role = 'inbox' and m.is_seen = 0",
        [],
        |row| row.get(0),
    )?)
}

pub fn unread_inbox_count_for_space(db: &Database, space_id: i64) -> AppResult<i64> {
    let conn = db.conn.lock().unwrap();
    Ok(conn.query_row(
        "select count(*) from messages m join folders f on f.id = m.folder_id join accounts a on a.id = m.account_id where f.role = 'inbox' and m.is_seen = 0 and a.space_id = ?1",
        params![space_id],
        |row| row.get(0),
    )?)
}

pub fn tab_unread_counts(db: &Database, query: &MessageQuery) -> AppResult<TabUnreadCounts> {
    let mut conditions = vec!["m.is_seen = 0".to_string()];
    let mut values: Vec<Value> = Vec::new();
    match query.folder_id {
        Some(folder_id) => {
            conditions.push("m.folder_id = ?".to_string());
            values.push(Value::Integer(folder_id));
        }
        None => match query.unified.as_deref().unwrap_or("inbox") {
            "archive" => conditions.push(ARCHIVE_MATCH.to_string()),
            "sent" => conditions.push("f.role = 'sent'".to_string()),
            "spam" => conditions.push("f.role = 'junk'".to_string()),
            "trash" => conditions.push("f.role = 'trash'".to_string()),
            "starred" => conditions.push(
                "(m.is_starred = 1 AND (f.role != 'all' OR NOT EXISTS (select 1 from messages mi join folders fi on fi.id = mi.folder_id where fi.account_id = m.account_id and fi.role = 'inbox' and mi.message_id is not null and mi.message_id = m.message_id)))"
                    .to_string(),
            ),
            _ => conditions.push("f.role = 'inbox'".to_string()),
        },
    }
    if let Some(account_id) = query.account_id {
        conditions.push("m.account_id = ?".to_string());
        values.push(Value::Integer(account_id));
    }
    if let Some(space_id) = query.space_id {
        conditions.push(
            "m.account_id in (select id from accounts where space_id = ?)".to_string(),
        );
        values.push(Value::Integer(space_id));
    }
    let scope = conditions.join(" and ");
    let thread = "m.account_id || ':' || coalesce(nullif(m.thread_key, ''), 'id:' || m.id)";
    let conn = db.conn.lock().unwrap();
    let all = conn.query_row(
        &format!(
            "select count(distinct {thread}) from messages m join folders f on f.id = m.folder_id where {scope}"
        ),
        params_from_iter(values.iter()),
        |row| row.get(0),
    )?;
    let sql = format!(
        "select c.tab_id as tab_id, count(distinct {thread}) from messages m join folders f on f.id = m.folder_id join classifications c on c.message_id = m.id where {scope} group by c.tab_id order by c.tab_id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(values.iter()), |row| {
        Ok(TabUnreadCount {
            tab_id: row.get(0)?,
            unread: row.get(1)?,
        })
    })?;
    let tabs = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(TabUnreadCounts { all, tabs })
}

pub fn list_messages(db: &Database, query: &MessageQuery) -> AppResult<Vec<Message>> {
    let search = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|search| !search.is_empty());
    let universal = search.is_some();
    let mut sql =
        format!("select {MESSAGE_COLUMNS} from messages m join folders f on f.id = m.folder_id");
    if !universal && query.tab_id.is_some() {
        sql.push_str(" left join classifications c on c.message_id = m.id");
    }
    if universal {
        sql.push_str(" left join message_bodies mb on mb.message_id = m.id");
    }

    let mut conditions = Vec::new();
    let mut values: Vec<Value> = Vec::new();

    if !universal {
        match query.folder_id {
            Some(folder_id) => {
                conditions.push("m.folder_id = ?".to_string());
                values.push(Value::Integer(folder_id));
            }
            None => match query.unified.as_deref().unwrap_or("inbox") {
                "archive" => conditions.push(ARCHIVE_MATCH.to_string()),
                "sent" => conditions.push("f.role = 'sent'".to_string()),
                "spam" => conditions.push("f.role = 'junk'".to_string()),
                "trash" => conditions.push("f.role = 'trash'".to_string()),
                "starred" => conditions.push(
                    "(m.is_starred = 1 AND (f.role != 'all' OR NOT EXISTS (select 1 from messages mi join folders fi on fi.id = mi.folder_id where fi.account_id = m.account_id and fi.role = 'inbox' and mi.message_id is not null and mi.message_id = m.message_id)))"
                        .to_string(),
                ),
                _ => conditions.push("f.role = 'inbox'".to_string()),
            },
        }
        if let Some(account_id) = query.account_id {
            conditions.push("m.account_id = ?".to_string());
            values.push(Value::Integer(account_id));
        }
        if let Some(tab_id) = query.tab_id {
            conditions.push("c.tab_id = ?".to_string());
            values.push(Value::Integer(tab_id));
        }
    }
    if let Some(space_id) = query.space_id {
        conditions.push(
            "m.account_id in (select id from accounts where space_id = ?)".to_string(),
        );
        values.push(Value::Integer(space_id));
    }
    if let Some(search) = search {
        conditions.push(SEARCH_MATCH.to_string());
        values.push(Value::Text(like_pattern(search)));
    }

    if !conditions.is_empty() {
        sql.push_str(" where ");
        sql.push_str(&conditions.join(" and "));
    }
    let direction = if query.order.as_deref() == Some("oldest") {
        "asc"
    } else {
        "desc"
    };
    sql.push_str(&format!(
        " order by m.date {direction}, m.id {direction} limit ? offset ?"
    ));

    let limit = query.limit.unwrap_or(100).clamp(1, 5000);
    let offset = query.offset.unwrap_or(0);
    values.push(Value::Integer(limit as i64));
    values.push(Value::Integer(offset as i64));

    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(values.iter()), row_to_message)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn list_threads(db: &Database, query: &MessageQuery) -> AppResult<Vec<ThreadMessage>> {
    let search = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|search| !search.is_empty());
    let universal = search.is_some();
    let mut matched =
        "select distinct m.account_id, m.thread_key from messages m join folders f on f.id = m.folder_id".to_string();
    if !universal && query.tab_id.is_some() {
        matched.push_str(" left join classifications c on c.message_id = m.id");
    }
    if universal {
        matched.push_str(" left join message_bodies mb on mb.message_id = m.id");
    }

    let mut conditions = Vec::new();
    let mut values: Vec<Value> = Vec::new();

    if !universal {
        match query.folder_id {
            Some(folder_id) => {
                conditions.push("m.folder_id = ?".to_string());
                values.push(Value::Integer(folder_id));
            }
            None => match query.unified.as_deref().unwrap_or("inbox") {
                "archive" => conditions.push(ARCHIVE_MATCH.to_string()),
                "sent" => conditions.push("f.role = 'sent'".to_string()),
                "spam" => conditions.push("f.role = 'junk'".to_string()),
                "trash" => conditions.push("f.role = 'trash'".to_string()),
                "starred" => conditions.push(
                    "(m.is_starred = 1 AND (f.role != 'all' OR NOT EXISTS (select 1 from messages mi join folders fi on fi.id = mi.folder_id where fi.account_id = m.account_id and fi.role = 'inbox' and mi.message_id is not null and mi.message_id = m.message_id)))"
                        .to_string(),
                ),
                _ => conditions.push("f.role = 'inbox'".to_string()),
            },
        }
        if let Some(account_id) = query.account_id {
            conditions.push("m.account_id = ?".to_string());
            values.push(Value::Integer(account_id));
        }
        if let Some(tab_id) = query.tab_id {
            conditions.push("c.tab_id = ?".to_string());
            values.push(Value::Integer(tab_id));
        }
    }
    if let Some(space_id) = query.space_id {
        conditions.push(
            "m.account_id in (select id from accounts where space_id = ?)".to_string(),
        );
        values.push(Value::Integer(space_id));
    }
    if let Some(search) = search {
        conditions.push(SEARCH_MATCH.to_string());
        values.push(Value::Text(like_pattern(search)));
    }

    if !conditions.is_empty() {
        matched.push_str(" where ");
        matched.push_str(&conditions.join(" and "));
    }

    let direction = if query.order.as_deref() == Some("oldest") {
        "asc"
    } else {
        "desc"
    };
    let sql = format!(
        "with matched as ({matched}), deduped as ({DEDUPED_MESSAGES}), threads as (select d.id, d.account_id, d.thread_key, row_number() over(partition by d.account_id, d.thread_key order by d.date desc, d.id desc) rn from deduped d join matched mt on mt.account_id = d.account_id and mt.thread_key = d.thread_key where d.rn = 1) select {MESSAGE_COLUMNS}, (select count(distinct coalesce(nullif(mx.message_id, ''), 'id:' || mx.id)) from messages mx where mx.account_id = m.account_id and mx.thread_key = m.thread_key), (select count(distinct case when mx.is_seen = 0 then coalesce(nullif(mx.message_id, ''), 'id:' || mx.id) end) from messages mx where mx.account_id = m.account_id and mx.thread_key = m.thread_key), (select exists(select 1 from messages mx join folders fx on fx.id = mx.folder_id where mx.account_id = m.account_id and mx.thread_key = m.thread_key and fx.role = 'inbox')) from deduped m join threads t on t.id = m.id and t.rn = 1 where m.rn = 1 order by m.date {direction}, m.id {direction} limit ? offset ?"
    );

    let limit = query.limit.unwrap_or(100).clamp(1, 5000);
    let offset = query.offset.unwrap_or(0);
    values.push(Value::Integer(limit as i64));
    values.push(Value::Integer(offset as i64));

    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(values.iter()), |row| {
        Ok(ThreadMessage {
            message: row_to_message(row)?,
            thread_count: row.get(19)?,
            thread_unread_count: row.get(20)?,
            thread_has_inbox: row.get::<_, i64>(21)? != 0,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn thread_message_ids_in_role(
    db: &Database,
    account_id: i64,
    thread_key: &str,
    role: &str,
) -> AppResult<Vec<i64>> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "select m.id from messages m join folders f on f.id = m.folder_id where m.account_id = ?1 and m.thread_key = ?2 and f.role = ?3 order by m.date desc, m.id desc",
    )?;
    let rows = stmt.query_map(params![account_id, thread_key, role], |row| row.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn list_thread_messages(
    db: &Database,
    account_id: i64,
    thread_key: &str,
) -> AppResult<Vec<Message>> {
    let sql = format!(
        "with deduped as ({DEDUPED_MESSAGES}) select {MESSAGE_COLUMNS} from deduped m where m.rn = 1 and m.account_id = ?1 and m.thread_key = ?2 order by m.date desc, m.id desc"
    );
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![account_id, thread_key], row_to_message)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
pub fn unclassified_message_ids(
    db: &Database,
    account_id: Option<i64>,
    limit: u32,
) -> AppResult<Vec<i64>> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "select m.id from messages m left join classifications c on c.message_id = m.id where (?1 is null or m.account_id = ?1) and (c.message_id is null or c.tab_id is null) order by m.date desc, m.id desc limit ?2",
    )?;
    let rows = stmt.query_map(params![account_id, limit.clamp(1, 100) as i64], |row| {
        row.get(0)
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn unclassified_message_ids_before(
    db: &Database,
    account_id: Option<i64>,
    folder_id: Option<i64>,
    before_id: Option<i64>,
    limit: u32,
) -> AppResult<Vec<i64>> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "select m.id from messages m left join classifications c on c.message_id = m.id where (?1 is null or m.account_id = ?1) and (?2 is null or m.folder_id = ?2) and (?3 is null or m.id < ?3) and (c.message_id is null or c.tab_id is null) order by m.id desc limit ?4",
    )?;
    let rows = stmt.query_map(
        params![account_id, folder_id, before_id, limit.clamp(1, 100) as i64],
        |row| row.get(0),
    )?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get_message_detail(db: &Database, message_id: i64) -> AppResult<MessageDetail> {
    let conn = db.conn.lock().unwrap();
    let sql = format!("select {MESSAGE_COLUMNS} from messages m where m.id = ?1");
    let message = conn
        .query_row(&sql, params![message_id], row_to_message)
        .optional()?
        .ok_or_else(|| AppError::Message("message not found".into()))?;
    let body: Option<(Option<String>, Option<String>, Option<String>)> = conn
        .query_row(
            "select text_body, html_body, image_html from message_bodies where message_id = ?1",
            params![message_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let mut attachments = conn.prepare(
        "select id, message_id, filename, mime_type, size, content_id from attachments where message_id = ?1 order by id",
    )?;
    let attachments = attachments
        .query_map(params![message_id], |row| {
            Ok(Attachment {
                id: row.get(0)?,
                message_id: row.get(1)?,
                filename: row.get(2)?,
                mime_type: row.get(3)?,
                size: row.get(4)?,
                content_id: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(MessageDetail {
        message,
        text_body: body.as_ref().and_then(|body| body.0.clone()),
        html_body: body.as_ref().and_then(|body| body.1.clone()),
        image_html: body.and_then(|body| body.2),
        attachments,
    })
}

pub fn message_by_id(db: &Database, message_id: i64) -> AppResult<Message> {
    let conn = db.conn.lock().unwrap();
    let sql = format!("select {MESSAGE_COLUMNS} from messages m where m.id = ?1");
    conn.query_row(&sql, params![message_id], row_to_message)
        .optional()?
        .ok_or_else(|| AppError::Message("message not found".into()))
}

pub fn cache_message_body(
    db: &Database,
    message_id: i64,
    parsed: &parse::ParsedMessage,
) -> AppResult<MessageDetail> {
    let mut conn = db.conn.lock().unwrap();
    let exists: bool = conn.query_row(
        "select exists(select 1 from messages where id = ?1)",
        params![message_id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(AppError::Message("message not found".into()));
    }

    let tx = conn.transaction()?;
    tx.execute(
        "delete from attachments where message_id = ?1",
        params![message_id],
    )?;
    for attachment in &parsed.attachments {
        tx.execute(
            "insert into attachments (message_id, filename, mime_type, size, content_id) values (?1, ?2, ?3, ?4, ?5)",
            params![
                message_id,
                attachment.filename,
                attachment.mime_type,
                attachment.size as i64,
                attachment.content_id,
            ],
        )?;
    }
    let text_body = (!parsed.text_body.is_empty()).then_some(parsed.text_body.as_str());
    tx.execute(
        "insert into message_bodies (message_id, text_body, html_body, fetched_at) values (?1, ?2, ?3, ?4) on conflict(message_id) do update set text_body = excluded.text_body, html_body = excluded.html_body, image_html = null, fetched_at = excluded.fetched_at",
        params![message_id, text_body, parsed.html_body, crate::db::now()],
    )?;
    tx.execute(
        "update messages set body_cached = 1, has_attachments = ?2, snippet = case when trim(snippet) = '' then ?3 else snippet end where id = ?1",
        params![
            message_id,
            !parsed.attachments.is_empty(),
            parsed.snippet
        ],
    )?;
    tx.commit()?;
    drop(conn);

    get_message_detail(db, message_id)
}

pub fn cache_image_html(db: &Database, message_id: i64, html: &str) -> AppResult<()> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "update message_bodies set image_html = ?2 where message_id = ?1",
        params![message_id, html],
    )?;
    Ok(())
}

pub fn set_message_seen(db: &Database, message_id: i64, seen: bool) -> AppResult<Message> {
    update_message_flag(db, message_id, "is_seen", seen)
}

pub fn set_message_starred(db: &Database, message_id: i64, starred: bool) -> AppResult<Message> {
    update_message_flag(db, message_id, "is_starred", starred)
}

fn update_message_flag(
    db: &Database,
    message_id: i64,
    column: &str,
    value: bool,
) -> AppResult<Message> {
    let sql = format!("update messages set {column} = ?2 where id = ?1");
    let updated = db
        .conn
        .lock()
        .unwrap()
        .execute(&sql, params![message_id, value as i64])?;
    if updated == 0 {
        return Err(AppError::Message("message not found".into()));
    }
    message_by_id(db, message_id)
}

pub fn reconcile_folder_messages(
    db: &Database,
    folder_id: i64,
    remote: &[(u32, bool, bool)],
) -> AppResult<usize> {
    let mut conn = db.conn.lock().unwrap();
    let remote_flags: HashMap<u32, (bool, bool)> = remote
        .iter()
        .map(|(uid, seen, starred)| (*uid, (*seen, *starred)))
        .collect();
    let local: Vec<(i64, u32, bool, bool)> = {
        let mut stmt =
            conn.prepare("select id, uid, is_seen, is_starred from messages where folder_id = ?1")?;
        let rows = stmt.query_map(params![folder_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, i64>(3)? != 0,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let tx = conn.transaction()?;
    let mut changed = 0;
    for (id, uid, seen, starred) in local {
        match remote_flags.get(&uid) {
            None => {
                tx.execute("delete from messages where id = ?1", params![id])?;
                changed += 1;
            }
            Some((remote_seen, remote_starred))
                if *remote_seen != seen || *remote_starred != starred =>
            {
                tx.execute(
                    "update messages set is_seen = ?2, is_starred = ?3 where id = ?1",
                    params![id, *remote_seen as i64, *remote_starred as i64],
                )?;
                changed += 1;
            }
            Some(_) => {}
        }
    }
    tx.commit()?;
    Ok(changed)
}

pub fn set_message_snippet(db: &Database, message_id: i64, snippet: &str) -> AppResult<()> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "update messages set snippet = ?2 where id = ?1",
        params![message_id, snippet],
    )?;
    Ok(())
}

pub fn remove_message(db: &Database, message_id: i64) -> AppResult<()> {
    db.conn
        .lock()
        .unwrap()
        .execute("delete from messages where id = ?1", params![message_id])?;
    Ok(())
}

pub fn attachment_by_id(db: &Database, attachment_id: i64) -> AppResult<Attachment> {
    let conn = db.conn.lock().unwrap();
    conn.query_row(
        "select id, message_id, filename, mime_type, size, content_id from attachments where id = ?1",
        params![attachment_id],
        |row| {
            Ok(Attachment {
                id: row.get(0)?,
                message_id: row.get(1)?,
                filename: row.get(2)?,
                mime_type: row.get(3)?,
                size: row.get(4)?,
                content_id: row.get(5)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::Message("attachment not found".into()))
}

pub fn set_attachment_cache_path(
    db: &Database,
    attachment_id: i64,
    cache_path: &str,
) -> AppResult<()> {
    db.conn.lock().unwrap().execute(
        "update attachments set cache_path = ?2 where id = ?1",
        params![attachment_id, cache_path],
    )?;
    Ok(())
}

pub fn attachment_cache_path(db: &Database, attachment_id: i64) -> AppResult<Option<String>> {
    let conn = db.conn.lock().unwrap();
    Ok(conn
        .query_row(
            "select cache_path from attachments where id = ?1",
            params![attachment_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten())
}

fn folder_path_by_role(db: &Database, account_id: i64, role: &str) -> AppResult<Option<String>> {
    let conn = db.conn.lock().unwrap();
    Ok(conn
        .query_row(
            "select path from folders where account_id = ?1 and role = ?2 order by id limit 1",
            params![account_id, role],
            |row| row.get(0),
        )
        .optional()?)
}

pub fn trash_folder_path(db: &Database, account_id: i64) -> AppResult<Option<String>> {
    folder_path_by_role(db, account_id, "trash")
}

pub fn archive_folder_path(db: &Database, account_id: i64) -> AppResult<Option<String>> {
    folder_path_by_role(db, account_id, "archive")
}

pub fn all_folder_path(db: &Database, account_id: i64) -> AppResult<Option<String>> {
    folder_path_by_role(db, account_id, "all")
}

pub fn safe_attachment_name(filename: &str) -> String {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let cleaned: String = base
        .chars()
        .filter(|character| {
            character.is_alphanumeric() || matches!(character, '.' | '-' | '_' | ' ')
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.');
    let truncated: String = cleaned.chars().take(120).collect();
    if truncated.is_empty() {
        "attachment".into()
    } else {
        truncated
    }
}

fn like_pattern(search: &str) -> String {
    let escaped = search
        .to_lowercase()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

fn row_to_folder(row: &rusqlite::Row<'_>) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: row.get(0)?,
        account_id: row.get(1)?,
        path: row.get(2)?,
        name: row.get(3)?,
        role: row.get(4)?,
        delimiter: row.get(5)?,
        uid_validity: row.get(6)?,
        highest_uid: row.get(7)?,
        unread_count: row.get(8)?,
    })
}

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    let to_addrs: String = row.get(7)?;
    let cc_addrs: String = row.get(8)?;
    Ok(Message {
        id: row.get(0)?,
        account_id: row.get(1)?,
        folder_id: row.get(2)?,
        uid: row.get(3)?,
        message_id: row.get(4)?,
        references: row.get(16)?,
        thread_key: row.get(17)?,
        from_addr: row.get(5)?,
        from_name: row.get(6)?,
        to_addrs: serde_json::from_str(&to_addrs).unwrap_or_default(),
        cc_addrs: serde_json::from_str(&cc_addrs).unwrap_or_default(),
        subject: row.get(9)?,
        date: row.get(10)?,
        snippet: row.get(11)?,
        is_seen: row.get::<_, i64>(12)? != 0,
        is_starred: row.get::<_, i64>(13)? != 0,
        has_attachments: row.get::<_, i64>(14)? != 0,
        body_cached: row.get::<_, i64>(15)? != 0,
        tab_id: row.get(18)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn sample_message(id: i64, message_id: Option<&str>) -> Message {
        Message {
            id,
            account_id: 1,
            folder_id: 1,
            uid: id as u32,
            message_id: message_id.map(str::to_string),
            references: None,
            thread_key: "thread".into(),
            from_addr: "ada@example.com".into(),
            from_name: "Ada".into(),
            to_addrs: Vec::new(),
            cc_addrs: Vec::new(),
            subject: "Hello".into(),
            date: 0,
            snippet: String::new(),
            is_seen: false,
            is_starred: false,
            has_attachments: false,
            body_cached: false,
            tab_id: None,
        }
    }

    #[test]
    fn message_identity_uses_message_id_and_falls_back_to_row_id() {
        assert_eq!(
            sample_message(3, Some(" <one@example.com> ")).identity(),
            "<one@example.com>"
        );
        assert_eq!(sample_message(7, None).identity(), "id:7");
        assert_eq!(sample_message(8, Some("   ")).identity(), "id:8");
    }

    fn seed_account(db: &Database, email: &str) -> i64 {
        let space_id = crate::spaces::default_id(db).unwrap();
        seed_account_in_space(db, space_id, email)
    }

    fn seed_account_in_space(db: &Database, space_id: i64, email: &str) -> i64 {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "insert into accounts (display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, sync_interval_secs, space_id, created_at) values (?1, ?2, ?2, 'imap.example.com', 993, 'tls', 'smtp.example.com', 465, 'tls', 120, ?3, 0)",
            params![email, email, space_id],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn messages_and_counts_are_scoped_to_the_requested_space() {
        let db = Database::open_memory().unwrap();
        let default = crate::spaces::ensure_default(&db).unwrap();
        let work = crate::spaces::create(&db, "Work").unwrap();
        let personal_account = seed_account_in_space(&db, default.id, "personal@example.com");
        let personal_inbox = seed_folder(&db, personal_account, "INBOX", "inbox");
        seed_message(&db, personal_inbox, 1, "ada@example.com", "Personal", "", 100);
        let work_account = seed_account_in_space(&db, work.id, "work@example.com");
        let work_inbox = seed_folder(&db, work_account, "INBOX", "inbox");
        seed_message(&db, work_inbox, 1, "grace@example.com", "Work", "", 200);

        let personal = list_messages(
            &db,
            &MessageQuery {
                space_id: Some(default.id),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        let work_mail = list_messages(
            &db,
            &MessageQuery {
                space_id: Some(work.id),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(personal.len(), 1);
        assert_eq!(work_mail.len(), 1);
        assert_eq!(personal[0].account_id, personal_account);
        assert_eq!(work_mail[0].account_id, work_account);
        assert_eq!(unread_inbox_count_for_space(&db, default.id).unwrap(), 1);
        assert_eq!(unread_inbox_count_for_space(&db, work.id).unwrap(), 1);
        assert_eq!(list_folders(&db, None, Some(default.id)).unwrap().len(), 1);
        assert_eq!(list_folders(&db, None, Some(work.id)).unwrap().len(), 1);
        let personal_counts = tab_unread_counts(
            &db,
            &MessageQuery {
                space_id: Some(default.id),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        let work_counts = tab_unread_counts(
            &db,
            &MessageQuery {
                space_id: Some(work.id),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!((personal_counts.all, work_counts.all), (1, 1));
    }

    fn seed_folder(db: &Database, account_id: i64, path: &str, role: &str) -> i64 {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "insert into folders (account_id, path, name, role, delimiter) values (?1, ?2, ?2, ?3, '/')",
            params![account_id, path, role],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn seed_message(
        db: &Database,
        folder_id: i64,
        uid: i64,
        from_addr: &str,
        subject: &str,
        snippet: &str,
        date: i64,
    ) {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "insert into messages (account_id, folder_id, uid, from_addr, from_name, subject, snippet, date, is_seen) select account_id, id, ?2, ?3, '', ?4, ?5, ?6, 0 from folders where id = ?1",
            params![folder_id, uid, from_addr, subject, snippet, date],
        )
        .unwrap();
    }

    #[test]
    fn threads_combine_inbox_and_sent_and_sort_latest_first() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        let sent = seed_folder(&db, account, "Sent", "sent");
        let all = seed_folder(&db, account, "[Gmail]/All Mail", "all");
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "insert into messages (account_id, folder_id, uid, message_id, references_header, thread_key, from_addr, from_name, subject, snippet, date, is_seen) values (?1, ?2, 1, '<root@example.com>', null, 'root@example.com', 'ada@example.com', 'Ada', 'Hello', 'root', 100, 0)",
                params![account, inbox],
            )
            .unwrap();
            conn.execute(
                "insert into messages (account_id, folder_id, uid, message_id, references_header, thread_key, from_addr, from_name, subject, snippet, date, is_seen) values (?1, ?2, 2, '<reply@example.com>', '<root@example.com>', 'root@example.com', 'me@example.com', 'Me', 'Re: Hello', 'reply', 200, 1)",
                params![account, sent],
            )
            .unwrap();
            conn.execute(
                "insert into messages (account_id, folder_id, uid, message_id, references_header, thread_key, from_addr, from_name, subject, snippet, date, is_seen) values (?1, ?2, 3, '<other@example.com>', null, 'other@example.com', 'grace@example.com', 'Grace', 'Other', 'other', 150, 0)",
                params![account, inbox],
            )
            .unwrap();
            conn.execute(
                "insert into messages (account_id, folder_id, uid, message_id, references_header, thread_key, from_addr, from_name, subject, snippet, date, is_seen) values (?1, ?2, 4, '<root@example.com>', null, 'root@example.com', 'ada@example.com', 'Ada', 'Hello', 'root', 100, 0)",
                params![account, all],
            )
            .unwrap();
        }

        let inbox_threads = list_threads(&db, &MessageQuery::default()).unwrap();
        assert_eq!(inbox_threads.len(), 2);
        assert_eq!(inbox_threads[0].message.thread_key, "root@example.com");
        assert_eq!(inbox_threads[0].thread_count, 2);
        assert_eq!(inbox_threads[0].message.snippet, "reply");
        assert_eq!(inbox_threads[0].message.folder_id, sent);
        assert!(inbox_threads[0].thread_has_inbox);
        let inbox_ids =
            thread_message_ids_in_role(&db, account, "root@example.com", "inbox").unwrap();
        assert_eq!(inbox_ids.len(), 1);

        let sent_threads = list_threads(
            &db,
            &MessageQuery {
                unified: Some("sent".into()),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(sent_threads.len(), 1);
        assert_eq!(sent_threads[0].thread_count, 2);

        let thread = list_thread_messages(&db, account, "root@example.com").unwrap();
        assert_eq!(thread.len(), 2);
        assert_eq!(thread[0].snippet, "reply");
        assert_eq!(thread[1].snippet, "root");
        assert_eq!(thread[1].folder_id, inbox);
    }

    fn seed_thread_order_fixture(db: &Database, with_third: bool) -> i64 {
        let account = seed_account(db, "me@example.com");
        let inbox = seed_folder(db, account, "INBOX", "inbox");
        let sent = seed_folder(db, account, "Sent", "sent");
        let conn = db.conn.lock().unwrap();
        // The root thread's newest message lives in Sent at date 200.
        conn.execute(
            "insert into messages (account_id, folder_id, uid, message_id, references_header, thread_key, from_addr, from_name, subject, snippet, date, is_seen) values (?1, ?2, 1, '<root@example.com>', null, 'root@example.com', 'ada@example.com', 'Ada', 'Hello', 'root', 100, 0)",
            params![account, inbox],
        )
        .unwrap();
        conn.execute(
            "insert into messages (account_id, folder_id, uid, message_id, references_header, thread_key, from_addr, from_name, subject, snippet, date, is_seen) values (?1, ?2, 2, '<reply@example.com>', '<root@example.com>', 'root@example.com', 'me@example.com', 'Me', 'Re: Hello', 'reply', 200, 1)",
            params![account, sent],
        )
        .unwrap();
        conn.execute(
            "insert into messages (account_id, folder_id, uid, message_id, references_header, thread_key, from_addr, from_name, subject, snippet, date, is_seen) values (?1, ?2, 3, '<other@example.com>', null, 'other@example.com', 'grace@example.com', 'Grace', 'Other', 'other', 150, 0)",
            params![account, inbox],
        )
        .unwrap();
        if with_third {
            conn.execute(
                "insert into messages (account_id, folder_id, uid, message_id, references_header, thread_key, from_addr, from_name, subject, snippet, date, is_seen) values (?1, ?2, 4, '<third@example.com>', null, 'third@example.com', 'linus@example.com', 'Linus', 'Third', 'third', 175, 0)",
                params![account, inbox],
            )
            .unwrap();
        }
        account
    }

    #[test]
    fn threads_sort_oldest_first_when_requested() {
        let db = Database::open_memory().unwrap();
        seed_thread_order_fixture(&db, false);

        let threads = list_threads(
            &db,
            &MessageQuery {
                order: Some("oldest".into()),
                ..MessageQuery::default()
            },
        )
        .unwrap();

        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].message.thread_key, "other@example.com");
        assert_eq!(threads[1].message.thread_key, "root@example.com");
        // Oldest-first reverses rows, not the representative message.
        assert_eq!(threads[1].message.date, 200);
        assert_eq!(threads[1].message.snippet, "reply");
        assert_eq!(threads[1].thread_count, 2);
    }

    #[test]
    fn thread_pages_are_stable_when_oldest_first() {
        let db = Database::open_memory().unwrap();
        seed_thread_order_fixture(&db, true);

        let page = |offset| MessageQuery {
            order: Some("oldest".into()),
            limit: Some(2),
            offset: Some(offset),
            ..MessageQuery::default()
        };
        let first = list_threads(&db, &page(0)).unwrap();
        let second = list_threads(&db, &page(2)).unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 1);

        let keys: Vec<_> = first
            .iter()
            .chain(second.iter())
            .map(|thread| thread.message.thread_key.clone())
            .collect();
        assert_eq!(keys.len(), 3);
        let unique: std::collections::HashSet<_> = keys.iter().collect();
        assert_eq!(unique.len(), 3);

        let dates: Vec<_> = first
            .iter()
            .chain(second.iter())
            .map(|thread| thread.message.date)
            .collect();
        let mut sorted = dates.clone();
        sorted.sort_unstable();
        assert_eq!(dates, sorted);
    }

    #[test]
    fn unified_inbox_queries_inbox_roles_across_accounts() {
        let db = Database::open_memory().unwrap();
        let first = seed_account(&db, "first@example.com");
        let second = seed_account(&db, "second@example.com");
        let first_inbox = seed_folder(&db, first, "INBOX", "inbox");
        let first_sent = seed_folder(&db, first, "Sent", "sent");
        let second_inbox = seed_folder(&db, second, "INBOX", "inbox");

        seed_message(
            &db,
            first_inbox,
            1,
            "ada@example.com",
            "Hello",
            "first",
            100,
        );
        seed_message(
            &db,
            first_sent,
            2,
            "bob@example.com",
            "Sent only",
            "sent",
            200,
        );
        seed_message(
            &db,
            second_inbox,
            1,
            "grace@example.com",
            "Second",
            "second",
            300,
        );

        let messages = list_messages(&db, &MessageQuery::default()).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].subject, "Second");
        assert_eq!(messages[1].subject, "Hello");

        let sent = list_messages(
            &db,
            &MessageQuery {
                folder_id: Some(first_sent),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].subject, "Sent only");
    }

    #[test]
    fn unified_spam_queries_junk_roles() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let spam = seed_folder(&db, account, "Spam", "junk");
        seed_message(&db, spam, 1, "spammer@example.com", "Offer", "buy now", 100);
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "update messages set message_id = '<spam@example.com>', thread_key = 'spam@example.com' where folder_id = ?1",
                params![spam],
            )
            .unwrap();
        }

        let messages = list_messages(
            &db,
            &MessageQuery {
                unified: Some("spam".into()),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].subject, "Offer");

        let threads = list_threads(
            &db,
            &MessageQuery {
                unified: Some("spam".into()),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(threads.len(), 1);
        assert!(!threads[0].thread_has_inbox);
    }

    #[test]
    fn unified_archive_uses_all_mail_only_without_an_actionable_copy() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        let sent = seed_folder(&db, account, "Sent", "sent");
        let all = seed_folder(&db, account, "[Gmail]/All Mail", "all");
        let archive = seed_folder(&db, account, "Archive", "archive");
        let spam = seed_folder(&db, account, "Spam", "junk");
        let trash = seed_folder(&db, account, "Trash", "trash");

        seed_message(&db, inbox, 1, "ada@example.com", "Inbox copy", "inbox", 500);
        seed_message(&db, all, 1, "ada@example.com", "Inbox copy in all", "inbox", 500);
        seed_message(&db, sent, 2, "me@example.com", "Sent copy", "sent", 450);
        seed_message(&db, all, 2, "me@example.com", "Sent copy in all", "sent", 450);
        seed_message(&db, spam, 3, "spam@example.com", "Spam copy", "spam", 400);
        seed_message(&db, all, 3, "spam@example.com", "Spam copy in all", "spam", 400);
        seed_message(&db, trash, 4, "ada@example.com", "Trash copy", "trash", 350);
        seed_message(&db, all, 4, "ada@example.com", "Trash copy in all", "trash", 350);
        seed_message(&db, all, 5, "news@example.com", "All only", "all", 300);
        seed_message(&db, archive, 1, "old@example.com", "Archived", "archive", 200);

        {
            let conn = db.conn.lock().unwrap();
            let update = |folder: i64, uid: i64, message_id: &str, thread_key: &str| {
                conn.execute(
                    "update messages set message_id = ?2, thread_key = ?3 where folder_id = ?1 and uid = ?4",
                    params![folder, message_id, thread_key, uid],
                )
                .unwrap();
            };
            update(inbox, 1, "<inbox@example.com>", "inbox-thread");
            update(all, 1, "<inbox@example.com>", "inbox-thread");
            update(sent, 2, "<sent@example.com>", "sent-thread");
            update(all, 2, "<sent@example.com>", "sent-thread");
            update(spam, 3, "<spam@example.com>", "spam-thread");
            update(all, 3, "<spam@example.com>", "spam-thread");
            update(trash, 4, "<trash@example.com>", "trash-thread");
            update(all, 4, "<trash@example.com>", "trash-thread");
            update(all, 5, "<all-only@example.com>", "all-thread");
            update(archive, 1, "<archived@example.com>", "archive-thread");
        }

        let threads = list_threads(
            &db,
            &MessageQuery {
                unified: Some("archive".into()),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        let mut keys = threads
            .iter()
            .map(|thread| thread.message.thread_key.as_str())
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(keys, ["all-thread", "archive-thread"]);

        let messages = list_messages(
            &db,
            &MessageQuery {
                unified: Some("archive".into()),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(messages.len(), 2);

        let counts = tab_unread_counts(
            &db,
            &MessageQuery {
                unified: Some("archive".into()),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(counts.all, 2);
    }

    #[test]
    fn tab_filters_include_unclassified_mail_only_in_all() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        seed_message(&db, inbox, 1, "ada@example.com", "Update", "receipt", 300);
        seed_message(
            &db,
            inbox,
            2,
            "bob@example.com",
            "Unclassified",
            "hello",
            200,
        );
        seed_message(
            &db,
            inbox,
            3,
            "carol@example.com",
            "Never classified",
            "hi",
            100,
        );
        let message_ids: Vec<i64> = {
            let conn = db.conn.lock().unwrap();
            let mut stmt = conn.prepare("select id from messages order by id").unwrap();
            let rows = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            rows
        };
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "insert into classifications (message_id, tab_id, confidence, model, classified_at) values (?1, 2, 0.9, 'test', 0)",
                params![message_ids[0]],
            )
            .unwrap();
            conn.execute(
                "insert into classifications (message_id, tab_id, confidence, model, classified_at) values (?1, null, 0.5, 'test', 0)",
                params![message_ids[1]],
            )
            .unwrap();
        }

        let all = list_messages(&db, &MessageQuery::default()).unwrap();
        assert_eq!(all.len(), 3);

        let primary = list_messages(
            &db,
            &MessageQuery {
                tab_id: Some(1),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert!(primary.is_empty());

        let updates = list_messages(
            &db,
            &MessageQuery {
                tab_id: Some(2),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].subject, "Update");

        let promotions = list_messages(
            &db,
            &MessageQuery {
                tab_id: Some(3),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert!(promotions.is_empty());
    }

    #[test]
    fn unclassified_ids_include_missing_and_null_classifications() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        seed_message(&db, inbox, 1, "ada@example.com", "Assigned", "", 300);
        seed_message(&db, inbox, 2, "bob@example.com", "Null", "", 200);
        seed_message(&db, inbox, 3, "carol@example.com", "Missing", "", 100);
        let ids: Vec<i64> = {
            let conn = db.conn.lock().unwrap();
            let mut stmt = conn.prepare("select id from messages order by id").unwrap();
            let rows = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            rows
        };
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "insert into classifications (message_id, tab_id, confidence, model, classified_at) values (?1, 1, 0.9, 'test', 0)",
                params![ids[0]],
            )
            .unwrap();
            conn.execute(
                "insert into classifications (message_id, tab_id, confidence, model, classified_at) values (?1, null, 0.5, 'test', 0)",
                params![ids[1]],
            )
            .unwrap();
        }

        let pending = unclassified_message_ids(&db, Some(account), 10).unwrap();
        assert_eq!(pending, [ids[1], ids[2]]);
        assert!(unclassified_message_ids(&db, Some(account + 1), 10)
            .unwrap()
            .is_empty());

        let first_page =
            unclassified_message_ids_before(&db, Some(account), None, None, 2).unwrap();
        assert_eq!(first_page, [ids[2], ids[1]]);
        let second_page =
            unclassified_message_ids_before(&db, Some(account), None, Some(ids[1]), 2).unwrap();
        assert!(second_page.is_empty());
        assert_eq!(
            unclassified_message_ids_before(&db, None, Some(inbox), None, 10).unwrap(),
            [ids[2], ids[1]]
        );
    }

    #[test]
    fn unclassified_mail_stays_out_of_primary_after_renaming() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        seed_message(&db, inbox, 1, "ada@example.com", "Unclassified", "", 100);
        crate::tabs::update(
            &db,
            crate::spaces::default_id(&db).unwrap(),
            1,
            "Personal",
            "Personal and fallback mail.",
            None,
            None,
        )
        .unwrap();

        let primary = list_messages(
            &db,
            &MessageQuery {
                tab_id: Some(1),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert!(primary.is_empty());
    }

    #[test]
    fn tab_unread_counts_follow_scope_and_count_threads() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        let sent = seed_folder(&db, account, "Sent", "sent");
        seed_message(&db, inbox, 1, "ada@example.com", "Thread one", "a", 300);
        seed_message(&db, inbox, 2, "ada@example.com", "Thread one", "b", 310);
        seed_message(&db, inbox, 3, "bob@example.com", "Thread two", "c", 200);
        seed_message(&db, inbox, 4, "carol@example.com", "Read", "d", 100);
        seed_message(&db, sent, 5, "me@example.com", "Sent unread", "e", 400);
        {
            let conn = db.conn.lock().unwrap();
            conn.execute_batch(
                "update messages set thread_key = 'thread-one', message_id = '<one@example.com>' where folder_id = 1 and uid in (1, 2);
                 update messages set thread_key = 'thread-two', message_id = '<two@example.com>' where folder_id = 1 and uid = 3;
                 update messages set thread_key = 'read-thread', message_id = '<read@example.com>', is_seen = 1 where folder_id = 1 and uid = 4;
                 update messages set thread_key = 'sent-thread', message_id = '<sent@example.com>' where folder_id = 2 and uid = 5;
                 insert into classifications (message_id, tab_id, confidence, model, classified_at)
                   select id, 2, 1.0, 'test', 0 from messages where folder_id = 1 and uid in (1, 2);
                 insert into classifications (message_id, tab_id, confidence, model, classified_at)
                   select id, 2, 1.0, 'test', 0 from messages where folder_id = 2 and uid = 5;",
            )
            .unwrap();
        }

        let counts = tab_unread_counts(&db, &MessageQuery::default()).unwrap();
        assert_eq!(counts.all, 2);
        let updates = counts
            .tabs
            .iter()
            .find(|tab| tab.tab_id == 2)
            .map(|tab| tab.unread);
        let primary = counts
            .tabs
            .iter()
            .find(|tab| tab.tab_id == 1)
            .map(|tab| tab.unread);
        assert_eq!(updates, Some(1));
        assert_eq!(primary, None);

        let sent_counts = tab_unread_counts(
            &db,
            &MessageQuery {
                folder_id: Some(sent),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(sent_counts.all, 1);
        assert_eq!(
            sent_counts
                .tabs
                .iter()
                .find(|tab| tab.tab_id == 2)
                .map(|tab| tab.unread),
            Some(1)
        );
    }

    #[test]
    fn search_is_case_insensitive_across_sender_subject_and_snippet() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        seed_message(&db, inbox, 1, "Ada@example.com", "Hello", "plain text", 100);
        seed_message(
            &db,
            inbox,
            2,
            "bob@example.com",
            "Invoice",
            "payment due",
            200,
        );
        seed_message(
            &db,
            inbox,
            3,
            "carol@example.com",
            "Note",
            "weekly digest",
            300,
        );

        let by_sender = list_messages(
            &db,
            &MessageQuery {
                search: Some("ADA".into()),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(by_sender.len(), 1);
        assert_eq!(by_sender[0].subject, "Hello");

        let by_subject = list_messages(
            &db,
            &MessageQuery {
                search: Some("invoice".into()),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(by_subject.len(), 1);
        assert_eq!(by_subject[0].subject, "Invoice");

        let by_snippet = list_messages(
            &db,
            &MessageQuery {
                search: Some("DIGEST".into()),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(by_snippet.len(), 1);
        assert_eq!(by_snippet[0].subject, "Note");
    }

    #[test]
    fn search_is_universal_across_folders_recipients_and_cached_bodies() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        let sent = seed_folder(&db, account, "Sent", "sent");
        let archive = seed_folder(&db, account, "Archive", "archive");
        seed_message(&db, inbox, 1, "Ada@example.com", "Hello", "plain", 100);
        seed_message(&db, sent, 2, "me@example.com", "Sent note", "sent", 200);
        seed_message(&db, archive, 3, "carol@example.com", "Archived", "old", 300);
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "update messages set message_id = '<inbox@example.com>', thread_key = 'inbox-thread', to_addrs = '[\"me@example.com\"]' where folder_id = ?1 and uid = 1",
                params![inbox],
            )
            .unwrap();
            conn.execute(
                "update messages set message_id = '<sent@example.com>', thread_key = 'sent-thread', to_addrs = '[\"bob@example.com\"]' where folder_id = ?1 and uid = 2",
                params![sent],
            )
            .unwrap();
            conn.execute(
                "update messages set message_id = '<archive@example.com>', thread_key = 'archive-thread' where folder_id = ?1 and uid = 3",
                params![archive],
            )
            .unwrap();
            let archived_id: i64 = conn
                .query_row(
                    "select id from messages where folder_id = ?1 and uid = 3",
                    params![archive],
                    |row| row.get(0),
                )
                .unwrap();
            conn.execute(
                "insert into message_bodies (message_id, text_body, html_body, fetched_at) values (?1, 'needle in the body', null, 0)",
                params![archived_id],
            )
            .unwrap();
            conn.execute(
                "update messages set body_cached = 1 where id = ?1",
                params![archived_id],
            )
            .unwrap();
        }

        let search = |term: &str| {
            list_threads(
                &db,
                &MessageQuery {
                    search: Some(term.into()),
                    ..MessageQuery::default()
                },
            )
            .unwrap()
        };

        assert_eq!(search("ADA")[0].message.subject, "Hello");
        assert_eq!(search("bob@example.com")[0].message.subject, "Sent note");
        assert_eq!(search("ARCHIVED")[0].message.subject, "Archived");
        assert_eq!(search("needle in the body")[0].message.subject, "Archived");
    }

    #[test]
    fn folder_reconciliation_removes_missing_uids_and_updates_flags() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        seed_message(&db, inbox, 1, "ada@example.com", "One", "a", 300);
        seed_message(&db, inbox, 2, "bob@example.com", "Two", "b", 200);
        seed_message(&db, inbox, 3, "carol@example.com", "Three", "c", 100);

        let changed =
            reconcile_folder_messages(&db, inbox, &[(1, true, false), (2, false, true)]).unwrap();

        assert_eq!(changed, 3);
        let messages = list_messages(
            &db,
            &MessageQuery {
                folder_id: Some(inbox),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].subject, "One");
        assert!(messages[0].is_seen);
        assert!(!messages[0].is_starred);
        assert_eq!(messages[1].subject, "Two");
        assert!(!messages[1].is_seen);
        assert!(messages[1].is_starred);
    }

    #[test]
    fn repeated_upserts_update_metadata_flags_and_keep_cached_body() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        let first = MessageHeader {
            uid: 7,
            headers: ParsedHeaders {
                subject: "First".into(),
                snippet: "before".into(),
                ..ParsedHeaders::default()
            },
            is_seen: false,
            is_starred: false,
        };
        assert_eq!(
            apply_folder_headers(&db, account, inbox, &[first], 0, 7)
                .unwrap()
                .len(),
            1
        );
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "update messages set body_cached = 1 where folder_id = ?1 and uid = 7",
                params![inbox],
            )
            .unwrap();
        }

        let second = MessageHeader {
            uid: 7,
            headers: ParsedHeaders {
                subject: "Second".into(),
                snippet: "after".into(),
                ..ParsedHeaders::default()
            },
            is_seen: true,
            is_starred: true,
        };
        assert!(apply_folder_headers(&db, account, inbox, &[second], 0, 7)
            .unwrap()
            .is_empty());
        let messages = list_messages(
            &db,
            &MessageQuery {
                folder_id: Some(inbox),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].subject, "Second");
        assert_eq!(messages[0].snippet, "after");
        assert!(messages[0].is_seen);
        assert!(messages[0].is_starred);
        assert!(messages[0].body_cached);
    }

    #[test]
    fn replacement_failure_keeps_old_messages_and_cursor() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        seed_message(&db, inbox, 1, "ada@example.com", "Old", "old", 100);

        let replacement = vec![MessageHeader {
            uid: 2,
            headers: ParsedHeaders {
                subject: "New".into(),
                ..ParsedHeaders::default()
            },
            is_seen: false,
            is_starred: false,
        }];
        let result = replace_folder_messages(&db, 9_999, inbox, &replacement, 5, 2);
        assert!(result.is_err());

        let messages = list_messages(
            &db,
            &MessageQuery {
                folder_id: Some(inbox),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].subject, "Old");
        assert_eq!(folder_cursor(&db, inbox).unwrap(), (0, 0));
    }

    #[test]
    fn replacement_swaps_rows_and_updates_cursor_after_success() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        seed_message(&db, inbox, 1, "ada@example.com", "Old", "old", 100);

        let replacement = vec![MessageHeader {
            uid: 20,
            headers: ParsedHeaders {
                subject: "New".into(),
                snippet: "fresh".into(),
                ..ParsedHeaders::default()
            },
            is_seen: true,
            is_starred: false,
        }];
        replace_folder_messages(&db, account, inbox, &replacement, 5, 20).unwrap();

        let messages = list_messages(
            &db,
            &MessageQuery {
                folder_id: Some(inbox),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].subject, "New");
        assert!(messages[0].is_seen);
        assert_eq!(folder_cursor(&db, inbox).unwrap(), (5, 20));
    }

    #[test]
    fn search_treats_like_wildcards_literally() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        seed_message(
            &db,
            inbox,
            1,
            "ada@example.com",
            "Sale",
            "50% off today",
            100,
        );
        seed_message(
            &db,
            inbox,
            2,
            "bob@example.com",
            "Plain",
            "no special characters",
            200,
        );

        let matches = list_messages(
            &db,
            &MessageQuery {
                search: Some("%".into()),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].subject, "Sale");
    }

    #[test]
    fn cached_body_replaces_attachments_and_marks_message() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        seed_message(&db, inbox, 5, "ada@example.com", "Files", "hello", 100);
        let conn = db.conn.lock().unwrap();
        let message_id: i64 = conn
            .query_row(
                "select id from messages where folder_id = ?1",
                params![inbox],
                |row| row.get(0),
            )
            .unwrap();
        drop(conn);

        let parsed = parse::ParsedMessage {
            text_body: "hello body".into(),
            html_body: Some("<p>hello body</p>".into()),
            attachments: vec![parse::ParsedAttachment {
                filename: "report.pdf".into(),
                mime_type: "application/pdf".into(),
                size: 5,
                content_id: Some("cid-1".into()),
                data: b"hello".to_vec(),
            }],
            ..parse::ParsedMessage::default()
        };
        let detail = cache_message_body(&db, message_id, &parsed).unwrap();
        assert!(detail.message.body_cached);
        assert_eq!(detail.text_body.as_deref(), Some("hello body"));
        assert_eq!(detail.html_body.as_deref(), Some("<p>hello body</p>"));
        assert_eq!(detail.attachments.len(), 1);
        assert_eq!(detail.attachments[0].filename, "report.pdf");
        assert_eq!(detail.attachments[0].content_id.as_deref(), Some("cid-1"));

        let refreshed = parse::ParsedMessage {
            text_body: "new".into(),
            html_body: None,
            attachments: Vec::new(),
            ..parse::ParsedMessage::default()
        };
        let detail = cache_message_body(&db, message_id, &refreshed).unwrap();
        assert_eq!(detail.text_body.as_deref(), Some("new"));
        assert!(detail.attachments.is_empty());
        let rows: i64 = db
            .conn
            .lock()
            .unwrap()
            .query_row("select count(*) from attachments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn seen_and_starred_updates_return_the_row() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        seed_message(&db, inbox, 1, "ada@example.com", "Hello", "body", 100);
        let id = {
            let conn = db.conn.lock().unwrap();
            conn.query_row(
                "select id from messages where folder_id = ?1",
                params![inbox],
                |row| row.get(0),
            )
            .unwrap()
        };

        assert!(set_message_seen(&db, id, true).unwrap().is_seen);
        assert!(set_message_starred(&db, id, true).unwrap().is_starred);
        let detail = get_message_detail(&db, id).unwrap();
        assert!(detail.message.is_seen);
        assert!(detail.message.is_starred);
    }

    #[test]
    fn safe_attachment_names_strip_paths_and_controls() {
        assert_eq!(safe_attachment_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_attachment_name("C:\\temp\\a.txt"), "a.txt");
        assert_eq!(safe_attachment_name("bad\nname\u{0}.txt"), "badname.txt");
        assert_eq!(safe_attachment_name(""), "attachment");
        assert_eq!(safe_attachment_name("..."), "attachment");
    }

    #[test]
    fn trash_folder_is_found_by_role() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        seed_folder(&db, account, "INBOX", "inbox");
        seed_folder(&db, account, "Trash", "trash");
        assert_eq!(
            trash_folder_path(&db, account).unwrap().as_deref(),
            Some("Trash")
        );
        assert_eq!(trash_folder_path(&db, account + 1).unwrap(), None);
    }

    #[test]
    fn header_writes_return_only_new_message_ids() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        let header = MessageHeader {
            uid: 8,
            headers: ParsedHeaders {
                subject: "New".into(),
                ..ParsedHeaders::default()
            },
            is_seen: false,
            is_starred: false,
        };
        let inserted = apply_folder_headers(&db, account, inbox, &[header], 0, 8).unwrap();
        assert_eq!(inserted.len(), 1);
        let updated = MessageHeader {
            uid: 8,
            headers: ParsedHeaders {
                subject: "Updated".into(),
                ..ParsedHeaders::default()
            },
            is_seen: true,
            is_starred: false,
        };
        assert!(apply_folder_headers(&db, account, inbox, &[updated], 0, 8)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn folder_reconciliation_removes_remote_deletions_and_cascades_messages() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        let removed = seed_folder(&db, account, "Old", "other");
        seed_message(&db, removed, 1, "ada@example.com", "Old", "gone", 100);

        reconcile_folders(&db, account, &["INBOX".to_string()]).unwrap();

        let folders = list_folders(&db, Some(account), None).unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].id, inbox);
        let messages: i64 = db
            .conn
            .lock()
            .unwrap()
            .query_row("select count(*) from messages", [], |row| row.get(0))
            .unwrap();
        assert_eq!(messages, 0);
    }

    #[test]
    fn messages_round_trip_references() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        let header = MessageHeader {
            uid: 9,
            headers: ParsedHeaders {
                subject: "Reply".into(),
                references: Some("<1@example.com> <2@example.com>".into()),
                ..ParsedHeaders::default()
            },
            is_seen: false,
            is_starred: false,
        };
        apply_folder_headers(&db, account, inbox, &[header], 0, 9).unwrap();
        let message = list_messages(
            &db,
            &MessageQuery {
                folder_id: Some(inbox),
                ..MessageQuery::default()
            },
        )
        .unwrap()
        .remove(0);
        assert_eq!(
            message.references.as_deref(),
            Some("<1@example.com> <2@example.com>")
        );
    }

    #[test]
    fn messages_default_to_one_hundred_and_apply_offset() {
        let db = Database::open_memory().unwrap();
        let account = seed_account(&db, "me@example.com");
        let inbox = seed_folder(&db, account, "INBOX", "inbox");
        for uid in 1..=120 {
            seed_message(
                &db,
                inbox,
                uid,
                "ada@example.com",
                &format!("Message {uid}"),
                "snippet",
                uid,
            );
        }

        let first = list_messages(&db, &MessageQuery::default()).unwrap();
        assert_eq!(first.len(), 100);
        assert_eq!(first[0].subject, "Message 120");

        let second = list_messages(
            &db,
            &MessageQuery {
                offset: Some(100),
                ..MessageQuery::default()
            },
        )
        .unwrap();
        assert_eq!(second.len(), 20);
        assert_eq!(second[0].subject, "Message 20");
    }
}
