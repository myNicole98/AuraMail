use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::ai::prompts::{self, TabContext};
use crate::ai::MailContext;
use crate::db::{now, Database};
use crate::error::{AppError, AppResult};
use crate::mail;
use crate::secrets::SecretStore;
use crate::tabs::Tab;

pub const BODY_LIMIT: usize = 2000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Classification {
    pub tab_id: Option<i64>,
    pub confidence: f64,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassificationResult {
    pub message_id: i64,
    pub classification: Option<Classification>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ClassificationBatch {
    pub classified: u32,
    pub failed: u32,
    pub results: Vec<ClassificationResult>,
}

#[derive(Deserialize)]
struct RawClassification {
    #[serde(default)]
    tab_id: Option<i64>,
    #[serde(default)]
    confidence: f64,
}

fn strip_fences(raw: &str) -> &str {
    let trimmed = raw.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let body = rest.split_once('\n').map(|(_, body)| body).unwrap_or(rest);
    body.trim()
        .strip_suffix("```")
        .unwrap_or(body.trim())
        .trim()
}

pub fn parse_classification(raw: &str, allowed: &[i64]) -> AppResult<Classification> {
    let parsed: RawClassification = serde_json::from_str(strip_fences(raw))
        .map_err(|_| AppError::Message("classification response was invalid".into()))?;
    Ok(Classification {
        tab_id: parsed.tab_id.filter(|id| allowed.contains(id)),
        confidence: parsed.confidence.clamp(0.0, 1.0),
        model: String::new(),
    })
}

pub fn validate_classification(classification: Classification, allowed: &[i64]) -> Classification {
    Classification {
        tab_id: classification.tab_id.filter(|id| allowed.contains(id)),
        confidence: classification.confidence.clamp(0.0, 1.0),
        model: classification.model,
    }
}

pub fn body_excerpt(body: &str) -> String {
    body.trim().chars().take(BODY_LIMIT).collect()
}

fn allowed_ids(tabs: &[Tab]) -> Vec<i64> {
    tabs.iter().map(|tab| tab.id).collect()
}

fn tab_contexts(tabs: &[Tab]) -> Vec<TabContext> {
    tabs.iter()
        .map(|tab| TabContext {
            id: tab.id,
            name: tab.name.clone(),
            description: tab.description.clone(),
        })
        .collect()
}

pub fn cached(db: &Database, message_id: i64) -> AppResult<Option<Classification>> {
    let conn = db.conn.lock().unwrap();
    Ok(conn
        .query_row(
            "select tab_id, confidence, model from classifications where message_id = ?1",
            params![message_id],
            |row| {
                Ok(Classification {
                    tab_id: row.get(0)?,
                    confidence: row.get(1)?,
                    model: row.get(2)?,
                })
            },
        )
        .optional()?)
}

fn space_for_account(db: &Database, account_id: i64) -> AppResult<i64> {
    db.conn
        .lock()
        .unwrap()
        .query_row(
            "select space_id from accounts where id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::Message("account not found".into()))
}

#[allow(dead_code)]
pub fn ensure_allowed(
    db: &Database,
    _secrets: &dyn SecretStore,
    message_id: i64,
) -> AppResult<i64> {
    let message = mail::message_by_id(db, message_id)?;
    let space_id = space_for_account(db, message.account_id)?;
    if !crate::ai::space_enabled(db, space_id)? {
        return Err(AppError::Message("AI is disabled for this space".into()));
    }
    Ok(space_id)
}

pub fn set_manual(
    db: &Database,
    message_id: i64,
    tab_id: Option<i64>,
) -> AppResult<Classification> {
    let message = mail::message_by_id(db, message_id)?;
    let space_id = space_for_account(db, message.account_id)?;
    let (revision, tabs) = crate::tabs::snapshot(db, space_id)?;
    if let Some(tab_id) = tab_id {
        if !tabs.iter().any(|tab| tab.id == tab_id) {
            return Err(AppError::Message("tab not found".into()));
        }
    }
    let classification = Classification {
        tab_id,
        confidence: if tab_id.is_some() { 1.0 } else { 0.0 },
        model: "manual".into(),
    };
    store(db, message_id, &classification, revision)?;
    Ok(classification)
}

pub fn store(
    db: &Database,
    message_id: i64,
    classification: &Classification,
    expected_revision: i64,
) -> AppResult<()> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    let current = crate::tabs::revision_from_conn(&tx)?;
    if current != expected_revision {
        return Err(AppError::Message(
            "tabs changed during classification".into(),
        ));
    }
    let (account_id, message_key, thread_key): (i64, Option<String>, String) = tx.query_row(
        "select account_id, message_id, thread_key from messages where id = ?1",
        params![message_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let mut target_ids = vec![message_id];
    if let Some(message_key) = message_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let mut stmt = tx.prepare(
            "select id from messages where account_id = ?1 and trim(message_id) = ?2",
        )?;
        let rows = stmt.query_map(params![account_id, message_key], |row| row.get(0))?;
        for id in rows {
            let id = id?;
            if !target_ids.contains(&id) {
                target_ids.push(id);
            }
        }
    }
    if !thread_key.trim().is_empty() {
        let mut stmt =
            tx.prepare("select id from messages where account_id = ?1 and thread_key = ?2")?;
        let rows = stmt.query_map(params![account_id, thread_key.trim()], |row| row.get(0))?;
        for id in rows {
            let id = id?;
            if !target_ids.contains(&id) {
                target_ids.push(id);
            }
        }
    }
    let classified_at = now();
    for target_id in target_ids {
        tx.execute(
            "insert into classifications (message_id, tab_id, confidence, model, classified_at) values (?1, ?2, ?3, ?4, ?5) on conflict(message_id) do update set tab_id = excluded.tab_id, confidence = excluded.confidence, model = excluded.model, classified_at = excluded.classified_at",
            params![
                target_id,
                classification.tab_id,
                classification.confidence,
                classification.model,
                classified_at,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn message_body(db: &Database, message: &mail::Message, full_body: bool) -> AppResult<String> {
    if !full_body {
        return Ok(body_excerpt(&message.snippet));
    }
    let detail = mail::get_message_detail(db, message.id)?;
    let body = detail
        .text_body
        .as_deref()
        .map(str::trim)
        .filter(|body| !body.is_empty())
        .unwrap_or_else(|| message.snippet.trim());
    Ok(body_excerpt(body))
}

pub async fn classify(
    db: &Database,
    secrets: &dyn SecretStore,
    message_id: i64,
    reclassify: bool,
) -> AppResult<Classification> {
    let message = mail::message_by_id(db, message_id)?;
    let space_id = space_for_account(db, message.account_id)?;
    let (revision, tabs) = crate::tabs::snapshot(db, space_id)?;
    if !reclassify {
        if let Some(cached) = cached(db, message_id)? {
            return Ok(cached);
        }
    }
    if !crate::ai::space_enabled(db, space_id)? {
        return Err(AppError::Message("AI is disabled for this space".into()));
    }
    let full_body = crate::ai::full_body_enabled(db, space_id)?;
    let allowed = allowed_ids(&tabs);
    let context = MailContext {
        from_name: message.from_name.clone(),
        from_addr: message.from_addr.clone(),
        subject: message.subject.clone(),
        body: message_body(db, &message, full_body)?,
    };
    let prompt = prompts::classification_prompt(&context, &tab_contexts(&tabs));
    let (config, client) = crate::ai::configured_client(db, secrets, space_id)?;
    let raw = client.chat(&prompt).await?;
    let parsed = match parse_classification(&raw, &allowed) {
        Ok(parsed) => parsed,
        Err(_) => {
            let raw = client.chat(&prompt).await?;
            parse_classification(&raw, &allowed).unwrap_or(Classification {
                tab_id: None,
                confidence: 0.0,
                model: String::new(),
            })
        }
    };
    let classification = validate_classification(parsed, &allowed);
    let classification = Classification {
        model: config.model,
        ..classification
    };
    store(db, message_id, &classification, revision)?;
    Ok(classification)
}

pub async fn classify_ids_detailed(
    db: &Database,
    secrets: &dyn SecretStore,
    message_ids: &[i64],
) -> ClassificationBatch {
    let mut batch = ClassificationBatch::default();
    let mut counted = std::collections::HashSet::new();
    for message_id in message_ids {
        match classify(db, secrets, *message_id, false).await {
            Ok(classification) => {
                // The same mail can exist in Inbox, All Mail, and other folders.
                // Count each logical message once for the user-facing notice.
                let identity = mail::message_by_id(db, *message_id)
                    .map(|message| message.identity())
                    .unwrap_or_else(|_| format!("id:{message_id}"));
                if counted.insert(identity) {
                    batch.classified += 1;
                }
                batch.results.push(ClassificationResult {
                    message_id: *message_id,
                    classification: Some(classification),
                    error: None,
                });
            }
            Err(error) => {
                batch.failed += 1;
                batch.results.push(ClassificationResult {
                    message_id: *message_id,
                    classification: None,
                    error: Some(error.to_string()),
                });
            }
        }
    }
    batch
}

#[cfg(test)]
pub async fn classify_backlog(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: Option<i64>,
    batch_size: u32,
) -> AppResult<(u32, u32)> {
    classify_backlog_in(db, secrets, account_id, None, batch_size).await
}

#[allow(dead_code)]
pub async fn classify_backlog_in(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: Option<i64>,
    folder_id: Option<i64>,
    batch_size: u32,
) -> AppResult<(u32, u32)> {
    classify_backlog_in_space(
        db,
        secrets,
        account_id,
        folder_id,
        None,
        batch_size,
    )
    .await
}

pub async fn classify_backlog_in_space(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: Option<i64>,
    folder_id: Option<i64>,
    space_id: Option<i64>,
    batch_size: u32,
) -> AppResult<(u32, u32)> {
    let mut before_id = None;
    let mut classified = 0;
    let mut failed = 0;
    loop {
        let ids = mail::unclassified_message_ids_before(
            db, account_id, folder_id, before_id, batch_size,
        )?;
        let Some(next_before) = ids.last().copied() else {
            break;
        };
        for message_id in ids {
            if let Some(space_id) = space_id {
                let Ok(message) = mail::message_by_id(db, message_id) else {
                    continue;
                };
                let Ok(message_space) =
                    crate::accounts::space_for_account(db, message.account_id)
                else {
                    continue;
                };
                if message_space != space_id {
                    continue;
                }
            }
            match classify(db, secrets, message_id, false).await {
                Ok(_) => classified += 1,
                Err(_) => failed += 1,
            }
        }
        before_id = Some(next_before);
    }
    Ok((classified, failed))
}

#[cfg(test)]
mod tests {
    use super::{
        body_excerpt, classify, classify_backlog, classify_ids_detailed, ensure_allowed,
        parse_classification, set_manual, store, validate_classification, Classification,
    };
    use crate::ai::{AiConfig, AiPreferences};
    use crate::db::Database;
    use crate::secrets::{MemoryStore, SecretStore};
    use rusqlite::params;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn classification_requires_a_known_tab() {
        let parsed = parse_classification(r#"{"tab_id": 7, "confidence": 0.9}"#, &[7, 8]).unwrap();
        assert_eq!(parsed.tab_id, Some(7));
        assert!(
            parse_classification(r#"{"tab_id": 99, "confidence": 0.9}"#, &[7, 8])
                .unwrap()
                .tab_id
                .is_none()
        );
    }

    #[test]
    fn strips_markdown_fences_before_json_parse() {
        let parsed =
            parse_classification("```json\n{\"tab_id\": 7, \"confidence\": 0.8}\n```", &[7])
                .unwrap();
        assert_eq!(parsed.tab_id, Some(7));
        assert_eq!(parsed.confidence, 0.8);
    }

    #[test]
    fn accepts_null_and_missing_tab_ids() {
        assert!(
            parse_classification(r#"{"tab_id": null, "confidence": 0.4}"#, &[7])
                .unwrap()
                .tab_id
                .is_none()
        );
        assert!(parse_classification(r#"{"confidence": 0.4}"#, &[7])
            .unwrap()
            .tab_id
            .is_none());
    }

    #[test]
    fn clamps_confidence_to_a_fraction() {
        assert_eq!(
            parse_classification(r#"{"tab_id": 7, "confidence": 4}"#, &[7])
                .unwrap()
                .confidence,
            1.0
        );
        assert_eq!(
            parse_classification(r#"{"tab_id": 7, "confidence": -2}"#, &[7])
                .unwrap()
                .confidence,
            0.0
        );
        assert_eq!(
            parse_classification(r#"{"tab_id": 7}"#, &[7])
                .unwrap()
                .confidence,
            0.0
        );
    }

    #[test]
    fn rejects_invalid_json_and_wrong_shapes() {
        assert!(parse_classification("not json", &[7]).is_err());
        assert!(parse_classification(r#"{"tab_id": "7"}"#, &[7]).is_err());
        assert!(parse_classification(r#"{"tab_id": 7, "confidence": "high"}"#, &[7]).is_err());
    }

    #[test]
    fn invalid_tab_id_becomes_unclassified() {
        let result = validate_classification(
            parse_classification(r#"{"tab_id": 99, "confidence": 1.0}"#, &[1, 2]).unwrap(),
            &[1, 2],
        );
        assert_eq!(result.tab_id, None);
    }

    #[test]
    fn body_excerpt_is_limited_to_two_thousand_characters() {
        let body = "é".repeat(2500);
        let excerpt = body_excerpt(&body);
        assert_eq!(excerpt.chars().count(), 2000);
        assert_eq!(body_excerpt("short").chars().count(), 5);
    }

    #[test]
    fn parsed_classifications_start_without_a_model_name() {
        let parsed = parse_classification(r#"{"tab_id": 7, "confidence": 0.8}"#, &[7]).unwrap();
        assert_eq!(parsed.model, "");
    }

    #[test]
    fn manual_classification_sets_and_clears_the_tab() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        let tab_id = crate::tabs::list(&db, space(&db)).unwrap()[1].id;

        let assigned = set_manual(&db, message_id, Some(tab_id)).unwrap();
        assert_eq!(assigned.tab_id, Some(tab_id));
        assert_eq!(assigned.model, "manual");
        assert_eq!(super::cached(&db, message_id).unwrap().unwrap(), assigned);

        let cleared = set_manual(&db, message_id, None).unwrap();
        assert_eq!(cleared.tab_id, None);
        assert_eq!(super::cached(&db, message_id).unwrap().unwrap(), cleared);
    }

    #[test]
    fn manual_classification_rejects_unknown_tabs() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        assert!(set_manual(&db, message_id, Some(999)).is_err());
        assert!(super::cached(&db, message_id).unwrap().is_none());
    }

    #[test]
    fn manual_classification_updates_duplicate_copies_and_their_tab() {
        let db = Database::open_memory().unwrap();
        let (first, second, second_folder) = seed_duplicate_messages(&db);
        let tab_id = crate::tabs::list(&db, space(&db)).unwrap()[1].id;

        set_manual(&db, first, Some(tab_id)).unwrap();

        assert_eq!(
            super::cached(&db, first).unwrap().map(|value| value.tab_id),
            Some(Some(tab_id))
        );
        assert_eq!(
            super::cached(&db, second).unwrap().map(|value| value.tab_id),
            Some(Some(tab_id))
        );
        let threads = crate::mail::list_threads(
            &db,
            &crate::mail::MessageQuery {
                folder_id: Some(second_folder),
                tab_id: Some(tab_id),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!threads.is_empty());
    }

    #[test]
    fn clearing_one_duplicate_copy_clears_all_copies() {
        let db = Database::open_memory().unwrap();
        let (first, second, _) = seed_duplicate_messages(&db);
        let tab_id = crate::tabs::list(&db, space(&db)).unwrap()[1].id;
        set_manual(&db, first, Some(tab_id)).unwrap();

        set_manual(&db, second, None).unwrap();

        assert_eq!(
            super::cached(&db, first).unwrap().map(|value| value.tab_id),
            Some(None)
        );
        assert_eq!(
            super::cached(&db, second).unwrap().map(|value| value.tab_id),
            Some(None)
        );
    }

    #[test]
    fn manual_classification_updates_the_whole_thread_and_sent_view() {
        let db = Database::open_memory().unwrap();
        let (displayed, sent) = seed_thread_messages(&db);
        let tab_id = crate::tabs::list(&db, space(&db)).unwrap()[1].id;

        set_manual(&db, displayed, Some(tab_id)).unwrap();

        assert_eq!(
            super::cached(&db, sent).unwrap().map(|value| value.tab_id),
            Some(Some(tab_id))
        );
        let threads = crate::mail::list_threads(
            &db,
            &crate::mail::MessageQuery {
                unified: Some("sent".into()),
                tab_id: Some(tab_id),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!threads.is_empty());
    }

    fn seed_thread_messages(db: &Database) -> (i64, i64) {
        let space_id = space(db);
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "insert into accounts (display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, space_id, created_at) values ('Me', 'me@example.com', 'me@example.com', 'imap.example.com', 993, 'tls', 'smtp.example.com', 465, 'tls', ?1, 0)",
            params![space_id],
        )
        .unwrap();
        let account_id = conn.last_insert_rowid();
        conn.execute(
            "insert into folders (account_id, path, name, role) values (?1, 'Trash', 'Trash', 'trash')",
            params![account_id],
        )
        .unwrap();
        let trash_id = conn.last_insert_rowid();
        conn.execute(
            "insert into folders (account_id, path, name, role) values (?1, 'Sent', 'Sent', 'sent')",
            params![account_id],
        )
        .unwrap();
        let sent_id = conn.last_insert_rowid();
        conn.execute(
            "insert into messages (account_id, folder_id, uid, message_id, thread_key, from_addr, from_name, subject, snippet, body_cached) values (?1, ?2, 1, '<displayed@example.com>', 'thread-1', 'ada@example.com', 'Ada', 'Displayed', 'short snippet', 0)",
            params![account_id, trash_id],
        )
        .unwrap();
        let displayed = conn.last_insert_rowid();
        conn.execute(
            "insert into messages (account_id, folder_id, uid, message_id, thread_key, from_addr, from_name, subject, snippet, body_cached) values (?1, ?2, 1, '<sent@example.com>', 'thread-1', 'me@example.com', 'Me', 'Sent copy', 'short snippet', 0)",
            params![account_id, sent_id],
        )
        .unwrap();
        let sent = conn.last_insert_rowid();
        (displayed, sent)
    }

    fn seed_duplicate_messages(db: &Database) -> (i64, i64, i64) {
        let space_id = space(db);
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "insert into accounts (display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, space_id, created_at) values ('Me', 'me@example.com', 'me@example.com', 'imap.example.com', 993, 'tls', 'smtp.example.com', 465, 'tls', ?1, 0)",
            params![space_id],
        )
        .unwrap();
        let account_id = conn.last_insert_rowid();
        conn.execute(
            "insert into folders (account_id, path, name, role) values (?1, 'INBOX', 'Inbox', 'inbox')",
            params![account_id],
        )
        .unwrap();
        let inbox_id = conn.last_insert_rowid();
        conn.execute(
            "insert into folders (account_id, path, name, role) values (?1, 'All Mail', 'All Mail', 'all')",
            params![account_id],
        )
        .unwrap();
        let all_id = conn.last_insert_rowid();
        let message_id = "<duplicate@example.com>";
        conn.execute(
            "insert into messages (account_id, folder_id, uid, message_id, from_addr, from_name, subject, snippet, body_cached) values (?1, ?2, 1, ?3, 'ada@example.com', 'Ada', 'Duplicate', 'short snippet', 0)",
            params![account_id, inbox_id, message_id],
        )
        .unwrap();
        let first = conn.last_insert_rowid();
        conn.execute(
            "insert into messages (account_id, folder_id, uid, message_id, from_addr, from_name, subject, snippet, body_cached) values (?1, ?2, 1, ?3, 'ada@example.com', 'Ada', 'Duplicate', 'short snippet', 0)",
            params![account_id, all_id, message_id],
        )
        .unwrap();
        let second = conn.last_insert_rowid();
        (first, second, all_id)
    }

    #[tokio::test]
    async fn detailed_classification_returns_one_result_per_message() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "{\"tab_id\": 1, \"confidence\": 0.9}"}}]
            })))
            .expect(2)
            .mount(&server)
            .await;
        let db = Database::open_memory().unwrap();
        let ids = seed_messages(&db, 2);
        let secrets = configure_ai(&db, &server);

        let batch = classify_ids_detailed(&db, &secrets, &ids).await;
        assert_eq!(batch.classified, 2);
        assert_eq!(batch.failed, 0);
        assert_eq!(batch.results.len(), 2);
        assert!(batch.results.iter().all(|result| {
            result.error.is_none()
                && result.classification.as_ref().map(|value| value.tab_id) == Some(Some(1))
        }));
    }

    fn space(db: &Database) -> i64 {
        crate::spaces::default_id(db).unwrap()
    }

    fn seed_messages(db: &Database, count: i64) -> Vec<i64> {
        let space_id = space(db);
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "insert into accounts (display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, space_id, created_at) values ('Me', 'me@example.com', 'me@example.com', 'imap.example.com', 993, 'tls', 'smtp.example.com', 465, 'tls', ?1, 0)",
            params![space_id],
        )
        .unwrap();
        let account_id = conn.last_insert_rowid();
        conn.execute(
            "insert into folders (account_id, path, name, role) values (?1, 'INBOX', 'Inbox', 'inbox')",
            params![account_id],
        )
        .unwrap();
        let folder_id = conn.last_insert_rowid();
        let mut ids = Vec::new();
        for uid in 1..=count {
            conn.execute(
                "insert into messages (account_id, folder_id, uid, from_addr, from_name, subject, snippet, body_cached) values (?1, ?2, ?3, 'ada@example.com', 'Ada', 'Needle subject', 'short snippet', 0)",
                params![account_id, folder_id, uid],
            )
            .unwrap();
            ids.push(conn.last_insert_rowid());
        }
        ids
    }

    fn seed_message(db: &Database) -> i64 {
        seed_messages(db, 1)[0]
    }

    fn configure_ai(db: &Database, server: &MockServer) -> MemoryStore {
        let base_url = server.uri();
        let space_id = crate::spaces::default_id(db).unwrap();
        AiConfig {
            base_url: base_url.clone(),
            model: "test-model".into(),
            embedding_base_url: String::new(),
            embedding_model: String::new(),
            extra_headers: Vec::new(),
        }
        .save(db, space_id)
        .unwrap();
        let secrets = MemoryStore::default();
        secrets
            .set(&crate::ai::key_name(space_id, &base_url), "secret")
            .unwrap();
        secrets
    }

    fn cache_body(db: &Database, message_id: i64, body: &str) {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "insert into message_bodies (message_id, text_body, fetched_at) values (?1, ?2, 0)",
            params![message_id, body],
        )
        .unwrap();
        conn.execute(
            "update messages set body_cached = 1 where id = ?1",
            params![message_id],
        )
        .unwrap();
    }

    #[tokio::test]
    async fn cached_classifications_skip_the_ai_client() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        db.conn
            .lock()
            .unwrap()
            .execute(
                "insert into classifications (message_id, tab_id, confidence, model, classified_at) values (?1, 1, 0.8, 'cached-model', 0)",
                params![message_id],
            )
            .unwrap();
        let cached = classify(&db, &MemoryStore::default(), message_id, false)
            .await
            .unwrap();
        assert_eq!(cached.tab_id, Some(1));
        assert_eq!(cached.model, "cached-model");
    }

    #[tokio::test]
    async fn classify_sends_truncated_body_and_stores_the_model() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "{\"tab_id\": 1, \"confidence\": 0.9}"}}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        cache_body(&db, message_id, &"x".repeat(2500));
        crate::ai::set_preferences(
            &db,
            crate::spaces::default_id(&db).unwrap(),
            &AiPreferences {
                auto_classify: false,
                auto_rail: false,
                full_body_context: true,
                match_voice: false,
            },
        )
        .unwrap();
        let secrets = configure_ai(&db, &server);

        let classification = classify(&db, &secrets, message_id, false).await.unwrap();
        assert_eq!(classification.tab_id, Some(1));
        assert_eq!(classification.model, "test-model");
        assert_eq!(
            super::cached(&db, message_id).unwrap().unwrap(),
            classification
        );

        let requests = server.received_requests().await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        let prompt = payload["messages"][1]["content"].as_str().unwrap();
        assert!(prompt.contains(&"x".repeat(2000)));
        assert!(!prompt.contains(&"x".repeat(2001)));
        assert!(prompt.contains("Needle subject"));

        let cached = classify(&db, &secrets, message_id, false).await.unwrap();
        assert_eq!(cached, classification);
    }

    #[tokio::test]
    async fn backend_privacy_setting_prevents_body_sharing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "{\"tab_id\": 1, \"confidence\": 0.9}"}}]
            })))
            .mount(&server)
            .await;
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        cache_body(&db, message_id, &"x".repeat(2500));
        let secrets = configure_ai(&db, &server);

        classify(&db, &secrets, message_id, false).await.unwrap();
        let requests = server.received_requests().await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        let prompt = payload["messages"][1]["content"].as_str().unwrap();
        assert!(!prompt.contains(&"x".repeat(2000)));
        assert!(prompt.contains("short snippet"));
    }

    #[tokio::test]
    async fn invalid_responses_become_unclassified_after_one_retry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "not json"}}]
            })))
            .expect(2)
            .mount(&server)
            .await;
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        let secrets = configure_ai(&db, &server);

        let classification = classify(&db, &secrets, message_id, false).await.unwrap();
        assert_eq!(classification.tab_id, None);
        assert_eq!(classification.confidence, 0.0);
        assert_eq!(
            super::cached(&db, message_id).unwrap().unwrap(),
            classification
        );
    }

    #[test]
    fn classification_is_refused_when_the_space_ai_is_disabled() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let work = crate::spaces::create(&db, "Work").unwrap();
        let message_id = seed_message(&db);
        db.conn
            .lock()
            .unwrap()
            .execute(
                "update accounts set space_id = ?1 where id = (select account_id from messages where id = ?2)",
                params![work.id, message_id],
            )
            .unwrap();
        let error = ensure_allowed(&db, &secrets, message_id).unwrap_err();
        assert_eq!(error.to_string(), "AI is disabled for this space");
    }

    #[test]
    fn manual_classification_rejects_tabs_from_another_space() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        let work = crate::spaces::create(&db, "Work").unwrap();
        let work_tab = crate::tabs::list(&db, work.id).unwrap()[0].id;
        assert!(set_manual(&db, message_id, Some(work_tab)).is_err());
        assert!(super::cached(&db, message_id).unwrap().is_none());
    }

    #[test]
    fn stale_classifications_are_rejected_after_tab_deletion() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        let tab = crate::tabs::create(&db, space(&db), "Projects", "Project threads.", None, None)
            .unwrap();
        let revision = crate::tabs::revision(&db).unwrap();
        crate::tabs::delete(&db, space(&db), tab.id).unwrap();
        let classification = Classification {
            tab_id: Some(1),
            confidence: 0.9,
            model: "test-model".into(),
        };
        assert!(store(&db, message_id, &classification, revision).is_err());
        assert!(super::cached(&db, message_id).unwrap().is_none());
    }

    #[tokio::test]
    async fn classify_ids_only_processes_the_new_sync_batch() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "{\"tab_id\": 1, \"confidence\": 0.9}"}}]
            })))
            .expect(2)
            .mount(&server)
            .await;
        let db = Database::open_memory().unwrap();
        let ids = seed_messages(&db, 3);
        let secrets = configure_ai(&db, &server);

        let batch = classify_ids_detailed(&db, &secrets, &ids[..2]).await;
        assert_eq!((batch.classified, batch.failed), (2, 0));
        assert!(super::cached(&db, ids[2]).unwrap().is_none());
        assert!(super::cached(&db, ids[0]).unwrap().is_some());
    }

    #[tokio::test]
    async fn backlog_classification_drains_more_than_one_batch() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "{\"tab_id\": 1, \"confidence\": 0.9}"}}]
            })))
            .mount(&server)
            .await;
        let db = Database::open_memory().unwrap();
        seed_messages(&db, 30);
        let secrets = configure_ai(&db, &server);

        let (classified, failed) = classify_backlog(&db, &secrets, None, 25).await.unwrap();
        assert_eq!(classified, 30);
        assert_eq!(failed, 0);
        assert!(crate::mail::unclassified_message_ids(&db, None, 100)
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn backlog_moves_past_a_full_batch_of_null_cached_results() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "{\"tab_id\": 1, \"confidence\": 0.9}"}}]
            })))
            .expect(5)
            .mount(&server)
            .await;
        let db = Database::open_memory().unwrap();
        let ids = seed_messages(&db, 30);
        {
            let conn = db.conn.lock().unwrap();
            for message_id in &ids[5..] {
                conn.execute(
                    "insert into classifications (message_id, tab_id, confidence, model, classified_at) values (?1, null, 0.0, 'test-model', 0)",
                    params![message_id],
                )
                .unwrap();
            }
        }
        let secrets = configure_ai(&db, &server);

        let (classified, failed) = classify_backlog(&db, &secrets, None, 25).await.unwrap();
        assert_eq!(classified, 30);
        assert_eq!(failed, 0);
        for message_id in &ids[..5] {
            assert_eq!(
                super::cached(&db, *message_id).unwrap().unwrap().tab_id,
                Some(1)
            );
        }
        for message_id in &ids[5..] {
            assert_eq!(
                super::cached(&db, *message_id).unwrap().unwrap().tab_id,
                None
            );
        }
    }

    #[tokio::test]
    async fn backlog_moves_past_a_full_batch_of_failed_results() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let db = Database::open_memory().unwrap();
        seed_messages(&db, 30);
        let secrets = configure_ai(&db, &server);

        let (classified, failed) = classify_backlog(&db, &secrets, None, 25).await.unwrap();
        assert_eq!(classified, 0);
        assert_eq!(failed, 30);
    }
}
