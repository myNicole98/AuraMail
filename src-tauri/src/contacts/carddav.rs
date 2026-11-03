use std::sync::Once;
use std::time::Duration;

use bytes::Bytes;
use fast_dav_rs::{etag_from_headers, CardDavClient};
use rusqlite::{params, OptionalExtension};

use crate::contacts::{ContactAccount, ContactAccountInput};
use crate::db::{self, Database};
use crate::error::{AppError, AppResult};
use crate::secrets::{contact_account_key, SecretStore};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBook {
    pub href: String,
    pub name: String,
    pub color: Option<String>,
}

fn install_crypto_provider() {
    static INSTALL_PROVIDER: Once = Once::new();
    INSTALL_PROVIDER.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

fn clean_base_url(value: &str) -> AppResult<String> {
    let url = value.trim().trim_end_matches('/');
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(AppError::Message(
            "address book URL must start with https:// or http://".into(),
        ));
    }
    if url.chars().any(char::is_whitespace) {
        return Err(AppError::Message("address book URL is invalid".into()));
    }
    Ok(url.to_string())
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
        format!("{origin}/.well-known/carddav"),
        format!("{base}/"),
    ];
    candidates.dedup();
    candidates
}

fn client_for_base(
    base_url: &str,
    username: &str,
    password: &str,
) -> AppResult<CardDavClient> {
    install_crypto_provider();
    CardDavClient::builder(base_url)
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .max_retries(2)
        .user_agent(format!("AuraMail/{}", env!("CARGO_PKG_VERSION")))
        .require_https(base_url.starts_with("https://"))
        .basic_auth(username, password)
        .build()
        .map_err(dav_error)
}

fn dav_error(error: fast_dav_rs::Error) -> AppError {
    AppError::Message(error.to_string())
}

async fn discover_books(client: &CardDavClient) -> AppResult<Vec<RemoteBook>> {
    let principal = client
        .discover_current_user_principal()
        .await
        .map_err(dav_error)?
        .ok_or_else(|| AppError::Message("address book server did not return a principal".into()))?;
    let homes = client
        .discover_addressbook_home_set(&principal)
        .await
        .map_err(dav_error)?;
    let home = homes
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Message("address book home set not found".into()))?;
    let books = client
        .list_addressbooks(&home)
        .await
        .map_err(dav_error)?;
    Ok(books
        .into_iter()
        .map(|book| RemoteBook {
            href: book.href,
            name: book
                .displayname
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "Contacts".into()),
            color: book.color,
        })
        .collect())
}

