pub mod carddav;
pub mod vcard;

use std::collections::{HashMap, HashSet};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{self, Database};
use crate::error::{AppError, AppResult};


#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactRecord {
    pub id: i64,
    pub address_book_id: i64,
    pub address_book_name: String,
    pub display_name: String,
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    pub organization: String,
    pub title: String,
    pub notes: String,
    pub photo: Option<String>,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactAccount {
    pub id: i64,
    pub display_name: String,
    pub base_url: String,
    pub username: String,
    pub space_id: i64,
    pub position: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactInput {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub emails: Vec<String>,
    #[serde(default)]
    pub phones: Vec<String>,
    #[serde(default)]
    pub organization: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub address_book_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactAddressBook {
    pub id: i64,
    pub contact_account_id: i64,
    pub href: String,
    pub name: String,
    pub color: Option<String>,
    pub read_only: bool,
}

/// Ensure the built-in local address book exists for a space. Idempotent and
/// also used as the runtime counterpart of the version-40 backfill.
pub fn ensure_local_space(db: &Database, space_id: i64) -> AppResult<()> {
    crate::spaces::by_id(db, space_id)?;
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "insert into contact_accounts
            (display_name, base_url, username, space_id, position, kind, created_at)
         select 'Local', '', '', ?1, 1000000, 'local', ?2
         where not exists (
           select 1 from contact_accounts where space_id = ?1 and kind = 'local'
         )",
        params![space_id, db::now()],
    )?;
    conn.execute(
        "insert into address_books (contact_account_id, href, name)
         select ca.id, 'local', 'Personal'
         from contact_accounts ca
         where ca.space_id = ?1 and ca.kind = 'local'
           and not exists (
             select 1 from address_books b
             where b.contact_account_id = ca.id and b.href = 'local'
           )",
        params![space_id],
    )?;
    Ok(())
}

pub fn local_book_id(db: &Database, space_id: i64) -> AppResult<i64> {
    db.read()
        .query_row(
            "select b.id from address_books b
             join contact_accounts ca on ca.id = b.contact_account_id
             where ca.space_id = ?1 and ca.kind = 'local' and b.href = 'local'",
            params![space_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::Message("local address book not found".into()))
}

pub fn list_accounts(db: &Database, space_id: i64) -> AppResult<Vec<ContactAccount>> {
    let conn = db.read();
    let mut stmt = conn.prepare(
        "select id, display_name, base_url, username, space_id, position, created_at
         from contact_accounts
         where space_id = ?1 and kind = 'carddav'
         order by position, id",
    )?;
    let rows = stmt.query_map(params![space_id], |row| {
        Ok(ContactAccount {
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

pub fn list_address_books(db: &Database, space_id: i64) -> AppResult<Vec<ContactAddressBook>> {
    let conn = db.read();
    let mut stmt = conn.prepare(
        "select b.id, b.contact_account_id, b.href, b.name, b.color, b.read_only
         from address_books b
         join contact_accounts ca on ca.id = b.contact_account_id
         where ca.space_id = ?1
         order by case when ca.kind = 'carddav' then 0 else 1 end,
                  ca.position, b.name collate nocase, b.id",
    )?;
    let rows = stmt.query_map(params![space_id], |row| {
        Ok(ContactAddressBook {
            id: row.get(0)?,
            contact_account_id: row.get(1)?,
            href: row.get(2)?,
            name: row.get(3)?,
            color: row.get(4)?,
            read_only: row.get::<_, i64>(5)? != 0,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Contact {
    pub name: String,
    pub email: String,
}

fn parse_recipient(value: &str) -> Option<(String, String)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let (Some(start), Some(end)) = (trimmed.rfind('<'), trimmed.rfind('>')) {
        if end > start {
            let email = trimmed[start + 1..end].trim().to_ascii_lowercase();
            let name = trimmed[..start].trim().trim_matches('"').trim().to_string();
            if email.contains('@') {
                return Some((email, name));
            }
        }
    }
    let email = trimmed.trim_matches('"').trim().to_ascii_lowercase();
    email.contains('@').then(|| (email, String::new()))
}

pub fn list_contacts(
    db: &Database,
    query: Option<&str>,
    limit: Option<usize>,
) -> AppResult<Vec<Contact>> {
    let own: HashSet<String> = {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare("select lower(email), lower(username) from accounts")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut values = HashSet::new();
        for row in rows {
            let (email, username) = row?;
            values.insert(email);
            values.insert(username);
        }
        values
    };

    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "select from_addr, from_name, to_addrs, cc_addrs from messages order by date desc limit 2000",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    let mut contacts: HashMap<String, (String, usize)> = HashMap::new();
    for row in rows {
        let (from_addr, from_name, to_addrs, cc_addrs) = row?;
        let mut add = |value: &str, fallback_name: &str| {
            if let Some((email, name)) = parse_recipient(value) {
                if own.contains(&email) {
                    return;
                }
                let display = if name.is_empty() {
                    fallback_name.trim().to_string()
                } else {
                    name
                };
                let entry = contacts.entry(email).or_insert((String::new(), 0));
                entry.1 += 1;
                if entry.0.is_empty() && !display.is_empty() {
                    entry.0 = display;
                }
            }
        };
        add(&from_addr, &from_name);
        for value in serde_json::from_str::<Vec<String>>(&to_addrs).unwrap_or_default() {
            add(&value, "");
        }
        for value in serde_json::from_str::<Vec<String>>(&cc_addrs).unwrap_or_default() {
            add(&value, "");
        }
    }

    let needle = query.unwrap_or("").trim().to_ascii_lowercase();
    let mut ranked: Vec<(Contact, usize)> = contacts
        .into_iter()
        .filter(|(email, (name, _))| {
            needle.is_empty()
                || email.contains(&needle)
                || name.to_ascii_lowercase().contains(&needle)
        })
        .map(|(email, (name, count))| (Contact { name, email }, count))
        .collect();

    ranked.sort_by(|(left, left_count), (right, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left.email.cmp(&right.email))
    });
    ranked.truncate(limit.unwrap_or(8).min(50));
    Ok(ranked.into_iter().map(|(contact, _)| contact).collect())
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContactRecord> {
    Ok(ContactRecord {
        id: row.get(0)?,
        address_book_id: row.get(1)?,
        address_book_name: row.get(2)?,
        display_name: row.get(3)?,
        emails: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
        phones: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
        organization: row.get(6)?,
        title: row.get(7)?,
        notes: row.get(8)?,
        photo: row.get(9)?,
        read_only: row.get::<_, i64>(10)? != 0,
    })
}

const RECORD_SELECT: &str = "select c.id, c.address_book_id, b.name, c.display_name,
        c.emails, c.phones, c.organization, c.title, c.notes, c.photo, b.read_only
     from contacts c
     join address_books b on b.id = c.address_book_id
     join contact_accounts ca on ca.id = b.contact_account_id
     where ca.space_id = ?1";

pub fn list_records(
    db: &Database,
    space_id: i64,
    address_book_id: Option<i64>,
    query: Option<&str>,
    limit: Option<usize>,
) -> AppResult<Vec<ContactRecord>> {
    ensure_local_space(db, space_id)?;
    let needle = query.unwrap_or("").trim().to_ascii_lowercase();
    let limit = limit.unwrap_or(500).clamp(1, 5000);
    let conn = db.read();
    let mut sql = RECORD_SELECT.to_string();
    if address_book_id.is_some() {
        sql.push_str(" and c.address_book_id = ?2");
    }
    if !needle.is_empty() {
        sql.push_str(if address_book_id.is_some() {
            " and (lower(c.display_name) like ?3 or lower(c.emails) like ?3)"
        } else {
            " and (lower(c.display_name) like ?2 or lower(c.emails) like ?2)"
        });
    }
    sql.push_str(" order by c.display_name collate nocase, c.id limit ");
    sql.push_str(&limit.to_string());
    let pattern = format!("%{needle}%");
    let mut stmt = conn.prepare(&sql)?;
    let rows = match (address_book_id, needle.is_empty()) {
        (Some(book), false) => stmt.query_map(params![space_id, book, pattern], row_to_record)?,
        (Some(book), true) => stmt.query_map(params![space_id, book], row_to_record)?,
        (None, false) => stmt.query_map(params![space_id, pattern], row_to_record)?,
        (None, true) => stmt.query_map(params![space_id], row_to_record)?,
    };
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get_record(db: &Database, id: i64) -> AppResult<ContactRecord> {
    db.read()
        .query_row(
            "select c.id, c.address_book_id, b.name, c.display_name,
                    c.emails, c.phones, c.organization, c.title, c.notes, c.photo, b.read_only
             from contacts c
             join address_books b on b.id = c.address_book_id
             where c.id = ?1",
            params![id],
            row_to_record,
        )
        .optional()?
        .ok_or_else(|| AppError::Message("contact not found".into()))
}

pub fn record_for_email(
    db: &Database,
    space_id: i64,
    email: &str,
) -> AppResult<Option<ContactRecord>> {
    let email = email.trim().to_ascii_lowercase();
    if email.is_empty() {
        return Ok(None);
    }
    let records = list_records(db, space_id, None, None, Some(5000))?;
    Ok(records.into_iter().find(|record| {
        record
            .emails
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&email))
    }))
}

fn validate(input: &ContactInput) -> AppResult<()> {
    if input.display_name.trim().is_empty() && input.emails.iter().all(|email| email.trim().is_empty()) {
        return Err(AppError::Message("a contact needs a name or an email".into()));
    }
    for email in input.emails.iter().filter(|email| !email.trim().is_empty()) {
        if !email.trim().contains('@') {
            return Err(AppError::Message(format!("invalid email address: {email}")));
        }
    }
    Ok(())
}

pub fn create_local(db: &Database, book_id: i64, input: ContactInput) -> AppResult<ContactRecord> {
    validate(&input)?;
    let uid = format!("{}-{}", db::now(), rand::random::<u64>());
    let vcard = vcard::build(&uid, &input);
    let emails = serde_json::to_string(
        &input
            .emails
            .iter()
            .map(|email| email.trim().to_string())
            .filter(|email| !email.is_empty())
            .collect::<Vec<_>>(),
    )?;
    let phones = serde_json::to_string(
        &input
            .phones
            .iter()
            .map(|phone| phone.trim().to_string())
            .filter(|phone| !phone.is_empty())
            .collect::<Vec<_>>(),
    )?;
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "insert into contacts
            (address_book_id, href, etag, uid, display_name, emails, phones,
             organization, title, notes, photo, vcard, updated_at)
         values (?1, ?2, null, ?3, ?4, ?5, ?6, ?7, ?8, ?9, null, ?10, ?11)",
        params![
            book_id,
            format!("{uid}.vcf"),
            uid,
            input.display_name.trim(),
            emails,
            phones,
            input.organization.trim(),
            input.title.trim(),
            input.notes.trim(),
            vcard,
            db::now(),
        ],
    )?;
    let id = conn.last_insert_rowid();
    drop(conn);
    get_record(db, id)
}

pub fn update_local(db: &Database, id: i64, input: ContactInput) -> AppResult<ContactRecord> {
    validate(&input)?;
    let existing = get_record(db, id)?;
    if existing.read_only {
        return Err(AppError::Message("this address book is read-only".into()));
    }
    let uid: String = db
        .read()
        .query_row("select uid from contacts where id = ?1", params![id], |row| row.get(0))
        .optional()?
        .ok_or_else(|| AppError::Message("contact not found".into()))?;
    let vcard = vcard::build(&uid, &input);
    let emails = serde_json::to_string(
        &input
            .emails
            .iter()
            .map(|email| email.trim().to_string())
            .filter(|email| !email.is_empty())
            .collect::<Vec<_>>(),
    )?;
    let phones = serde_json::to_string(
        &input
            .phones
            .iter()
            .map(|phone| phone.trim().to_string())
            .filter(|phone| !phone.is_empty())
            .collect::<Vec<_>>(),
    )?;
    db.conn.lock().unwrap().execute(
        "update contacts set display_name = ?1, emails = ?2, phones = ?3,
            organization = ?4, title = ?5, notes = ?6, vcard = ?7, updated_at = ?8
         where id = ?9",
        params![
            input.display_name.trim(),
            emails,
            phones,
            input.organization.trim(),
            input.title.trim(),
            input.notes.trim(),
            vcard,
            db::now(),
            id,
        ],
    )?;
    get_record(db, id)
}

pub fn delete_local(db: &Database, id: i64) -> AppResult<()> {
    let existing = get_record(db, id)?;
    if existing.read_only {
        return Err(AppError::Message("this address book is read-only".into()));
    }
    db.conn
        .lock()
        .unwrap()
        .execute("delete from contacts where id = ?1", params![id])?;
    Ok(())
}

fn book_kind(db: &Database, book_id: i64) -> AppResult<String> {
    db.read()
        .query_row(
            "select ca.kind from address_books b
             join contact_accounts ca on ca.id = b.contact_account_id
             where b.id = ?1",
            params![book_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::Message("address book not found".into()))
}

fn contact_kind(db: &Database, contact_id: i64) -> AppResult<String> {
    db.read()
        .query_row(
            "select ca.kind from contacts c
             join address_books b on b.id = c.address_book_id
             join contact_accounts ca on ca.id = b.contact_account_id
             where c.id = ?1",
            params![contact_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::Message("contact not found".into()))
}

pub async fn create_contact(
    db: &Database,
    _secrets: &dyn crate::secrets::SecretStore,
    input: ContactInput,
) -> AppResult<ContactRecord> {
    let book = input
        .address_book_id
        .ok_or_else(|| AppError::Message("address book is required".into()))?;
    if book_kind(db, book)? != "local" {
        return Err(AppError::Message(
            "CardDAV contact writing is not available yet".into(),
        ));
    }
    create_local(db, book, input)
}

pub async fn update_contact(
    db: &Database,
    _secrets: &dyn crate::secrets::SecretStore,
    id: i64,
    input: ContactInput,
) -> AppResult<ContactRecord> {
    if contact_kind(db, id)? != "local" {
        return Err(AppError::Message(
            "CardDAV contact writing is not available yet".into(),
        ));
    }
    update_local(db, id, input)
}

pub async fn delete_contact(
    db: &Database,
    _secrets: &dyn crate::secrets::SecretStore,
    id: i64,
) -> AppResult<()> {
    if contact_kind(db, id)? != "local" {
        return Err(AppError::Message(
            "CardDAV contact writing is not available yet".into(),
        ));
    }
    delete_local(db, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(db: &Database) {
        let conn = db.conn.lock().unwrap();
        conn.execute_batch(
            "insert into accounts (id, display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, sync_interval_secs, created_at) values (1, 'Me', 'me@example.com', 'me@example.com', 'imap', 993, 'tls', 'smtp', 465, 'tls', 120, 0);
             insert into folders (id, account_id, path, name, role) values (1, 1, 'INBOX', 'Inbox', 'inbox');
             insert into messages (account_id, folder_id, uid, from_addr, from_name, to_addrs, cc_addrs, subject, date) values
               (1, 1, 1, 'ada@example.com', 'Ada Lovelace', '[\"me@example.com\"]', '[]', 'One', 300),
               (1, 1, 2, 'grace@example.com', 'Grace Hopper', '[\"me@example.com\", \"ada@example.com\"]', '[]', 'Two', 200),
               (1, 1, 3, 'news@example.com', '', '[\"Grace Hopper <grace@example.com>\"]', '[\"team@example.com\"]', 'Three', 100),
               (1, 1, 4, 'me@example.com', 'Me', '[\"ada@example.com\"]', '[]', 'Four', 50);",
        )
        .unwrap();
    }

    #[test]
    fn local_contacts_round_trip_with_multiple_emails_and_phones() {
        let db = Database::open_memory().unwrap();
        let space = crate::spaces::ensure_default(&db).unwrap();
        ensure_local_space(&db, space.id).unwrap();
        let book = local_book_id(&db, space.id).unwrap();

        let created = create_local(
            &db,
            book,
            ContactInput {
                display_name: "Ada Lovelace".into(),
                emails: vec!["ada@example.com".into(), "ada@work.example".into()],
                phones: vec!["+44 1".into()],
                organization: "Analytical".into(),
                title: "Engineer".into(),
                notes: "First programmer".into(),
                address_book_id: None,
            },
        )
        .unwrap();
        assert_eq!(created.emails.len(), 2);

        let listed = list_records(&db, space.id, Some(book), Some("ada"), None).unwrap();
        assert_eq!(listed.len(), 1);

        let updated = update_local(
            &db,
            created.id,
            ContactInput {
                display_name: "Ada King".into(),
                emails: vec!["ada@example.com".into()],
                phones: vec![],
                organization: String::new(),
                title: String::new(),
                notes: String::new(),
                address_book_id: None,
            },
        )
        .unwrap();
        assert_eq!(updated.display_name, "Ada King");
        assert_eq!(updated.emails, vec!["ada@example.com"]);

        assert!(record_for_email(&db, space.id, "ADA@EXAMPLE.COM")
            .unwrap()
            .is_some());
        delete_local(&db, created.id).unwrap();
        assert!(list_records(&db, space.id, None, None, None)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn every_space_gets_one_local_address_book() {
        let db = Database::open_memory().unwrap();
        let space = crate::spaces::ensure_default(&db).unwrap();
        ensure_local_space(&db, space.id).unwrap();
        ensure_local_space(&db, space.id).unwrap();

        assert!(list_accounts(&db, space.id).unwrap().is_empty());
        let books = list_address_books(&db, space.id).unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].name, "Personal");
        assert_eq!(books[0].id, local_book_id(&db, space.id).unwrap());
    }

    #[test]
    fn ranks_people_by_frequency_and_excludes_own_addresses() {
        let db = Database::open_memory().unwrap();
        seed(&db);
        let contacts = list_contacts(&db, None, Some(10)).unwrap();
        let emails: Vec<&str> = contacts
            .iter()
            .map(|contact| contact.email.as_str())
            .collect();
        assert_eq!(emails[0], "ada@example.com");
        assert!(emails.contains(&"grace@example.com"));
        assert!(emails.contains(&"team@example.com"));
        assert!(!emails.contains(&"me@example.com"));
    }

    #[test]
    fn filters_by_name_or_email_and_honors_limit() {
        let db = Database::open_memory().unwrap();
        seed(&db);
        assert_eq!(
            list_contacts(&db, Some("lovelace"), None)
                .unwrap()
                .first()
                .map(|contact| contact.email.as_str()),
            Some("ada@example.com")
        );
        assert_eq!(
            list_contacts(&db, Some("example.com"), Some(1))
                .unwrap()
                .len(),
            1
        );
    }
}
