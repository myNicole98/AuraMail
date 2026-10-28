use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::accounts::{self, AccountAuth};
use crate::ai::AiConfig;
use crate::db::{self, Database};
use crate::error::{AppError, AppResult};
use crate::oauth;
use crate::secrets::SecretStore;
use crate::tabs;

const NAME_LIMIT: usize = 40;
const ICON_LIMIT: usize = 40;
const DEFAULT_SPACE_NAME: &str = "Personal";
const DEFAULT_SPACE_ICON: &str = "user";
const DEFAULT_ICON: &str = "layers";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Space {
    pub id: i64,
    pub name: String,
    pub icon: String,
    pub position: i64,
    pub created_at: i64,
}

fn row_to_space(row: &rusqlite::Row<'_>) -> rusqlite::Result<Space> {
    Ok(Space {
        id: row.get(0)?,
        name: row.get(1)?,
        icon: row.get(2)?,
        position: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn clean_name(value: &str) -> AppResult<String> {
    let name = value.trim();
    if name.is_empty() {
        return Err(AppError::Message("space name is required".into()));
    }
    if name.chars().count() > NAME_LIMIT {
        return Err(AppError::Message("space name is too long".into()));
    }
    Ok(name.to_string())
}

fn clean_icon(value: Option<&str>) -> AppResult<String> {
    let icon = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_ICON);
    if icon.chars().count() > ICON_LIMIT
        || !icon
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(AppError::Message("space icon is invalid".into()));
    }
    Ok(icon.to_ascii_lowercase())
}

fn is_unique_violation(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
    )
}

