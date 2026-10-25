use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::ai::{configured_embedding_client, AiConfig};
use crate::db::{now, Database};
use crate::error::{AppError, AppResult};
use crate::mail::{self, Message};
use crate::secrets::SecretStore;

pub const INDEX_LIMIT: usize = 5000;
const BATCH_SIZE: usize = 24;
const RELATED_LIMIT: usize = 4;
const MIN_SIMILARITY: f32 = 0.2;

static INDEXING: AtomicBool = AtomicBool::new(false);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticStatus {
    pub configured: bool,
    pub model: String,
    pub indexed: u64,
    pub pending: u64,
    pub indexing: bool,
    pub error: Option<String>,
}

fn embedding_text(message: &Message) -> String {
    let subject = message.subject.trim();
    let snippet = message.snippet.trim();
    let text = match (subject.is_empty(), snippet.is_empty()) {
        (true, true) => String::new(),
        (true, false) => snippet.to_string(),
        (false, true) => subject.to_string(),
        (false, false) => format!("{subject}\n{snippet}"),
    };
    text.chars().take(2000).collect()
}

fn content_hash(text: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn vector_bytes(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn vector_from_bytes(bytes: &[u8], dim: usize) -> Option<Vec<f32>> {
    if bytes.len() != dim.saturating_mul(4) {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (left, right) in a.iter().zip(b) {
        dot += left * right;
        norm_a += left * left;
        norm_b += right * right;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return None;
    }
    Some(dot / (norm_a.sqrt() * norm_b.sqrt()))
}

fn candidate_ids(
    db: &Database,
    space_id: i64,
    model: &str,
    limit: usize,
) -> AppResult<Vec<i64>> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "select m.id from messages m
         join folders f on f.id = m.folder_id
         left join message_embeddings e on e.message_id = m.id and e.model = ?1
         where f.role not in ('trash', 'junk', 'drafts') and e.message_id is null
           and m.account_id in (select id from accounts where space_id = ?2)
           and (trim(m.subject) <> '' or trim(m.snippet) <> '')
         order by m.date desc, m.id desc
         limit ?3",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![model, space_id, limit as i64],
        |row| row.get(0),
    )?;
    Ok(rows.collect::<Result<Vec<i64>, _>>()?)
}

fn indexed_count(db: &Database, space_id: i64, model: &str) -> AppResult<u64> {
    let conn = db.conn.lock().unwrap();
    let count: i64 = conn.query_row(
        "select count(*) from message_embeddings e
         join messages m on m.id = e.message_id
         where e.model = ?1 and m.account_id in (select id from accounts where space_id = ?2)",
        rusqlite::params![model, space_id],
        |row| row.get(0),
    )?;
    Ok(count as u64)
}

fn pending_count(db: &Database, space_id: i64, model: &str) -> AppResult<u64> {
    let conn = db.conn.lock().unwrap();
    let count: i64 = conn.query_row(
        "select count(*) from messages m
         join folders f on f.id = m.folder_id
         left join message_embeddings e on e.message_id = m.id and e.model = ?1
         where f.role not in ('trash', 'junk', 'drafts') and e.message_id is null
           and m.account_id in (select id from accounts where space_id = ?2)
           and (trim(m.subject) <> '' or trim(m.snippet) <> '')",
        rusqlite::params![model, space_id],
        |row| row.get(0),
    )?;
    Ok(count as u64)
}

fn store_embeddings(
    db: &Database,
    model: &str,
    rows: &[(i64, String, Vec<f32>)],
) -> AppResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    let stamp = now();
    for (message_id, hash, vector) in rows {
        let bytes = vector_bytes(vector);
        tx.execute(
            "insert into message_embeddings
             (message_id, model, dim, vector, content_hash, created_at)
             values (?1, ?2, ?3, ?4, ?5, ?6)
             on conflict(message_id) do update set
                model = excluded.model,
                dim = excluded.dim,
                vector = excluded.vector,
                content_hash = excluded.content_hash,
                created_at = excluded.created_at",
            rusqlite::params![
                message_id,
                model,
                vector.len() as i64,
                bytes,
                hash,
                stamp
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn status(db: &Database, space_id: i64) -> AppResult<SemanticStatus> {
    let config = AiConfig::load(db, space_id)?;
    let model = config.embedding_model.trim().to_string();
    let configured = !model.is_empty()
        && !config.embedding_base_url.trim().is_empty();
    let indexed = if configured {
        indexed_count(db, space_id, &model)?
    } else {
        0
    };
    let pending = if configured {
        pending_count(db, space_id, &model)?
    } else {
        0
    };
    Ok(SemanticStatus {
        configured,
        model,
        indexed,
        pending,
        indexing: INDEXING.load(Ordering::SeqCst),
        error: if configured {
            LAST_ERROR.lock().unwrap().clone()
        } else {
            None
        },
    })
}

async fn run_index_inner(
    db: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    space_id: i64,
) -> AppResult<u32> {
    let (model, client) = configured_embedding_client(&db, secrets.as_ref(), space_id)?;
    if model.trim().is_empty() {
        return Ok(0);
    }

    db.conn
        .lock()
        .unwrap()
        .execute(
            "delete from message_embeddings
             where model <> ?1
               and message_id in (
                   select m.id from messages m
                   where m.account_id in (select id from accounts where space_id = ?2)
               )",
            rusqlite::params![model, space_id],
        )?;

    let mut total = 0u32;
    while total < INDEX_LIMIT as u32 {
        let ids = candidate_ids(&db, space_id, &model, BATCH_SIZE)?;
        if ids.is_empty() {
            break;
        }
        let mut messages = Vec::new();
        for id in ids {
            let Ok(message) = mail::message_by_id(&db, id) else {
                continue;
            };
            let text = embedding_text(&message);
            if !text.trim().is_empty() {
                messages.push((id, text));
            }
        }
        if messages.is_empty() {
            break;
        }
        let inputs: Vec<String> = messages.iter().map(|(_, text)| text.clone()).collect();
        let vectors = client.embed(&model, &inputs).await?;
        if vectors.len() != messages.len() {
            return Err(AppError::Message(
                "Embedding response count did not match".into(),
            ));
        }
        let rows: Vec<(i64, String, Vec<f32>)> = messages
            .into_iter()
            .zip(vectors)
            .map(|((id, text), vector)| (id, content_hash(&text), vector))
            .collect();
        total += rows.len() as u32;
        store_embeddings(&db, &model, &rows)?;
    }
    Ok(total)
}

pub fn schedule_index(
    db: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    space_id: i64,
) {
    let Ok(config) = AiConfig::load(&db, space_id) else {
        return;
    };
    if config.embedding_base_url.trim().is_empty()
        || config.embedding_model.trim().is_empty()
    {
        return;
    }
    if INDEXING.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let result = run_index_inner(db, secrets, space_id).await;
        INDEXING.store(false, Ordering::SeqCst);
        match &result {
            Ok(_) => *LAST_ERROR.lock().unwrap() = None,
            Err(error) => {
                *LAST_ERROR.lock().unwrap() = Some(error.to_string());
                eprintln!("Aura semantic indexing failed: {error}");
            }
        }
    });
}

pub fn related(
    db: &Database,
    space_id: i64,
    model: &str,
    query: &[f32],
    exclude_thread: Option<&str>,
    limit: usize,
) -> AppResult<Vec<Message>> {
    if query.is_empty() || model.trim().is_empty() {
        return Ok(Vec::new());
    }
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "select e.message_id, e.vector, e.dim, m.thread_key
         from message_embeddings e
         join messages m on m.id = e.message_id
         join folders f on f.id = m.folder_id
         where e.model = ?1 and f.role not in ('trash', 'junk', 'drafts')
           and m.account_id in (select id from accounts where space_id = ?2)",
    )?;
    let rows = stmt.query_map(rusqlite::params![model, space_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Vec<u8>>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut hits = Vec::new();
    for row in rows {
        let (message_id, bytes, dim, thread_key) = row?;
        if exclude_thread == Some(thread_key.as_str()) {
            continue;
        }
        let Some(vector) = vector_from_bytes(&bytes, dim.max(0) as usize) else {
            continue;
        };
        let Some(score) = cosine(query, &vector) else {
            continue;
        };
        if score >= MIN_SIMILARITY {
            hits.push((score, message_id));
        }
    }
    drop(stmt);
    drop(conn);
    hits.sort_by(|left, right| right.0.total_cmp(&left.0));
    let mut messages = Vec::new();
    for (_, message_id) in hits.into_iter().take(limit.clamp(1, RELATED_LIMIT)) {
        if let Ok(message) = mail::message_by_id(db, message_id) {
            messages.push(message);
        }
    }
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn space(db: &Database) -> i64 {
        crate::spaces::default_id(db).unwrap()
    }

    fn seed(db: &Database) -> (i64, i64) {
        let space_id = space(db);
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "insert into accounts (id, display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, sync_interval_secs, space_id, created_at) values (1, 'Me', 'me@example.com', 'me', 'imap', 993, 'tls', 'smtp', 465, 'tls', 60, ?1, 0)",
            rusqlite::params![space_id],
        )
        .unwrap();
        conn.execute_batch(
            "
             insert into folders (id, account_id, path, name, role) values (1, 1, 'INBOX', 'Inbox', 'inbox');
             insert into messages (account_id, folder_id, uid, thread_key, subject, snippet, date, is_seen) values (1, 1, 1, 'first', 'Invoice', 'March invoice', 100, 1);
             insert into messages (account_id, folder_id, uid, thread_key, subject, snippet, date, is_seen) values (1, 1, 2, 'second', 'Lunch', 'Lunch on Friday', 200, 1);",
        )
        .unwrap();
        let first = conn.last_insert_rowid() - 1;
        let second = conn.last_insert_rowid();
        (first, second)
    }

    #[test]
    fn hashes_and_vector_round_trip() {
        assert_eq!(content_hash("hello"), content_hash("hello"));
        assert_ne!(content_hash("hello"), content_hash("world"));
        let vector = vec![0.1f32, -0.2, 0.3];
        let bytes = vector_bytes(&vector);
        assert_eq!(vector_from_bytes(&bytes, 3), Some(vector));
        assert_eq!(vector_from_bytes(&bytes, 2), None);
    }

    #[test]
    fn reports_indexed_and_pending_counts() {
        let db = Database::open_memory().unwrap();
        let (first, _) = seed(&db);
        crate::ai::AiConfig {
            base_url: "https://api.example.com/v1".into(),
            model: "chat-model".into(),
            embedding_base_url: "https://embed.example.com/v1".into(),
            embedding_model: "embed".into(),
            extra_headers: Vec::new(),
        }
        .save(&db, space(&db))
        .unwrap();

        let before = status(&db, space(&db)).unwrap();
        assert!(before.configured);
        assert_eq!(before.indexed, 0);
        assert_eq!(before.pending, 2);

        store_embeddings(
            &db,
            "embed",
            &[(first, content_hash("invoice"), vec![1.0, 0.0])],
        )
        .unwrap();
        let after = status(&db, space(&db)).unwrap();
        assert_eq!(after.indexed, 1);
        assert_eq!(after.pending, 1);
    }

    #[test]
    fn finds_semantically_closest_messages() {
        let db = Database::open_memory().unwrap();
        let (first, second) = seed(&db);
        store_embeddings(
            &db,
            "embed",
            &[
                (first, content_hash("invoice"), vec![1.0, 0.0]),
                (second, content_hash("lunch"), vec![0.0, 1.0]),
            ],
        )
        .unwrap();

        let found = related(&db, space(&db), "embed", &[0.9, 0.1], Some("none"), 4).unwrap();
        assert_eq!(found[0].id, first);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn related_messages_are_isolated_by_space() {
        let db = Database::open_memory().unwrap();
        let (first, _) = seed(&db);
        let work = crate::spaces::create(&db, "Work").unwrap();
        let work_message = {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "insert into accounts (id, display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, sync_interval_secs, space_id, created_at) values (2, 'Work', 'work@example.com', 'work', 'imap', 993, 'tls', 'smtp', 465, 'tls', 60, ?1, 0)",
                rusqlite::params![work.id],
            )
            .unwrap();
            conn.execute(
                "insert into folders (id, account_id, path, name, role) values (2, 2, 'INBOX', 'Inbox', 'inbox')",
                [],
            )
            .unwrap();
            conn.execute(
                "insert into messages (account_id, folder_id, uid, thread_key, subject, snippet, date, is_seen) values (2, 2, 1, 'work', 'Work invoice', 'Work invoice', 300, 1)",
                [],
            )
            .unwrap();
            conn.last_insert_rowid()
        };
        store_embeddings(
            &db,
            "embed",
            &[
                (first, content_hash("invoice"), vec![1.0, 0.0]),
                (work_message, content_hash("work"), vec![1.0, 0.0]),
            ],
        )
        .unwrap();

        let personal = related(&db, space(&db), "embed", &[1.0, 0.0], None, 4).unwrap();
        assert_eq!(personal.iter().map(|m| m.id).collect::<Vec<_>>(), vec![first]);
        let work_hits = related(&db, work.id, "embed", &[1.0, 0.0], None, 4).unwrap();
        assert_eq!(
            work_hits.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![work_message]
        );
    }

    #[test]
    fn excludes_the_current_thread() {
        let db = Database::open_memory().unwrap();
        let (first, _) = seed(&db);
        store_embeddings(
            &db,
            "embed",
            &[(first, content_hash("invoice"), vec![1.0, 0.0])],
        )
        .unwrap();

        assert!(related(&db, space(&db), "embed", &[1.0, 0.0], Some("first"), 4)
            .unwrap()
            .is_empty());
    }
}
