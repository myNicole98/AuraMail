use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use crate::ai::{self, AiConfig};
use crate::db::Database;
use crate::error::AppResult;
use crate::secrets::{
    account_key, calendar_account_key, contact_account_key, FileStore, SecretStore, AI_KEY,
    KEYRING_SERVICE,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyValue {
    Found(String),
    Missing,
    Unavailable,
}

pub trait LegacyLookup: Send + Sync {
    fn get(&self, key: &str) -> LegacyValue;
    fn delete(&self, key: &str);
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MigrationReport {
    pub imported: usize,
    pub missing: usize,
    pub unavailable: usize,
    pub skipped_after_budget: usize,
}

/// Every key the app can hold today, so the import covers accounts, calendars,
/// contacts and per-space AI keys. Direct SQL because the public listers are
/// per-space and filtered by `kind`.
pub fn candidate_keys(db: &Database) -> AppResult<Vec<String>> {
    let mut keys: BTreeSet<String> = BTreeSet::new();
    {
        let conn = db.read();
        for (table, build) in [
            ("accounts", account_key as fn(i64) -> String),
            ("calendar_accounts", calendar_account_key),
            ("contact_accounts", contact_account_key),
        ] {
            let mut statement = conn.prepare(&format!("select id from {table}"))?;
            let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
            for id in rows {
                keys.insert(build(id?));
            }
        }
    }
    for space in crate::spaces::list(db)? {
        if let Ok(config) = AiConfig::load(db, space.id) {
            if !config.base_url.trim().is_empty() {
                keys.insert(ai::key_name(space.id, &config.base_url));
            }
        }
    }
    keys.insert(AI_KEY.to_string());
    Ok(keys.into_iter().collect())
}

pub fn run(
    store: &FileStore,
    db: &Database,
    legacy: &dyn LegacyLookup,
    budget: Duration,
) -> AppResult<MigrationReport> {
    run_keys(store, candidate_keys(db)?, legacy, budget)
}

fn run_keys(
    store: &FileStore,
    keys: Vec<String>,
    legacy: &dyn LegacyLookup,
    budget: Duration,
) -> AppResult<MigrationReport> {
    if !store.migration_pending() {
        return Ok(MigrationReport::default());
    }
    let started = Instant::now();
    let mut report = MigrationReport::default();
    for key in keys {
        if started.elapsed() >= budget {
            report.skipped_after_budget += 1;
            continue;
        }
        match legacy.get(&key) {
            LegacyValue::Found(value) => match store.set(&key, &value) {
                Ok(()) => {
                    legacy.delete(&key);
                    report.imported += 1;
                }
                Err(_) => report.unavailable += 1,
            },
            LegacyValue::Missing => report.missing += 1,
            LegacyValue::Unavailable => report.unavailable += 1,
        }
    }
    store.mark_migrated()?;
    Ok(report)
}

/// Older Windows builds stored values over 1000 chars as `key#chunk:N` with a
/// `key#chunks` manifest. Reassemble those instead of losing the token.
fn legacy_value(
    plain: LegacyValue,
    manifest: impl FnOnce() -> LegacyValue,
    chunk: impl FnMut(usize) -> LegacyValue,
) -> LegacyValue {
    if plain != LegacyValue::Missing {
        return plain;
    }
    let count = match manifest() {
        LegacyValue::Found(value) => match value.parse::<usize>() {
            Ok(count) => count,
            Err(_) => return LegacyValue::Unavailable,
        },
        _ => return LegacyValue::Unavailable,
    };
    let mut chunk = chunk;
    let mut assembled = String::new();
    for index in 0..count {
        match chunk(index) {
            LegacyValue::Found(part) => assembled.push_str(&part),
            _ => return LegacyValue::Unavailable,
        }
    }
    LegacyValue::Found(assembled)
}

pub struct KeyringLookup;

impl KeyringLookup {
    fn entry(key: &str) -> Option<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, key).ok()
    }

    fn read(key: &str) -> LegacyValue {
        match Self::entry(key).map(|entry| entry.get_password()) {
            Some(Ok(value)) => LegacyValue::Found(value),
            Some(Err(keyring::Error::NoEntry)) => LegacyValue::Missing,
            _ => LegacyValue::Unavailable,
        }
    }
}

impl LegacyLookup for KeyringLookup {
    fn get(&self, key: &str) -> LegacyValue {
        legacy_value(
            Self::read(key),
            || Self::read(&format!("{key}#chunks")),
            |index| Self::read(&format!("{key}#chunk:{index}")),
        )
    }

