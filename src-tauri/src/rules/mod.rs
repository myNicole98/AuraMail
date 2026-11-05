//! Mail rules: model, storage and the entry point used by sync and the sweep.
//!
//! Conditions and actions are stored as JSON so the schema does not need a
//! table per variant. Evaluated by `eval`, executed by `actions`.

pub mod actions;
pub mod eval;
pub mod run;

use serde::{Deserialize, Serialize};

use crate::db::{now, Database};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    High,
    Normal,
    Low,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

impl Priority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Normal => "normal",
            Self::Low => "low",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "high" => Self::High,
            "low" => Self::Low,
            _ => Self::Normal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sensitivity {
    #[default]
    Normal,
    Personal,
    Private,
    Confidential,
}

impl Sensitivity {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "personal" => Self::Personal,
            "private" => Self::Private,
            "confidential" => Self::Confidential,
            _ => Self::Normal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    #[default]
    Normal,
    Invitation,
    Receipt,
}

/// Every Outlook condition the client can express. `kind` is the serde tag used
/// by the frontend.
fn one_or_many<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(value) => vec![value],
        OneOrMany::Many(values) => values,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Condition {
    FromContains {
        #[serde(alias = "value", deserialize_with = "one_or_many")]
        values: Vec<String>,
    },
    ToContains {
        #[serde(alias = "value", deserialize_with = "one_or_many")]
        values: Vec<String>,
    },
    CcContains {
        #[serde(alias = "value", deserialize_with = "one_or_many")]
        values: Vec<String>,
    },
    OnBehalfOf,
    MyNameInTo,
    MyNameInCc,
    MyNameInToOrCc,
    MyNameNotInTo,
    OnlyRecipient,
    MentionedInBody,
    SubjectContains {
        #[serde(alias = "value", deserialize_with = "one_or_many")]
        values: Vec<String>,
    },
    SubjectOrBodyContains {
        #[serde(alias = "value", deserialize_with = "one_or_many")]
        values: Vec<String>,
    },
    BodyContains {
        #[serde(alias = "value", deserialize_with = "one_or_many")]
        values: Vec<String>,
    },
    SenderAddressContains {
        #[serde(alias = "value", deserialize_with = "one_or_many")]
        values: Vec<String>,
    },
    RecipientAddressContains {
        #[serde(alias = "value", deserialize_with = "one_or_many")]
        values: Vec<String>,
    },
    HeaderContains {
        name: String,
        #[serde(alias = "value", deserialize_with = "one_or_many")]
        values: Vec<String>,
    },
    Importance { value: Priority },
    Sensitivity { value: Sensitivity },
    ExternalSender,
    HasFlag,
    MessageType { value: MessageType },
    HasAttachment,
    HasLabel { label_id: i64 },
    HasAnyLabel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    MoveTo { folder_id: i64 },
    CopyTo { folder_id: i64 },
    Delete,
    PermanentDelete,
    MarkRead,
    MarkJunk,
    SetImportance { value: Priority },
    AddLabel { label_id: i64 },
    ClearLabels,
    SetFlag,
    ClearFlag,
    CompleteFlag,
    ForwardTo { address: String },
    ForwardAsAttachment { address: String },
    RedirectTo { address: String },
    NotifyDesktop,
    PlaySound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub enabled: bool,
    pub position: i64,
    pub stop_processing: bool,
    pub conditions: Vec<Condition>,
    pub exceptions: Vec<Condition>,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleInput {
    pub account_id: i64,
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub stop_processing: bool,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub exceptions: Vec<Condition>,
    #[serde(default)]
    pub actions: Vec<Action>,
}

fn default_enabled() -> bool {
    true
}

fn decode_conditions(raw: &str) -> Vec<Condition> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn decode_actions(raw: &str) -> Vec<Action> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn encode<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "[]".to_string())
}

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Rule> {
    let conditions: String = row.get(6)?;
    let exceptions: String = row.get(7)?;
    let actions: String = row.get(8)?;
    Ok(Rule {
        id: row.get(0)?,
        account_id: row.get(1)?,
        name: row.get(2)?,
        enabled: row.get::<_, i64>(3)? != 0,
        position: row.get(4)?,
        stop_processing: row.get::<_, i64>(5)? != 0,
        conditions: decode_conditions(&conditions),
        exceptions: decode_conditions(&exceptions),
        actions: decode_actions(&actions),
    })
}

