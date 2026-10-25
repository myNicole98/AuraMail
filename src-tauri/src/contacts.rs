use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::db::Database;
use crate::error::AppResult;

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
