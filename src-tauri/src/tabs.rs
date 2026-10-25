use std::collections::HashSet;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::db::{now, Database};
use crate::error::{AppError, AppResult};
use crate::spaces;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tab {
    pub id: i64,
    pub space_id: i64,
    pub name: String,
    pub description: String,
    pub position: i64,
    pub removable: bool,
    pub primary: bool,
    pub color: Option<String>,
    pub icon: Option<String>,
}

pub const REVISION_KEY: &str = "tabs.revision";
const NAME_LIMIT: usize = 60;
const DESCRIPTION_LIMIT: usize = 500;
const ICON_LIMIT: usize = 40;
const DEFAULT_ICON: &str = "folder";
const DEFAULT_COLORS: [&str; 6] = [
    "#6da4ff", "#58c98b", "#f2b85b", "#e879b9", "#b489ff", "#f28f6b",
];

const COLUMNS: &str = "id, space_id, name, description, position, removable, is_primary, color, icon";

fn row_to_tab(row: &rusqlite::Row<'_>) -> rusqlite::Result<Tab> {
    Ok(Tab {
        id: row.get(0)?,
        space_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        position: row.get(4)?,
        removable: row.get::<_, i64>(5)? != 0,
        primary: row.get::<_, i64>(6)? != 0,
        color: row.get(7)?,
        icon: row.get(8)?,
    })
}

pub(crate) fn revision_from_conn(conn: &rusqlite::Connection) -> AppResult<i64> {
    let raw: Option<String> = conn
        .query_row(
            "select value from settings where key = ?1",
            params![REVISION_KEY],
            |row| row.get(0),
        )
        .optional()?;
    Ok(raw.and_then(|value| value.parse().ok()).unwrap_or(0))
}

#[cfg(test)]
pub fn revision(db: &Database) -> AppResult<i64> {
    let conn = db.conn.lock().unwrap();
    revision_from_conn(&conn)
}

fn bump_revision(tx: &rusqlite::Transaction<'_>) -> AppResult<()> {
    let next = revision_from_conn(tx)? + 1;
    tx.execute(
        "insert into settings (key, value) values (?1, ?2) on conflict(key) do update set value = excluded.value",
        params![REVISION_KEY, next.to_string()],
    )?;
    Ok(())
}

fn invalidate(tx: &rusqlite::Transaction<'_>, space_id: i64) -> AppResult<()> {
    tx.execute(
        "delete from classifications where message_id in (
            select m.id from messages m
            join accounts a on a.id = m.account_id
            where a.space_id = ?1
         )",
        params![space_id],
    )?;
    bump_revision(tx)
}

pub fn seed_space(db: &Database, space_id: i64) -> AppResult<()> {
    spaces::by_id(db, space_id)?;
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    for (position, (name, description, primary, color, icon)) in
        crate::db::SEED_TABS.iter().enumerate()
    {
        tx.execute(
            "insert or ignore into tabs (space_id, name, description, position, is_primary, color, icon, created_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                space_id,
                name,
                description,
                position as i64,
                *primary as i64,
                color,
                icon,
                now()
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn list(db: &Database, space_id: i64) -> AppResult<Vec<Tab>> {
    let conn = db.conn.lock().unwrap();
    let sql = format!("select {COLUMNS} from tabs where space_id = ?1 order by position, id");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![space_id], row_to_tab)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn snapshot(db: &Database, space_id: i64) -> AppResult<(i64, Vec<Tab>)> {
    let conn = db.conn.lock().unwrap();
    let revision = revision_from_conn(&conn)?;
    let sql = format!("select {COLUMNS} from tabs where space_id = ?1 order by position, id");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![space_id], row_to_tab)?;
    Ok((revision, rows.collect::<Result<Vec<_>, _>>()?))
}

fn by_id(db: &Database, id: i64) -> AppResult<Tab> {
    let conn = db.conn.lock().unwrap();
    let sql = format!("select {COLUMNS} from tabs where id = ?1");
    conn.query_row(&sql, params![id], row_to_tab)
        .optional()?
        .ok_or_else(|| AppError::Message("tab not found".into()))
}

fn clean(name: &str, description: &str) -> AppResult<(String, String)> {
    let name = name.trim().to_string();
    let description = description.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Message("tab name is required".into()));
    }
    if name.eq_ignore_ascii_case("all") {
        return Err(AppError::Message(
            "All is a built-in tab and cannot be recreated".into(),
        ));
    }
    if name.chars().count() > NAME_LIMIT {
        return Err(AppError::Message("tab name is too long".into()));
    }
    if description.is_empty() {
        return Err(AppError::Message("tab description is required".into()));
    }
    if description.chars().count() > DESCRIPTION_LIMIT {
        return Err(AppError::Message("tab description is too long".into()));
    }
    Ok((name, description))
}

