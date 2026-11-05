//! Outlook-style labels (categories).
//!
//! Unlike the single AI tab in `classifications`, a message can carry any
//! number of labels. They are stored against the message's RFC 822
//! `Message-ID` (scoped by account) rather than a `messages.id`, so the label
//! is shared by every folder copy and survives a folder being cleared for a
//! UIDVALIDITY reset.

use serde::{Deserialize, Serialize};

use crate::db::{now, Database};
use crate::error::{AppError, AppResult};

/// Seeded on first run, in Outlook's order.
pub const DEFAULT_LABELS: &[(&str, &str)] = &[
    ("Blue", "#5b9bd5"),
    ("Green", "#4ea72e"),
    ("Orange", "#e07b39"),
    ("Purple", "#8b5cf6"),
    ("Red", "#e0413c"),
    ("Yellow", "#f2c94c"),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub position: i64,
}

/// Stable identity used by `message_labels`. Messages without a Message-ID
/// fall back to the local row id and are the one case a folder reset loses.
pub fn message_key(message_id: Option<&str>, row_id: i64) -> String {
    match message_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => value.to_string(),
        None => format!("local:{row_id}"),
    }
}

pub fn list(db: &Database) -> AppResult<Vec<Label>> {
    let conn = db.read();
    let mut stmt =
        conn.prepare("select id, name, color, position from labels order by position, id")?;
    let rows = stmt.query_map([], |row| {
        Ok(Label {
            id: row.get(0)?,
            name: row.get(1)?,
            color: row.get(2)?,
            position: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn create(db: &Database, name: &str, color: &str) -> AppResult<Label> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Message("label name cannot be empty".into()));
    }
    let conn = db.conn.lock().unwrap();
    let position: i64 = conn.query_row(
        "select coalesce(max(position), -1) + 1 from labels",
        [],
        |row| row.get(0),
    )?;
    conn.execute(
        "insert into labels (name, color, position, created_at) values (?1, ?2, ?3, ?4)",
        rusqlite::params![name, color, position, now()],
    )
    .map_err(|error| match error {
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            AppError::Message("a label with that name already exists".into())
        }
        other => other.into(),
    })?;
    Ok(Label {
        id: conn.last_insert_rowid(),
        name: name.to_string(),
        color: color.to_string(),
        position,
    })
}

pub fn update(db: &Database, id: i64, name: &str, color: &str) -> AppResult<Label> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Message("label name cannot be empty".into()));
    }
    let conn = db.conn.lock().unwrap();
    let changed = conn
        .execute(
            "update labels set name = ?1, color = ?2 where id = ?3",
            rusqlite::params![name, color, id],
        )
        .map_err(|error| match error {
            rusqlite::Error::SqliteFailure(failure, _)
                if failure.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                AppError::Message("a label with that name already exists".into())
            }
            other => other.into(),
        })?;
    if changed == 0 {
        return Err(AppError::Message("label not found".into()));
    }
    let position: i64 = conn.query_row("select position from labels where id = ?1", [id], |row| {
        row.get(0)
    })?;
    Ok(Label {
        id,
        name: name.to_string(),
        color: color.to_string(),
        position,
    })
}

pub fn delete(db: &Database, id: i64) -> AppResult<()> {
    let conn = db.conn.lock().unwrap();
    conn.execute("delete from labels where id = ?1", [id])?;
    Ok(())
}

/// Labels attached to a message identity, sorted by the label order.
pub fn for_message(db: &Database, account_id: i64, key: &str) -> AppResult<Vec<i64>> {
    let conn = db.read();
    let mut stmt = conn.prepare(
        "select ml.label_id from message_labels ml
         join labels l on l.id = ml.label_id
         where ml.account_id = ?1 and ml.message_key = ?2
         order by l.position, l.id",
    )?;
    let rows = stmt.query_map(rusqlite::params![account_id, key], |row| row.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn set_for_message(
    db: &Database,
    account_id: i64,
    key: &str,
    label_ids: &[i64],
) -> AppResult<()> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    tx.execute(
        "delete from message_labels where account_id = ?1 and message_key = ?2",
        rusqlite::params![account_id, key],
    )?;
    for label_id in label_ids {
        tx.execute(
            "insert or ignore into message_labels (account_id, message_key, label_id)
             values (?1, ?2, ?3)",
            rusqlite::params![account_id, key, label_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn add_for_message(db: &Database, account_id: i64, key: &str, label_id: i64) -> AppResult<()> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "insert or ignore into message_labels (account_id, message_key, label_id)
         values (?1, ?2, ?3)",
        rusqlite::params![account_id, key, label_id],
    )?;
    Ok(())
}

pub fn clear_for_message(db: &Database, account_id: i64, key: &str) -> AppResult<()> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "delete from message_labels where account_id = ?1 and message_key = ?2",
        rusqlite::params![account_id, key],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Database {
        let db = Database::open_memory().unwrap();
        for (id, name, email) in [
            (1, "Test", "me@example.com"),
            (2, "Other", "other@example.com"),
        ] {
            db.conn
                .lock()
                .unwrap()
                .execute(
                    "insert into accounts (id, display_name, email, username, imap_host, imap_port,
                        imap_security, smtp_host, smtp_port, smtp_security, created_at)
                     values (?1, ?2, ?3, ?3, 'imap.example.com', 993, 'tls', 'smtp.example.com', 465, 'tls', 0)",
                    rusqlite::params![id, name, email],
                )
                .unwrap();
        }
        db
    }

    #[test]
    fn seeds_the_outlook_palette() {
        let db = db();
        let labels = list(&db).unwrap();
        let names: Vec<_> = labels.iter().map(|label| label.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Blue", "Green", "Orange", "Purple", "Red", "Yellow"]
        );
    }

    #[test]
    fn create_update_delete_round_trip() {
        let db = db();
        let label = create(&db, "Invoices", "#123456").unwrap();
        assert_eq!(label.name, "Invoices");
        let updated = update(&db, label.id, "Receipts", "#654321").unwrap();
        assert_eq!(updated.color, "#654321");
        delete(&db, label.id).unwrap();
        assert!(list(&db).unwrap().iter().all(|item| item.id != label.id));
    }

    #[test]
    fn duplicate_names_are_rejected() {
        let db = db();
        let error = create(&db, "Blue", "#000000").unwrap_err();
        assert!(error.to_string().contains("already exists"));
    }

    #[test]
    fn message_labels_are_keyed_by_identity() {
        let db = db();
        let labels = list(&db).unwrap();
        set_for_message(&db, 1, "abc@example.com", &[labels[0].id, labels[1].id]).unwrap();
        assert_eq!(
            for_message(&db, 1, "abc@example.com").unwrap(),
            vec![labels[0].id, labels[1].id]
        );
        // A different account does not see them.
        assert!(for_message(&db, 2, "abc@example.com").unwrap().is_empty());
        clear_for_message(&db, 1, "abc@example.com").unwrap();
        assert!(for_message(&db, 1, "abc@example.com").unwrap().is_empty());
    }

    #[test]
    fn message_key_falls_back_to_the_row_id() {
        assert_eq!(message_key(Some(" id@host "), 4), "id@host");
        assert_eq!(message_key(Some(""), 4), "local:4");
        assert_eq!(message_key(None, 7), "local:7");
    }
}
