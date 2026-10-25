use rusqlite::{params, OptionalExtension};

use crate::db::{now, Database};
use crate::error::{AppError, AppResult};

use super::AiAuraRail;

const CACHE_TTL_SECS: i64 = 30 * 24 * 60 * 60;

pub fn load(
    db: &Database,
    message_id: i64,
    model: &str,
    full_body: bool,
    thread_latest_id: i64,
    thread_latest_date: i64,
) -> AppResult<Option<AiAuraRail>> {
    let conn = db.conn.lock().unwrap();
    let row = conn
        .query_row(
            "select model, full_body, thread_latest_id, thread_latest_date, payload, created_at
             from ai_rail_cache where message_id = ?1",
            [message_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((cached_model, cached_full_body, cached_latest_id, cached_latest_date, payload, created_at)) =
        row
    else {
        return Ok(None);
    };
    if created_at < now() - CACHE_TTL_SECS {
        conn.execute("delete from ai_rail_cache where message_id = ?1", [message_id])?;
        return Ok(None);
    }
    if cached_model != model
        || cached_full_body != (if full_body { 1 } else { 0 })
        || cached_latest_id != thread_latest_id
        || cached_latest_date != thread_latest_date
    {
        return Ok(None);
    }
    if let Ok(rail) = serde_json::from_str::<AiAuraRail>(&payload) {
        if rail.question.is_some() || rail.drafts.len() >= 3 {
            return Ok(Some(rail));
        }
    }
    conn.execute("delete from ai_rail_cache where message_id = ?1", [message_id])?;
    Ok(None)
}

#[allow(dead_code)]
pub fn clear_all(db: &Database) -> AppResult<()> {
    db.conn
        .lock()
        .unwrap()
        .execute("delete from ai_rail_cache", [])?;
    Ok(())
}

pub fn clear_space(db: &Database, space_id: i64) -> AppResult<()> {
    db.conn.lock().unwrap().execute(
        "delete from ai_rail_cache where message_id in (
            select m.id from messages m
            join accounts a on a.id = m.account_id
            where a.space_id = ?1
         )",
        params![space_id],
    )?;
    Ok(())
}

pub fn store(
    db: &Database,
    message_id: i64,
    model: &str,
    full_body: bool,
    thread_latest_id: i64,
    thread_latest_date: i64,
    rail: &AiAuraRail,
) -> AppResult<()> {
    let payload = serde_json::to_string(rail)
        .map_err(|_| AppError::Message("could not cache the thread brief".into()))?;
    let timestamp = now();
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "insert into ai_rail_cache
         (message_id, model, full_body, thread_latest_id, thread_latest_date, payload, created_at)
         values (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         on conflict(message_id) do update set
            model = excluded.model,
            full_body = excluded.full_body,
            thread_latest_id = excluded.thread_latest_id,
            thread_latest_date = excluded.thread_latest_date,
            payload = excluded.payload,
            created_at = excluded.created_at",
        params![
            message_id,
            model,
            if full_body { 1i64 } else { 0i64 },
            thread_latest_id,
            thread_latest_date,
            payload,
            timestamp
        ],
    )?;
    conn.execute(
        "delete from ai_rail_cache where created_at < ?1",
        [timestamp - CACHE_TTL_SECS],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::AiAuraDraft;

    fn rail() -> AiAuraRail {
        AiAuraRail {
            summary: "Summary".into(),
            ask: "Confirm the date.".into(),
            deadline: Some("Thursday".into()),
            tone: Some("Direct".into()),
            question: None,
            options: Vec::new(),
            drafts: vec![
                AiAuraDraft {
                    tone: "direct".into(),
                    text: "Direct reply".into(),
                },
                AiAuraDraft {
                    tone: "warm".into(),
                    text: "Warm reply".into(),
                },
                AiAuraDraft {
                    tone: "concise".into(),
                    text: "Concise reply".into(),
                },
            ],
            message_count: 2,
        }
    }

    fn seed_message(db: &Database) -> i64 {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "insert into accounts
             (display_name, email, username, imap_host, imap_port, imap_security,
              smtp_host, smtp_port, smtp_security, sync_interval_secs, created_at)
             values ('Me', 'me@example.com', 'me@example.com', 'imap.example.com', 993, 'tls',
                     'smtp.example.com', 465, 'tls', 60, 0)",
            [],
        )
        .unwrap();
        let account_id = conn.last_insert_rowid();
        conn.execute(
            "insert into folders (account_id, path, name, role, delimiter) values (?1, 'INBOX', 'Inbox', 'inbox', '/')",
            [account_id],
        )
        .unwrap();
        let folder_id = conn.last_insert_rowid();
        conn.execute(
            "insert into messages (account_id, folder_id, uid, subject, date, snippet)
             values (?1, ?2, 1, 'Hello', 100, 'Snippet')",
            params![account_id, folder_id],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn round_trips_a_rail_for_the_same_model_and_thread() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        store(&db, message_id, "model-a", false, 5, 100, &rail()).unwrap();

        assert_eq!(
            load(&db, message_id, "model-a", false, 5, 100).unwrap(),
            Some(rail())
        );
    }

    #[test]
    fn round_trips_a_question_only_rail() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        let rail = AiAuraRail {
            question: Some("Which package?".into()),
            options: vec!["Full package".into(), "Base shoot".into()],
            drafts: Vec::new(),
            ..rail()
        };
        store(&db, message_id, "model-a", false, 5, 100, &rail).unwrap();

        assert_eq!(
            load(&db, message_id, "model-a", false, 5, 100).unwrap(),
            Some(rail)
        );
    }

    #[test]
    fn misses_when_the_model_or_thread_changes() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        store(&db, message_id, "model-a", false, 5, 100, &rail()).unwrap();

        assert_eq!(load(&db, message_id, "model-b", false, 5, 100).unwrap(), None);
        assert_eq!(load(&db, message_id, "model-a", true, 5, 100).unwrap(), None);
        assert_eq!(load(&db, message_id, "model-a", false, 6, 100).unwrap(), None);
        assert_eq!(load(&db, message_id, "model-a", false, 5, 101).unwrap(), None);
    }

    #[test]
    fn clears_every_cached_rail() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        store(&db, message_id, "model-a", false, 5, 100, &rail()).unwrap();
        clear_all(&db).unwrap();

        let remaining: i64 = db
            .conn
            .lock()
            .unwrap()
            .query_row("select count(*) from ai_rail_cache", [], |row| row.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[test]
    fn drops_corrupt_payloads() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "insert into ai_rail_cache
                 (message_id, model, full_body, thread_latest_id, thread_latest_date, payload, created_at)
                 values (?1, 'model-a', 0, 5, 100, 'not json', ?2)",
                params![message_id, now()],
            )
            .unwrap();
        }

        assert_eq!(load(&db, message_id, "model-a", false, 5, 100).unwrap(), None);
        let remaining: i64 = db
            .conn
            .lock()
            .unwrap()
            .query_row("select count(*) from ai_rail_cache", [], |row| row.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
    }
}