const RULE_COLUMNS: &str = "id, account_id, name, enabled, position, stop_processing, conditions, exceptions, actions";

pub fn list(db: &Database, account_id: Option<i64>) -> AppResult<Vec<Rule>> {
    let conn = db.read();
    let (sql, params): (String, Vec<rusqlite::types::Value>) = match account_id {
        Some(id) => (
            format!(
                "select {RULE_COLUMNS} from rules where account_id = ?1 order by position, id"
            ),
            vec![rusqlite::types::Value::Integer(id)],
        ),
        None => (
            format!("select {RULE_COLUMNS} from rules order by account_id, position, id"),
            Vec::new(),
        ),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params), from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn enabled_for_account(db: &Database, account_id: i64) -> AppResult<Vec<Rule>> {
    Ok(list(db, Some(account_id))?
        .into_iter()
        .filter(|rule| rule.enabled)
        .collect())
}

pub fn create(db: &Database, input: &RuleInput) -> AppResult<Rule> {
    if input.name.trim().is_empty() {
        return Err(AppError::Message("rule name cannot be empty".into()));
    }
    let conn = db.conn.lock().unwrap();
    let position: i64 = conn.query_row(
        "select coalesce(max(position), -1) + 1 from rules where account_id = ?1",
        [input.account_id],
        |row| row.get(0),
    )?;
    conn.execute(
        "insert into rules (account_id, name, enabled, position, stop_processing, conditions, exceptions, actions, created_at)
         values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            input.account_id,
            input.name.trim(),
            input.enabled as i64,
            position,
            input.stop_processing as i64,
            encode(&input.conditions),
            encode(&input.exceptions),
            encode(&input.actions),
            now()
        ],
    )?;
    Ok(Rule {
        id: conn.last_insert_rowid(),
        account_id: input.account_id,
        name: input.name.trim().to_string(),
        enabled: input.enabled,
        position,
        stop_processing: input.stop_processing,
        conditions: input.conditions.clone(),
        exceptions: input.exceptions.clone(),
        actions: input.actions.clone(),
    })
}

pub fn update(db: &Database, id: i64, input: &RuleInput) -> AppResult<Rule> {
    if input.name.trim().is_empty() {
        return Err(AppError::Message("rule name cannot be empty".into()));
    }
    let conn = db.conn.lock().unwrap();
    let changed = conn.execute(
        "update rules set name = ?2, enabled = ?3, stop_processing = ?4, conditions = ?5,
                exceptions = ?6, actions = ?7
         where id = ?1",
        rusqlite::params![
            id,
            input.name.trim(),
            input.enabled as i64,
            input.stop_processing as i64,
            encode(&input.conditions),
            encode(&input.exceptions),
            encode(&input.actions)
        ],
    )?;
    if changed == 0 {
        return Err(AppError::Message("rule not found".into()));
    }
    let position: i64 = conn.query_row("select position from rules where id = ?1", [id], |row| {
        row.get(0)
    })?;
    Ok(Rule {
        id,
        account_id: input.account_id,
        name: input.name.trim().to_string(),
        enabled: input.enabled,
        position,
        stop_processing: input.stop_processing,
        conditions: input.conditions.clone(),
        exceptions: input.exceptions.clone(),
        actions: input.actions.clone(),
    })
}

pub fn delete(db: &Database, id: i64) -> AppResult<()> {
    let conn = db.conn.lock().unwrap();
    conn.execute("delete from rules where id = ?1", [id])?;
    Ok(())
}