fn clean_color(value: Option<&str>) -> AppResult<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let valid = value.len() == 7
        && value.starts_with('#')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit());
    if !valid {
        return Err(AppError::Message("tab color must be a hex color".into()));
    }
    Ok(Some(value.to_ascii_lowercase()))
}

fn clean_icon(value: Option<&str>) -> AppResult<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.chars().count() > ICON_LIMIT {
        return Err(AppError::Message("tab icon is too long".into()));
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(AppError::Message("tab icon is invalid".into()));
    }
    Ok(Some(value.to_string()))
}

pub fn create(
    db: &Database,
    space_id: i64,
    name: &str,
    description: &str,
    color: Option<&str>,
    icon: Option<&str>,
) -> AppResult<Tab> {
    let (name, description) = clean(name, description)?;
    spaces::by_id(db, space_id)?;
    let mut conn = db.conn.lock().unwrap();
    let duplicate: bool = conn.query_row(
        "select exists(select 1 from tabs where space_id = ?1 and lower(name) = lower(?2))",
        params![space_id, name],
        |row| row.get(0),
    )?;
    if duplicate {
        return Err(AppError::Message(
            "a tab with that name already exists".into(),
        ));
    }
    let position: i64 = conn.query_row(
        "select coalesce(max(position), -1) + 1 from tabs where space_id = ?1",
        params![space_id],
        |row| row.get(0),
    )?;
    let color = clean_color(color)?.unwrap_or_else(|| {
        DEFAULT_COLORS[(position.max(0) as usize) % DEFAULT_COLORS.len()].to_string()
    });
    let icon = clean_icon(icon)?.unwrap_or_else(|| DEFAULT_ICON.to_string());
    let tx = conn.transaction()?;
    tx.execute(
        "insert into tabs (space_id, name, description, position, removable, is_primary, color, icon, created_at) values (?1, ?2, ?3, ?4, 1, 0, ?5, ?6, ?7)",
        params![space_id, name, description, position, color, icon, now()],
    )?;
    let id = tx.last_insert_rowid();
    invalidate(&tx, space_id)?;
    tx.commit()?;
    drop(conn);
    by_id(db, id)
}