pub fn list(db: &Database) -> AppResult<Vec<Space>> {
    let conn = db.read();
    let mut stmt = conn.prepare("select id, name, icon, position, created_at from spaces order by position, id")?;
    let rows = stmt.query_map([], row_to_space)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn by_id(db: &Database, id: i64) -> AppResult<Space> {
    let conn = db.read();
    conn.query_row(
        "select id, name, icon, position, created_at from spaces where id = ?1",
        params![id],
        row_to_space,
    )
    .optional()?
    .ok_or_else(|| AppError::Message("space not found".into()))
}

#[allow(dead_code)]
pub fn default_id(db: &Database) -> AppResult<i64> {
    Ok(ensure_default(db)?.id)
}

#[allow(dead_code)]
pub fn ensure_default(db: &Database) -> AppResult<Space> {
    let spaces = list(db)?;
    if let Some(space) = spaces
        .iter()
        .find(|space| space.name.eq_ignore_ascii_case(DEFAULT_SPACE_NAME))
    {
        return Ok(space.clone());
    }
    if let Some(space) = spaces
        .iter()
        .find(|space| space.name.eq_ignore_ascii_case("Default"))
    {
        return Ok(space.clone());
    }
    if let Some(first) = spaces.first() {
        return Ok(first.clone());
    }
    create_with_icon(db, DEFAULT_SPACE_NAME, Some(DEFAULT_SPACE_ICON))
}

#[allow(dead_code)]
pub fn create(db: &Database, name: &str) -> AppResult<Space> {
    create_with_icon(db, name, None)
}

pub fn create_with_icon(
    db: &Database,
    name: &str,
    icon: Option<&str>,
) -> AppResult<Space> {
    let name = clean_name(name)?;
    let icon = clean_icon(icon)?;
    let created_at = db::now();
    let conn = db.conn.lock().unwrap();
    let position: i64 = conn.query_row(
        "select coalesce(max(position) + 1, 0) from spaces",
        [],
        |row| row.get(0),
    )?;
    let inserted = conn.execute(
        "insert into spaces (name, icon, position, created_at) values (?1, ?2, ?3, ?4)",
        params![name, icon, position, created_at],
    );
    if let Err(err) = inserted {
        if is_unique_violation(&err) {
            return Err(AppError::Message("space already exists".into()));
        }
        return Err(err.into());
    }
    let id = conn.last_insert_rowid();
    drop(conn);
    tabs::seed_space(db, id)?;
    Ok(Space {
        id,
        name,
        icon,
        position,
        created_at,
    })
}

#[allow(dead_code)]
pub fn rename(db: &Database, id: i64, name: &str) -> AppResult<Space> {
    rename_with_icon(db, id, name, None)
}

pub fn rename_with_icon(
    db: &Database,
    id: i64,
    name: &str,
    icon: Option<&str>,
) -> AppResult<Space> {
    let name = clean_name(name)?;
    let icon = icon.map(|value| clean_icon(Some(value))).transpose()?;
    let changed = db.conn.lock().unwrap().execute(
        "update spaces set name = ?1, icon = coalesce(?2, icon) where id = ?3",
        params![name, icon, id],
    );
    match changed {
        Ok(0) => Err(AppError::Message("space not found".into())),
        Ok(_) => by_id(db, id),
        Err(err) if is_unique_violation(&err) => {
            Err(AppError::Message("space already exists".into()))
        }
        Err(err) => Err(err.into()),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpaceDeleteAction {
    DeleteAccounts,
    Migrate {
        #[serde(rename = "targetSpaceId")]
        target_space_id: i64,
    },
}

fn delete_scope(db: &Database, id: i64) -> AppResult<()> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    tx.execute("delete from tabs where space_id = ?1", params![id])?;
    tx.execute(
        "delete from settings where key like ?1",
        params![format!("space:{id}:%")],
    )?;
    tx.execute("delete from spaces where id = ?1", params![id])?;
    tx.commit()?;
    Ok(())
}

pub fn delete(
    db: &Database,
    secrets: &dyn SecretStore,
    id: i64,
    action: SpaceDeleteAction,
) -> AppResult<Vec<Space>> {
    by_id(db, id)?;
    if list(db)?.len() <= 1 {
        return Err(AppError::Message(
            "the last space cannot be deleted".into(),
        ));
    }
    match action {
        SpaceDeleteAction::DeleteAccounts => {
            let accounts = accounts::list_for_space(db, id)?;
            let proton_usernames = accounts
                .iter()
                .filter(|account| account.auth_type == AccountAuth::Proton)
                .map(|account| account.username.clone())
                .collect::<Vec<_>>();
            if !proton_usernames.is_empty() {
                // Fail before deleting rows if a sign-in owns the auth lock.
                crate::proton::runtime::forget_accounts(&proton_usernames)?;
            }
            for account in accounts {
                let refresh_token = if account.auth_type == AccountAuth::OAuth {
                    oauth::account::oauth_refresh_token(&account, secrets)
                } else {
                    None
                };
                accounts::delete(db, secrets, account.id)?;
                if let Some(token) = refresh_token {
                    oauth::account::revoke_account_token(&account, &token);
                }
            }
            if !proton_usernames.is_empty() {
                if let Err(error) = crate::proton::runtime::reconcile() {
                    eprintln!("Proton bridge reconcile failed after space deletion: {error}");
                }
            }
        }
        SpaceDeleteAction::Migrate { target_space_id } => {
            if target_space_id == id {
                return Err(AppError::Message(
                    "accounts cannot migrate to the space being deleted".into(),
                ));
            }
            by_id(db, target_space_id)?;
            for account in accounts::list_for_space(db, id)? {
                accounts::move_to_space(db, account.id, target_space_id)?;
            }
        }
    }
    if let Ok(config) = AiConfig::load(db, id) {
        for base_url in [&config.base_url, &config.embedding_base_url] {
            if !base_url.trim().is_empty() {
                let _ = secrets.delete(&crate::ai::key_name(id, base_url));
            }
        }
    }
    delete_scope(db, id)?;
    list(db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{self, AccountInput, SignatureFields};
    use crate::db::Database;
    use crate::secrets::MemoryStore;

    fn input_in_space(space_id: i64) -> AccountInput {
        AccountInput {
            display_name: "Test".into(),
            email: "me@example.com".into(),
            username: "me@example.com".into(),
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
        }
    }

    #[test]
    fn ensure_default_is_idempotent_and_creates_names_in_order() {
        let db = Database::open_memory().unwrap();
        let first = ensure_default(&db).unwrap();
        let again = ensure_default(&db).unwrap();
        assert_eq!(first.id, again.id);
        assert_eq!((first.name.as_str(), first.icon.as_str()), ("Personal", "user"));
        let work = create(&db, "Work").unwrap();
        assert_eq!(work.position, 1);
        assert_eq!(list(&db).unwrap().len(), 2);
    }

    #[test]
    fn names_are_trimmed_unique_and_length_limited() {
        let db = Database::open_memory().unwrap();
        let work = create(&db, "  Work  ").unwrap();
        assert_eq!(work.name, "Work");
        assert_eq!(
            create(&db, "work").unwrap_err().to_string(),
            "space already exists"
        );
        assert!(create(&db, &"x".repeat(41)).is_err());
        let personal = create_with_icon(&db, "Home", Some("briefcase")).unwrap();
        assert_eq!(personal.icon, "briefcase");
        assert_eq!(rename_with_icon(&db, personal.id, "Studio", Some("home")).unwrap().icon, "home");
        assert!(create_with_icon(&db, "Bad", Some("../../etc")).is_err());
    }

    #[test]
    fn deleting_a_space_with_migration_keeps_accounts_and_mail() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let personal = ensure_default(&db).unwrap();
        let work = create(&db, "Work").unwrap();
        let account = accounts::create(&db, &secrets, input_in_space(work.id)).unwrap();
        delete(
            &db,
            &secrets,
            work.id,
            SpaceDeleteAction::Migrate {
                target_space_id: personal.id,
            },
        )
        .unwrap();
        let moved = accounts::list_for_space(&db, personal.id).unwrap();
        assert_eq!(moved[0].id, account.id);
        assert!(by_id(&db, work.id).is_err());
    }

    #[test]
    fn deleting_the_last_space_is_rejected() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let default = ensure_default(&db).unwrap();
        let error = delete(
            &db,
            &secrets,
            default.id,
            SpaceDeleteAction::DeleteAccounts,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "the last space cannot be deleted");
    }

    #[test]
    fn deleting_a_space_with_accounts_removes_them() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let personal = ensure_default(&db).unwrap();
        let work = create(&db, "Work").unwrap();
        accounts::create(&db, &secrets, input_in_space(work.id)).unwrap();
        delete(&db, &secrets, work.id, SpaceDeleteAction::DeleteAccounts).unwrap();
        assert!(accounts::list_for_space(&db, personal.id).unwrap().is_empty());
        assert_eq!(accounts::list(&db).unwrap().len(), 0);
    }
}