    fn delete(&self, key: &str) {
        if let Some(entry) = Self::entry(key) {
            let _ = entry.delete_credential();
        }
        let manifest = format!("{key}#chunks");
        let count = match Self::read(&manifest) {
            LegacyValue::Found(value) => value.parse::<usize>().unwrap_or(0),
            _ => 0,
        };
        for index in 0..count {
            if let Some(entry) = Self::entry(&format!("{key}#chunk:{index}")) {
                let _ = entry.delete_credential();
            }
        }
        if let Some(entry) = Self::entry(&manifest) {
            let _ = entry.delete_credential();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{FileStore, SecretStore};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::Duration;

    struct FakeLegacy {
        values: Mutex<HashMap<String, LegacyValue>>,
        deleted: Mutex<Vec<String>>,
    }

    impl FakeLegacy {
        fn new(pairs: Vec<(&str, LegacyValue)>) -> Self {
            Self {
                values: Mutex::new(
                    pairs
                        .into_iter()
                        .map(|(key, value)| (key.to_string(), value))
                        .collect(),
                ),
                deleted: Mutex::new(Vec::new()),
            }
        }

        fn deleted(&self) -> Vec<String> {
            self.deleted.lock().unwrap().clone()
        }
    }

    impl LegacyLookup for FakeLegacy {
        fn get(&self, key: &str) -> LegacyValue {
            self.values
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .unwrap_or(LegacyValue::Missing)
        }

        fn delete(&self, key: &str) {
            self.deleted.lock().unwrap().push(key.to_string());
        }
    }

    fn store() -> (tempfile::TempDir, FileStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::open(dir.path().join("secrets.json")).unwrap();
        (dir, store)
    }

    #[test]
    fn found_values_are_imported_and_deleted() {
        let (_dir, store) = store();
        let legacy = FakeLegacy::new(vec![
            ("account:1", LegacyValue::Found("a".into())),
            ("ai", LegacyValue::Found("b".into())),
        ]);

        let report = run_keys(
            &store,
            vec!["account:1".into(), "ai".into()],
            &legacy,
            Duration::MAX,
        )
        .unwrap();

        assert_eq!(report.imported, 2);
        assert_eq!(store.get("account:1").unwrap(), "a");
        assert_eq!(store.get("ai").unwrap(), "b");
        assert_eq!(
            legacy.deleted(),
            vec!["account:1".to_string(), "ai".to_string()]
        );
        assert!(!store.migration_pending());
    }

    #[test]
    fn unreadable_values_are_neither_stored_nor_deleted() {
        let (_dir, store) = store();
        let legacy = FakeLegacy::new(vec![
            ("account:1", LegacyValue::Unavailable),
            ("account:2", LegacyValue::Missing),
        ]);

        let report = run_keys(
            &store,
            vec!["account:1".into(), "account:2".into()],
            &legacy,
            Duration::MAX,
        )
        .unwrap();

        assert_eq!(report.imported, 0);
        assert_eq!(report.unavailable, 1);
        assert_eq!(report.missing, 1);
        assert_eq!(
            store.get("account:1").unwrap_err().to_string(),
            "secret not found"
        );
        assert!(legacy.deleted().is_empty());
        assert!(!store.migration_pending());
    }

    #[test]
    fn a_second_run_does_nothing() {
        let (_dir, store) = store();
        let legacy = FakeLegacy::new(vec![("account:1", LegacyValue::Found("a".into()))]);
        run_keys(&store, vec!["account:1".into()], &legacy, Duration::MAX).unwrap();

        let again = run_keys(&store, vec!["account:1".into()], &legacy, Duration::MAX).unwrap();
        assert_eq!(again.imported, 0);
        assert_eq!(legacy.deleted().len(), 1);
    }

    #[test]
    fn an_exhausted_budget_imports_nothing_but_still_marks_the_store() {
        let (_dir, store) = store();
        let legacy = FakeLegacy::new(vec![("account:1", LegacyValue::Found("a".into()))]);

        let report = run_keys(&store, vec!["account:1".into()], &legacy, Duration::ZERO).unwrap();

        assert_eq!(report.imported, 0);
        assert_eq!(report.skipped_after_budget, 1);
        assert!(legacy.deleted().is_empty());
        assert!(!store.migration_pending());
    }

    #[test]
    fn chunked_legacy_values_are_reassembled() {
        assert_eq!(
            legacy_value(
                LegacyValue::Found("plain".into()),
                || LegacyValue::Found("0".into()),
                |_| LegacyValue::Missing
            ),
            LegacyValue::Found("plain".into())
        );
        assert_eq!(
            legacy_value(
                LegacyValue::Missing,
                || LegacyValue::Found("2".into()),
                |index| LegacyValue::Found(if index == 0 { "one-" } else { "two" }.into())
            ),
            LegacyValue::Found("one-two".into())
        );
        assert_eq!(
            legacy_value(
                LegacyValue::Missing,
                || LegacyValue::Found("garbage".into()),
                |_| LegacyValue::Missing
            ),
            LegacyValue::Unavailable
        );
        assert_eq!(
            legacy_value(
                LegacyValue::Missing,
                || LegacyValue::Found("2".into()),
                |_| LegacyValue::Missing
            ),
            LegacyValue::Unavailable
        );
        assert_eq!(
            legacy_value(
                LegacyValue::Unavailable,
                || LegacyValue::Found("2".into()),
                |_| LegacyValue::Found("x".into())
            ),
            LegacyValue::Unavailable
        );
    }
}