pub fn update(
    db: &Database,
    space_id: i64,
    id: i64,
    name: &str,
    description: &str,
    color: Option<&str>,
    icon: Option<&str>,
) -> AppResult<Tab> {
    let (name, description) = clean(name, description)?;
    let mut conn = db.conn.lock().unwrap();
    let existing: Option<(String, Option<String>, Option<String>)> = conn
        .query_row(
            "select description, color, icon from tabs where id = ?1 and space_id = ?2",
            params![id, space_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((old_description, old_color, old_icon)) = existing else {
        return Err(AppError::Message("tab not found".into()));
    };
    let duplicate: bool = conn.query_row(
        "select exists(select 1 from tabs where space_id = ?3 and lower(name) = lower(?1) and id <> ?2)",
        params![name, id, space_id],
        |row| row.get(0),
    )?;
    if duplicate {
        return Err(AppError::Message(
            "a tab with that name already exists".into(),
        ));
    }
    let color = clean_color(color)?.or(old_color);
    let icon = clean_icon(icon)?.or(old_icon);
    let definition_changed = old_description != description;
    let tx = conn.transaction()?;
    tx.execute(
        "update tabs set name = ?2, description = ?3, color = ?4, icon = ?5 where id = ?1 and space_id = ?6",
        params![id, name, description, color, icon, space_id],
    )?;
    if definition_changed {
        invalidate(&tx, space_id)?;
    }
    tx.commit()?;
    drop(conn);
    by_id(db, id)
}

pub fn delete(db: &Database, space_id: i64, id: i64) -> AppResult<()> {
    let mut conn = db.conn.lock().unwrap();
    let tab: Option<(bool, bool)> = conn
        .query_row(
            "select removable, is_primary from tabs where id = ?1 and space_id = ?2",
            params![id, space_id],
            |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0)),
        )
        .optional()?;
    let primary = match tab {
        None => return Err(AppError::Message("tab not found".into())),
        Some((false, _)) => {
            return Err(AppError::Message("this tab cannot be removed".into()));
        }
        Some((true, primary)) => primary,
    };
    if primary {
        let total: i64 = conn.query_row(
            "select count(*) from tabs where space_id = ?1",
            params![space_id],
            |row| row.get(0),
        )?;
        if total <= 1 {
            return Err(AppError::Message("at least one tab is required".into()));
        }
    }
    let tx = conn.transaction()?;
    invalidate(&tx, space_id)?;
    tx.execute(
        "delete from tabs where id = ?1 and space_id = ?2",
        params![id, space_id],
    )?;
    if primary {
        tx.execute(
            "update tabs set is_primary = 1 where space_id = ?1 and id = (select id from tabs where space_id = ?1 order by position, id limit 1)",
            params![space_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn reorder(db: &Database, space_id: i64, ids: &[i64]) -> AppResult<Vec<Tab>> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    let existing: Vec<i64> = {
        let mut stmt = tx.prepare("select id from tabs where space_id = ?1")?;
        let rows = stmt
            .query_map(params![space_id], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        rows
    };
    let unique: HashSet<i64> = ids.iter().copied().collect();
    if unique.len() != ids.len() || unique.len() != existing.len() {
        return Err(AppError::Message(
            "reorder requires every tab exactly once".into(),
        ));
    }
    if !existing.iter().all(|id| unique.contains(id)) {
        return Err(AppError::Message(
            "reorder requires every tab exactly once".into(),
        ));
    }
    for (position, id) in ids.iter().enumerate() {
        tx.execute(
            "update tabs set position = ?2 where id = ?1 and space_id = ?3",
            params![id, position as i64, space_id],
        )?;
    }
    bump_revision(&tx)?;
    tx.commit()?;
    drop(conn);
    list(db, space_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use rusqlite::params;

    fn space(db: &Database) -> i64 {
        spaces::default_id(db).unwrap()
    }

    fn seed_message(db: &Database) -> i64 {
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
        conn.execute(
            "insert into messages (account_id, folder_id, uid, subject) values (?1, ?2, 1, 'Hello')",
            params![account_id, folder_id],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn seed_tab(db: &Database, name: &str) -> i64 {
        let space_id = spaces::default_id(db).unwrap();
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "insert into tabs (space_id, name, description, position, created_at) values (?1, ?2, 'Test tab', 10, 0)",
            params![space_id, name],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn seed_classification(db: &Database, message_id: i64, tab_id: i64) {
        db.conn
            .lock()
            .unwrap()
            .execute(
                "insert into classifications (message_id, tab_id, confidence, model, classified_at) values (?1, ?2, 1.0, 'test', 0)",
                params![message_id, tab_id],
            )
            .unwrap();
    }

    #[test]
    fn tabs_are_isolated_by_space() {
        let db = Database::open_memory().unwrap();
        let default = spaces::ensure_default(&db).unwrap();
        let work = spaces::create(&db, "Work").unwrap();
        let default_tabs = list(&db, default.id).unwrap();
        let work_tabs = list(&db, work.id).unwrap();
        assert!(default_tabs.iter().any(|tab| tab.name == "Primary"));
        assert!(work_tabs.iter().any(|tab| tab.name == "Primary"));
        let custom = create(&db, work.id, "Clients", "Client mail", None, None).unwrap();
        assert!(list(&db, default.id)
            .unwrap()
            .iter()
            .all(|tab| tab.id != custom.id));
        assert_eq!(list(&db, work.id).unwrap().len(), work_tabs.len() + 1);
        assert!(delete(&db, default.id, custom.id).is_err());
    }

    #[test]
    fn lists_seeded_tabs_in_position_order() {
        let db = Database::open_memory().unwrap();
        let tabs = list(&db, space(&db)).unwrap();
        let names: Vec<&str> = tabs.iter().map(|tab| tab.name.as_str()).collect();
        assert_eq!(names, ["Primary", "Updates", "Promotions", "Social"]);
        assert!(tabs.iter().all(|tab| tab.removable));
        assert!(tabs[0].primary);
        assert!(tabs[1..].iter().all(|tab| !tab.primary));
        assert_eq!(tabs[0].color.as_deref(), Some("#6da4ff"));
        assert_eq!(tabs[0].icon.as_deref(), Some("inbox"));
        assert_eq!(tabs[1].icon.as_deref(), Some("bell"));
    }

    #[test]
    fn deletes_a_removable_tab_without_touching_its_virtual_all_tab() {
        let db = Database::open_memory().unwrap();
        assert_eq!(list(&db, space(&db)).unwrap().len(), 4);
        assert!(delete(&db, space(&db), 1).is_ok());
        assert_eq!(list(&db, space(&db)).unwrap().len(), 3);
    }

    #[test]
    fn deleting_the_primary_tab_promotes_the_next_tab() {
        let db = Database::open_memory().unwrap();
        let primary = list(&db, space(&db)).unwrap()[0].id;
        delete(&db, space(&db), primary).unwrap();
        let tabs = list(&db, space(&db)).unwrap();
        assert!(!tabs.is_empty());
        assert_eq!(tabs.iter().filter(|tab| tab.primary).count(), 1);
        assert_eq!(tabs[0].name, "Updates");
        assert!(tabs[0].primary);
    }

    #[test]
    fn the_last_tab_cannot_be_deleted() {
        let db = Database::open_memory().unwrap();
        let ids: Vec<i64> = list(&db, space(&db)).unwrap().into_iter().map(|tab| tab.id).collect();
        for id in ids.iter().take(ids.len() - 1) {
            delete(&db, space(&db), *id).unwrap();
        }
        assert!(delete(&db, space(&db), *ids.last().unwrap()).is_err());
        assert_eq!(list(&db, space(&db)).unwrap().len(), 1);
    }

    #[test]
    fn deleting_an_unknown_tab_is_an_error() {
        let db = Database::open_memory().unwrap();
        assert!(delete(&db, space(&db), 99).is_err());
        assert_eq!(list(&db, space(&db)).unwrap().len(), 4);
    }

    #[test]
    fn changing_tabs_invalidates_classifications() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        let tab_id = seed_tab(&db, "Projects");
        seed_classification(&db, message_id, tab_id);
        delete(&db, space(&db), tab_id).unwrap();
        let count: i64 = db
            .conn
            .lock()
            .unwrap()
            .query_row("select count(*) from classifications", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn create_appends_trimmed_tabs_and_rejects_bad_input() {
        let db = Database::open_memory().unwrap();
        let tab = create(
            &db,
            space(&db),
            "  Projects  ",
            "  Work threads  ",
            Some("#b489ff"),
            Some("briefcase"),
        )
        .unwrap();
        assert_eq!(tab.name, "Projects");
        assert_eq!(tab.description, "Work threads");
        assert_eq!(tab.position, 4);
        assert_eq!(tab.color.as_deref(), Some("#b489ff"));
        assert_eq!(tab.icon.as_deref(), Some("briefcase"));
        assert!(tab.removable);
        assert!(!tab.primary);
        let defaulted = create(&db, space(&db), "Receipts", "Receipts.", None, None).unwrap();
        assert!(defaulted.color.is_some());
        assert_eq!(defaulted.icon.as_deref(), Some("folder"));
        assert!(create(&db, space(&db), "projects", "Duplicate", None, None).is_err());
        assert!(create(&db, space(&db), "All", "Reserved", None, None).is_err());
        assert!(create(&db, space(&db), "", "No name", None, None).is_err());
        assert!(create(&db, space(&db), "Empty", "   ", None, None).is_err());
        assert!(create(&db, space(&db), &"n".repeat(61), "Too long", None, None).is_err());
        assert!(create(&db, space(&db), "Long description", &"d".repeat(501), None, None).is_err());
        assert!(create(&db, space(&db), "Bad colour", "Bad.", Some("red"), None).is_err());
    }

    #[test]
    fn create_invalidates_cached_classifications() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        seed_classification(&db, message_id, 1);
        let before = revision(&db).unwrap();
        create(&db, space(&db), "Projects", "Project threads.", None, None).unwrap();
        let count: i64 = db
            .conn
            .lock()
            .unwrap()
            .query_row("select count(*) from classifications", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert!(revision(&db).unwrap() > before);
    }

    #[test]
    fn update_changes_the_definition_and_clears_cached_classifications() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        seed_classification(&db, message_id, 1);
        let tab = update(
            &db,
            space(&db),
            1,
            "Personal",
            "Family and friends.",
            Some("#e879b9"),
            Some("users"),
        )
        .unwrap();
        assert_eq!(tab.name, "Personal");
        assert_eq!(tab.description, "Family and friends.");
        assert_eq!(tab.color.as_deref(), Some("#e879b9"));
        assert_eq!(tab.icon.as_deref(), Some("users"));
        let count: i64 = db
            .conn
            .lock()
            .unwrap()
            .query_row("select count(*) from classifications", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert!(update(&db, space(&db), 99, "Missing", "Missing tab", None, None).is_err());
    }

    #[test]
    fn name_and_appearance_changes_preserve_classifications() {
        let db = Database::open_memory().unwrap();
        let message_id = seed_message(&db);
        seed_classification(&db, message_id, 1);
        let before = list(&db, space(&db)).unwrap()[0].clone();
        let updated = update(
            &db,
            space(&db),
            before.id,
            "Personal",
            &before.description,
            Some("#e879b9"),
            Some("users"),
        )
        .unwrap();
        assert_eq!(updated.name, "Personal");
        let count: i64 = db
            .conn
            .lock()
            .unwrap()
            .query_row("select count(*) from classifications", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn rejects_invalid_colours_on_update() {
        let db = Database::open_memory().unwrap();
        let tab = list(&db, space(&db)).unwrap()[0].clone();
        assert!(update(&db, space(&db), tab.id, &tab.name, &tab.description, Some("red"), None).is_err());
    }

    #[test]
    fn reorder_requires_a_complete_unique_id_list() {
        let db = Database::open_memory().unwrap();
        let ids: Vec<i64> = list(&db, space(&db)).unwrap().into_iter().map(|tab| tab.id).collect();
        let reversed: Vec<i64> = ids.iter().rev().copied().collect();
        reorder(&db, space(&db), &reversed).unwrap();
        let names: Vec<String> = list(&db, space(&db)).unwrap().into_iter().map(|tab| tab.name).collect();
        assert_eq!(names, ["Social", "Promotions", "Updates", "Primary"]);
        assert!(reorder(&db, space(&db), &ids[..3]).is_err());
        assert!(reorder(&db, space(&db), &[ids[0], ids[0], ids[2], ids[3]]).is_err());
    }
}
