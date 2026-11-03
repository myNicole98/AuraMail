use std::sync::Once;
use std::time::Duration;

use fast_dav_rs::CardDavClient;
use rusqlite::params;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contacts::ensure_local_space;

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