pub fn reorder(db: &Database, account_id: i64, ids: &[i64]) -> AppResult<()> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    for (position, id) in ids.iter().enumerate() {
        tx.execute(
            "update rules set position = ?2 where id = ?1 and account_id = ?3",
            rusqlite::params![id, position as i64, account_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Database {
        let db = Database::open_memory().unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "insert into accounts (id, display_name, email, username, imap_host, imap_port,
                    imap_security, smtp_host, smtp_port, smtp_security, created_at)
                 values (1, 'Test', 'me@example.com', 'me@example.com', 'imap.example.com', 993,
                    'tls', 'smtp.example.com', 465, 'tls', 0)",
                [],
            )
            .unwrap();
        db
    }

    fn input(account_id: i64, name: &str) -> RuleInput {
        RuleInput {
            account_id,
            name: name.into(),
            enabled: true,
            stop_processing: false,
            conditions: vec![Condition::SubjectContains {
                values: vec!["invoice".into()],
            }],
            exceptions: Vec::new(),
            actions: vec![Action::MarkRead],
        }
    }

    #[test]
    fn create_list_update_delete() {
        let db = db();
        let rule = create(&db, &input(1, "Invoices")).unwrap();
        assert_eq!(list(&db, Some(1)).unwrap().len(), 1);
        assert_eq!(rule.conditions.len(), 1);

        let mut changed = input(1, "Invoices 2");
        changed.actions = vec![Action::SetFlag];
        let updated = update(&db, rule.id, &changed).unwrap();
        assert_eq!(updated.name, "Invoices 2");
        assert_eq!(updated.actions, vec![Action::SetFlag]);

        delete(&db, rule.id).unwrap();
        assert!(list(&db, Some(1)).unwrap().is_empty());
    }

    #[test]
    fn rules_round_trip_every_variant() {
        let db = db();
        let mut rule = input(1, "Kitchen sink");
        rule.conditions = vec![
            Condition::FromContains {
                values: vec!["a@b.c".into()],
            },
            Condition::OnBehalfOf,
            Condition::HeaderContains {
                name: "X-Spam".into(),
                values: vec!["yes".into()],
            },
            Condition::Importance {
                value: Priority::High,
            },
            Condition::HasLabel { label_id: 7 },
        ];
        rule.actions = vec![
            Action::MoveTo { folder_id: 3 },
            Action::AddLabel { label_id: 7 },
            Action::ForwardTo {
                address: "x@y.z".into(),
            },
        ];
        let created = create(&db, &rule).unwrap();
        let loaded = &list(&db, Some(1)).unwrap()[0];
        assert_eq!(loaded.conditions, rule.conditions);
        assert_eq!(loaded.actions, rule.actions);
        assert_eq!(loaded.id, created.id);
    }

    #[test]
    fn reorder_sets_positions() {
        let db = db();
        let first = create(&db, &input(1, "First")).unwrap();
        let second = create(&db, &input(1, "Second")).unwrap();
        assert_eq!(second.position, first.position + 1);
        reorder(&db, 1, &[second.id, first.id]).unwrap();
        let names: Vec<_> = list(&db, Some(1))
            .unwrap()
            .into_iter()
            .map(|rule| rule.name)
            .collect();
        assert_eq!(names, vec!["Second", "First"]);
    }

    #[test]
    fn conditions_accept_a_single_legacy_value() {
        let json = r#"[{"kind":"fromContains","value":"alice@example.com"}]"#;
        let conditions: Vec<Condition> = serde_json::from_str(json).unwrap();
        assert_eq!(
            conditions,
            vec![Condition::FromContains {
                values: vec!["alice@example.com".into()]
            }]
        );
    }

    #[test]
    fn disabled_rules_are_filtered() {
        let db = db();
        let mut rule = input(1, "Off");
        rule.enabled = false;
        create(&db, &rule).unwrap();
        assert!(enabled_for_account(&db, 1).unwrap().is_empty());
    }
}