pub async fn add_account(
    db: &Database,
    secrets: &dyn SecretStore,
    input: ContactAccountInput,
) -> AppResult<ContactAccount> {
    let base_url = clean_base_url(&input.base_url)?;
    let username = input.username.trim();
    if username.is_empty() {
        return Err(AppError::Message("address book username is required".into()));
    }
    if input.password.is_empty() {
        return Err(AppError::Message("address book password is required".into()));
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
        match discover_books(&client).await {
            Ok(books) => {
                discovered = Some((candidate, books));
                break;
            }
            Err(error) => errors.push(error.to_string()),
        }
    }
    let (base_url, remote) = discovered.ok_or_else(|| {
        if errors.iter().any(|error| error.contains("401")) {
            AppError::Message(
                "address book authentication failed (401 Unauthorized). Use an \
                 app-specific password/token if your provider requires one."
                    .into(),
            )
        } else {
            AppError::Message(format!(
                "address book discovery failed: {}",
                errors.join("; ")
            ))
        }
    })?;

    let created_at = db::now();
    let (id, position) = {
        let conn = db.conn.lock().unwrap();
        let position: i64 = conn.query_row(
            "select coalesce(max(position) + 1, 0) from contact_accounts
             where space_id = ?1 and kind = 'carddav'",
            params![input.space_id],
            |row| row.get(0),
        )?;
        conn.execute(
            "insert into contact_accounts
                (display_name, base_url, username, space_id, position, kind, created_at)
             values (?1, ?2, ?3, ?4, ?5, 'carddav', ?6)",
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

    if let Err(error) = secrets.set(&contact_account_key(id), &input.password) {
        let _ = db
            .conn
            .lock()
            .unwrap()
            .execute("delete from contact_accounts where id = ?1", params![id]);
        return Err(error);
    }
    let account = ContactAccount {
        id,
        display_name,
        base_url,
        username: username.to_string(),
        space_id: input.space_id,
        position,
        created_at,
    };
    if let Err(error) = save_books(db, id, &remote) {
        let _ = secrets.delete(&contact_account_key(id));
        let _ = db
            .conn
            .lock()
            .unwrap()
            .execute("delete from contact_accounts where id = ?1", params![id]);
        return Err(error);
    }
    Ok(account)
}

pub fn save_books(db: &Database, account_id: i64, books: &[RemoteBook]) -> AppResult<()> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    for book in books {
        tx.execute(
            "insert into address_books
                (contact_account_id, href, name, color)
             values (?1, ?2, ?3, ?4)
             on conflict(contact_account_id, href) do update set
                name = excluded.name,
                color = excluded.color",
            params![account_id, book.href, book.name, book.color],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub struct WriteTarget {
    pub href: String,
    pub etag: Option<String>,
    pub book_id: i64,
    pub read_only: bool,
    pub account_id: i64,
    pub base_url: String,
    pub username: String,
    pub uid: String,
}

fn write_error(status: u16) -> AppError {
    AppError::Message(match status {
        401 => "The address book server rejected the credentials.".into(),
        403 => "This address book is read-only on the server.".into(),
        404 => "Contact not found on the server.".into(),
        412 => "This contact changed on the server. Review it and try again.".into(),
        other => format!("The contact server rejected the change ({other})."),
    })
}

pub fn mark_read_only(db: &Database, book_id: i64) -> AppResult<()> {
    db.conn.lock().unwrap().execute(
        "update address_books set read_only = 1 where id = ?1",
        params![book_id],
    )?;
    Ok(())
}

fn member_path(book_href: &str, file: &str) -> String {
    if book_href.ends_with('/') {
        format!("{book_href}{file}")
    } else {
        format!("{book_href}/{file}")
    }
}

fn book_target(db: &Database, book_id: i64) -> AppResult<WriteTarget> {
    db.read()
        .query_row(
            "select b.href, b.read_only, ca.id, ca.base_url, ca.username
             from address_books b
             join contact_accounts ca on ca.id = b.contact_account_id
             where b.id = ?1 and ca.kind = 'carddav'",
            params![book_id],
            |row| {
                Ok(WriteTarget {
                    href: row.get(0)?,
                    etag: None,
                    book_id,
                    read_only: row.get::<_, i64>(1)? != 0,
                    account_id: row.get(2)?,
                    base_url: row.get(3)?,
                    username: row.get(4)?,
                    uid: String::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::Message("address book not found".into()))
}

fn contact_target(db: &Database, contact_id: i64) -> AppResult<WriteTarget> {
    db.read()
        .query_row(
            "select c.href, c.etag, c.uid, c.address_book_id, b.read_only,
                    ca.id, ca.base_url, ca.username
             from contacts c
             join address_books b on b.id = c.address_book_id
             join contact_accounts ca on ca.id = b.contact_account_id
             where c.id = ?1 and ca.kind = 'carddav'",
            params![contact_id],
            |row| {
                Ok(WriteTarget {
                    href: row.get(0)?,
                    etag: row.get(1)?,
                    uid: row.get(2)?,
                    book_id: row.get(3)?,
                    read_only: row.get::<_, i64>(4)? != 0,
                    account_id: row.get(5)?,
                    base_url: row.get(6)?,
                    username: row.get(7)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::Message("contact not found".into()))
}

fn client_for_target(
    secrets: &dyn SecretStore,
    target: &WriteTarget,
) -> AppResult<CardDavClient> {
    let password = secrets.get(&contact_account_key(target.account_id))?;
    client_for_base(&target.base_url, &target.username, &password)
}

pub async fn create(
    db: &Database,
    secrets: &dyn SecretStore,
    book_id: i64,
    input: crate::contacts::ContactInput,
) -> AppResult<crate::contacts::ContactRecord> {
    crate::contacts::validate(&input)?;
    let target = book_target(db, book_id)?;
    if target.read_only {
        return Err(AppError::Message("this address book is read-only".into()));
    }
    let uid = format!("{}-{}", db::now(), rand::random::<u64>());
    let vcard = crate::contacts::vcard::build(&uid, &input);
    let path = member_path(&target.href, &format!("{uid}.vcf"));
    let client = client_for_target(secrets, &target)?;
    let response = client
        .put_if_none_match(&path, Bytes::from(vcard.clone()))
        .await
        .map_err(dav_error)?;
    if !response.status().is_success() {
        return Err(write_error(response.status().as_u16()));
    }
    let etag = etag_from_headers(response.headers());
    upsert_card(db, book_id, &path, etag.as_deref(), &vcard)?;
    let _ = sync_account(db, secrets, target.account_id).await;
    crate::contacts::record_by_href(db, book_id, &path)
}

pub async fn update(
    db: &Database,
    secrets: &dyn SecretStore,
    contact_id: i64,
    input: crate::contacts::ContactInput,
) -> AppResult<crate::contacts::ContactRecord> {
    crate::contacts::validate(&input)?;
    let target = contact_target(db, contact_id)?;
    if target.read_only {
        return Err(AppError::Message("this address book is read-only".into()));
    }
    let etag = target
        .etag
        .clone()
        .ok_or_else(|| AppError::Message("contact has no ETag".into()))?;
    let vcard = crate::contacts::vcard::build(&target.uid, &input);
    let client = client_for_target(secrets, &target)?;
    let response = client
        .put_if_match(&target.href, Bytes::from(vcard.clone()), &etag)
        .await
        .map_err(dav_error)?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        if status == 412 || status == 403 {
            let _ = sync_account(db, secrets, target.account_id).await;
        }
        if status == 403 {
            let _ = mark_read_only(db, target.book_id);
        }
        return Err(write_error(status));
    }
    let new_etag = etag_from_headers(response.headers()).or(Some(etag));
    upsert_card(db, target.book_id, &target.href, new_etag.as_deref(), &vcard)?;
    let _ = sync_account(db, secrets, target.account_id).await;
    crate::contacts::get_record(db, contact_id)
}

pub async fn delete(
    db: &Database,
    secrets: &dyn SecretStore,
    contact_id: i64,
) -> AppResult<()> {
    let target = contact_target(db, contact_id)?;
    if target.read_only {
        return Err(AppError::Message("this address book is read-only".into()));
    }
    let etag = target
        .etag
        .clone()
        .ok_or_else(|| AppError::Message("contact has no ETag".into()))?;
    let client = client_for_target(secrets, &target)?;
    let response = client
        .delete_if_match(&target.href, &etag)
        .await
        .map_err(dav_error)?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        if status == 412 || status == 403 {
            let _ = sync_account(db, secrets, target.account_id).await;
        }
        if status == 403 {
            let _ = mark_read_only(db, target.book_id);
        }
        return Err(write_error(status));
    }
    db.conn.lock().unwrap().execute(
        "delete from contacts where id = ?1",
        params![contact_id],
    )?;
    let _ = sync_account(db, secrets, target.account_id).await;
    Ok(())
}

struct BookTarget {
    id: i64,
    href: String,
    sync_token: Option<String>,
}

fn book_targets(db: &Database, account_id: i64) -> AppResult<Vec<BookTarget>> {
    let conn = db.read();
    let mut stmt = conn.prepare(
        "select id, href, sync_token from address_books
         where contact_account_id = ?1
         order by id",
    )?;
    let rows = stmt.query_map(params![account_id], |row| {
        Ok(BookTarget {
            id: row.get(0)?,
            href: row.get(1)?,
            sync_token: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn upsert_conn(
    conn: &rusqlite::Connection,
    book_id: i64,
    href: &str,
    etag: Option<&str>,
    raw: &str,
) -> AppResult<()> {
    let card = crate::contacts::vcard::parse(raw);
    let emails = serde_json::to_string(&card.emails)?;
    let phones = serde_json::to_string(&card.phones)?;
    conn.execute(
        "insert into contacts
            (address_book_id, href, etag, uid, display_name, emails, phones,
             organization, title, notes, photo, vcard, updated_at)
         values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         on conflict(address_book_id, href) do update set
            etag = excluded.etag,
            uid = excluded.uid,
            display_name = excluded.display_name,
            emails = excluded.emails,
            phones = excluded.phones,
            organization = excluded.organization,
            title = excluded.title,
            notes = excluded.notes,
            photo = excluded.photo,
            vcard = excluded.vcard,
            updated_at = excluded.updated_at",
        params![
            book_id,
            href,
            etag,
            card.uid,
            card.display_name,
            emails,
            phones,
            card.organization,
            card.title,
            card.notes,
            card.photo,
            raw,
            db::now(),
        ],
    )?;
    Ok(())
}

pub fn upsert_card(
    db: &Database,
    book_id: i64,
    href: &str,
    etag: Option<&str>,
    raw: &str,
) -> AppResult<()> {
    let conn = db.conn.lock().unwrap();
    upsert_conn(&conn, book_id, href, etag, raw)
}

pub fn apply_snapshot(
    db: &Database,
    book_id: i64,
    entries: &[(String, Option<String>, String)],
) -> AppResult<usize> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    let existing: Vec<String> = {
        let mut stmt = tx.prepare("select href from contacts where address_book_id = ?1")?;
        let rows = stmt.query_map(params![book_id], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let keep: std::collections::HashSet<&str> =
        entries.iter().map(|(href, _, _)| href.as_str()).collect();
    for href in existing {
        if !keep.contains(href.as_str()) {
            tx.execute(
                "delete from contacts where address_book_id = ?1 and href = ?2",
                params![book_id, href],
            )?;
        }
    }
    for (href, etag, raw) in entries {
        upsert_conn(&tx, book_id, href, etag.as_deref(), raw)?;
    }
    tx.commit()?;
    Ok(entries.len())
}

pub fn apply_delta(
    db: &Database,
    book_id: i64,
    added: &[(String, Option<String>, String)],
    deleted: &[String],
) -> AppResult<usize> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    for href in deleted {
        tx.execute(
            "delete from contacts where address_book_id = ?1 and href = ?2",
            params![book_id, href],
        )?;
    }
    for (href, etag, raw) in added {
        upsert_conn(&tx, book_id, href, etag.as_deref(), raw)?;
    }
    tx.commit()?;
    Ok(added.len())
}

fn cards_from_entries(entries: &[fast_dav_rs::webdav::sync::SyncEntry]) -> (Vec<(String, Option<String>, String)>, Vec<String>) {
    let mut cards = Vec::new();
    let mut errors = Vec::new();
    for entry in entries {
        match entry.data.as_ref() {
            Some(data) => cards.push((entry.href.clone(), entry.etag.clone(), data.clone())),
            None => errors.push(format!("{} did not include contact data", entry.href)),
        }
    }
    (cards, errors)
}

fn set_sync_token(db: &Database, book_id: i64, token: Option<&str>) -> AppResult<()> {
    db.conn.lock().unwrap().execute(
        "update address_books set sync_token = ?1, last_synced_at = ?2 where id = ?3",
        params![token, db::now(), book_id],
    )?;
    Ok(())
}

pub async fn sync_account(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: i64,
) -> AppResult<crate::contacts::ContactSyncResult> {
    let (base_url, username): (String, String) = db
        .read()
        .query_row(
            "select base_url, username from contact_accounts where id = ?1 and kind = 'carddav'",
            params![account_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::Message("contact account not found".into()))?;
    let password = secrets.get(&contact_account_key(account_id))?;
    let client = client_for_base(&base_url, &username, &password)?;
    let mut result = crate::contacts::ContactSyncResult {
        books: 0,
        contacts: 0,
        errors: Vec::new(),
    };

    match discover_books(&client).await {
        Ok(remote) => {
            if let Err(error) = save_books(db, account_id, &remote) {
                result.errors.push(error.to_string());
            }
        }
        Err(error) => result.errors.push(error.to_string()),
    }

    for book in book_targets(db, account_id)? {
        let session = client
            .sync_session(&book.href)
            .with_sync_token(book.sync_token.as_deref());
        if book.sync_token.is_none() {
            match session.initial().await {
                Ok(snapshot) => {
                    let (cards, errors) = cards_from_entries(&snapshot.items);
                    result.errors.extend(errors);
                    apply_snapshot(db, book.id, &cards)?;
                    set_sync_token(db, book.id, snapshot.sync_token.as_deref())?;
                    result.books += 1;
                    result.contacts += cards.len();
                }
                Err(error) => result
                    .errors
                    .push(format!("{}: {error}", book.href)),
            }
            continue;
        }
        match session.incremental().await {
            Ok(delta) => {
                let (added, added_errors) = cards_from_entries(&delta.added);
                result.errors.extend(added_errors);
                if delta.resynced {
                    apply_snapshot(db, book.id, &added)?;
                } else {
                    apply_delta(db, book.id, &added, &delta.deleted)?;
                }
                set_sync_token(db, book.id, delta.sync_token.as_deref())?;
                result.books += 1;
                result.contacts += added.len();
            }
            Err(error) => result
                .errors
                .push(format!("{}: {error}", book.href)),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contacts::ensure_local_space;

    fn seed_carddav_account(db: &Database) -> (i64, i64) {
        let space = crate::spaces::ensure_default(db).unwrap();
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "insert into contact_accounts
                (display_name, base_url, username, space_id, position, kind, created_at)
             values ('Work', 'https://dav.example.com', 'me', ?1, 0, 'carddav', 0)",
            params![space.id],
        )
        .unwrap();
        let account = conn.last_insert_rowid();
        conn.execute(
            "insert into address_books (contact_account_id, href, name)
             values (?1, '/books/main/', 'Contacts')",
            params![account],
        )
        .unwrap();
        (account, conn.last_insert_rowid())
    }

    #[test]
    fn write_errors_explain_conflicts_and_read_only_books() {
        assert!(write_error(412)
            .to_string()
            .contains("changed on the server"));
        assert!(write_error(403).to_string().contains("read-only"));
        assert!(write_error(500).to_string().contains("rejected"));
    }

    #[test]
    fn marking_a_book_read_only_persists() {
        let db = Database::open_memory().unwrap();
        let (account, book) = seed_carddav_account(&db);
        mark_read_only(&db, book).unwrap();
        let space_id: i64 = {
            let conn = db.read();
            conn.query_row(
                "select space_id from contact_accounts where id = ?1",
                params![account],
                |row| row.get(0),
            )
            .unwrap()
        };
        let books = crate::contacts::list_address_books(&db, space_id).unwrap();
        assert!(books
            .iter()
            .find(|candidate| candidate.id == book)
            .unwrap()
            .read_only);
    }

    #[tokio::test]
    async fn local_contacts_dispatch_without_a_network_client() {
        let db = Database::open_memory().unwrap();
        let space = crate::spaces::ensure_default(&db).unwrap();
        crate::contacts::ensure_local_space(&db, space.id).unwrap();
        let book = crate::contacts::local_book_id(&db, space.id).unwrap();
        let record = crate::contacts::create_contact(
            &db,
            &crate::secrets::MemoryStore::default(),
            crate::contacts::ContactInput {
                display_name: "Ada".into(),
                emails: vec!["ada@example.com".into()],
                phones: vec![],
                organization: String::new(),
                title: String::new(),
                notes: String::new(),
                address_book_id: Some(book),
            },
        )
        .await
        .unwrap();
        assert_eq!(record.display_name, "Ada");
    }

    #[test]
    fn snapshot_and_delta_keep_the_contact_cache_in_sync() {
        let db = Database::open_memory().unwrap();
        let (_account, book) = seed_carddav_account(&db);
        let v1 = "BEGIN:VCARD\nVERSION:4.0\nUID:a\nFN:Ada\nEMAIL:ada@example.com\nEND:VCARD\n";
        let v2 = "BEGIN:VCARD\nVERSION:4.0\nUID:b\nFN:Bob\nEMAIL:bob@example.com\nEND:VCARD\n";

        assert_eq!(
            apply_snapshot(
                &db,
                book,
                &[
                    ("a.vcf".into(), Some("e1".into()), v1.into()),
                    ("b.vcf".into(), Some("e2".into()), v2.into()),
                ]
            )
            .unwrap(),
            2
        );

        let v2b = "BEGIN:VCARD\nVERSION:4.0\nUID:b\nFN:Bob Stone\nEMAIL:bob@example.com\nEND:VCARD\n";
        apply_delta(
            &db,
            book,
            &[("b.vcf".into(), Some("e3".into()), v2b.into())],
            &["a.vcf".into()],
        )
        .unwrap();

        let conn = db.conn.lock().unwrap();
        let records: Vec<(String, Option<String>)> = conn
            .prepare("select display_name, etag from contacts order by href")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, "Bob Stone");
        assert_eq!(records[0].1.as_deref(), Some("e3"));
    }

    #[test]
    fn save_books_upserts_metadata_and_keeps_cache_state() {
        let db = Database::open_memory().unwrap();
        let space = crate::spaces::ensure_default(&db).unwrap();
        ensure_local_space(&db, space.id).unwrap();
        let account = db
            .conn
            .lock()
            .unwrap()
            .query_row(
                "select id from contact_accounts where space_id = ?1 and kind = 'local'",
                params![space.id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();

        save_books(
            &db,
            account,
            &[RemoteBook {
                href: "/books/work/".into(),
                name: "Work".into(),
                color: Some("#6da4ff".into()),
            }],
        )
        .unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "update address_books set sync_token = 'token-1' where href = '/books/work/'",
                [],
            )
            .unwrap();
        save_books(
            &db,
            account,
            &[RemoteBook {
                href: "/books/work/".into(),
                name: "Work team".into(),
                color: None,
            }],
        )
        .unwrap();

        let (name, token): (String, Option<String>) = db
            .conn
            .lock()
            .unwrap()
            .query_row(
                "select name, sync_token from address_books where href = '/books/work/'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "Work team");
        assert_eq!(token.as_deref(), Some("token-1"));
    }
}
