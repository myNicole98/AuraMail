use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const KEYRING_SERVICE: &str = "auramail";
#[allow(dead_code)]
pub const AI_KEY: &str = "ai";
pub(crate) const SECRET_NOT_FOUND: &str = "secret not found";

pub fn account_key(id: i64) -> String {
    format!("account:{id}")
}

pub fn calendar_account_key(id: i64) -> String {
    format!("calendar-account:{id}")
}

pub fn contact_account_key(id: i64) -> String {
    format!("contact-account:{id}")
}

pub trait SecretStore: Send + Sync {
    fn set(&self, key: &str, value: &str) -> AppResult<()>;
    fn get(&self, key: &str) -> AppResult<String>;
    fn delete(&self, key: &str) -> AppResult<()>;
}

/// Version of the on-disk envelope. Bumping it is what would let the file pick
/// up an encrypted format without touching a single call site.
const STORE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Envelope {
    #[serde(default = "store_version")]
    version: u32,
    #[serde(default)]
    migrated: bool,
    #[serde(default)]
    secrets: std::collections::BTreeMap<String, String>,
}

fn store_version() -> u32 {
    STORE_VERSION
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            version: STORE_VERSION,
            migrated: false,
            secrets: std::collections::BTreeMap::new(),
        }
    }
}

struct StoreState {
    path: PathBuf,
    envelope: Envelope,
}

/// Secrets live in a single owner-only file next to the database. The OS keyring
/// is only read once, to import what older releases stored there.
pub struct FileStore {
    inner: Mutex<StoreState>,
}

impl FileStore {
    pub fn open(path: impl Into<PathBuf>) -> AppResult<Self> {
        let path = path.into();
        let envelope = if path.exists() {
            match fs::read_to_string(&path) {
                Ok(raw) => match serde_json::from_str::<Envelope>(&raw) {
                    Ok(envelope) => envelope,
                    Err(error) => quarantine(&path, &raw, &error),
                },
                Err(error) => quarantine(&path, "", &error),
            }
        } else {
            Envelope::default()
        };
        Ok(Self {
            inner: Mutex::new(StoreState { path, envelope }),
        })
    }

    pub fn migration_pending(&self) -> bool {
        self.inner
            .lock()
            .map(|state| !state.envelope.migrated)
            .unwrap_or(true)
    }

    pub fn mark_migrated(&self) -> AppResult<()> {
        let mut state = self.lock()?;
        state.envelope.migrated = true;
        persist(&state.path, &state.envelope)
    }

    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, StoreState>> {
        self.inner
            .lock()
            .map_err(|_| AppError::Message("secret store unavailable".into()))
    }
}

/// Never overwrite bytes we failed to parse: they are the only copy left.
fn quarantine(path: &Path, raw: &str, error: &dyn std::fmt::Display) -> Envelope {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let target = path.with_file_name(format!("secrets.json.corrupt-{stamp}"));
    if !raw.is_empty() {
        let _ = fs::write(&target, raw);
    } else if let Err(copy_error) = fs::rename(path, &target) {
        eprintln!("could not quarantine unreadable secret store: {copy_error}");
    }
    eprintln!(
        "secret store was unreadable ({error}); kept it at {}",
        target.display()
    );
    Envelope::default()
}

fn persist(path: &Path, envelope: &Envelope) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let temp = path.with_file_name(format!("secrets.json.tmp-{}-{stamp}", std::process::id()));

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = match options.open(&temp) {
        Ok(mut file) => {
            let result = (|| -> AppResult<()> {
                serde_json::to_writer_pretty(&mut file, envelope)?;
                file.write_all(b"\n")?;
                file.sync_all()?;
                Ok(())
            })();
            drop(file);
            result
        }
        Err(error) => Err(AppError::Io(error)),
    };
    if let Err(error) = written {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(error.into());
    }
    Ok(())
}

impl SecretStore for FileStore {
    fn set(&self, key: &str, value: &str) -> AppResult<()> {
        let mut state = self.lock()?;
        state
            .envelope
            .secrets
            .insert(key.to_string(), value.to_string());
        if let Err(error) = persist(&state.path, &state.envelope) {
            state.envelope.secrets.remove(key);
            return Err(error);
        }
        Ok(())
    }

    fn get(&self, key: &str) -> AppResult<String> {
        let state = self.lock()?;
        state
            .envelope
            .secrets
            .get(key)
            .cloned()
            .ok_or_else(|| AppError::Message(SECRET_NOT_FOUND.into()))
    }

    fn delete(&self, key: &str) -> AppResult<()> {
        let mut state = self.lock()?;
        if state.envelope.secrets.remove(key).is_none() {
            return Ok(());
        }
        persist(&state.path, &state.envelope)
    }
}

#[allow(dead_code)]
#[derive(Default)]
pub struct MemoryStore {
    values: Mutex<HashMap<String, String>>,
}

impl SecretStore for MemoryStore {
    fn set(&self, key: &str, value: &str) -> AppResult<()> {
        self.values
            .lock()
            .map_err(|_| AppError::Message("secret store unavailable".into()))?
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn get(&self, key: &str) -> AppResult<String> {
        self.values
            .lock()
            .map_err(|_| AppError::Message("secret store unavailable".into()))?
            .get(key)
            .cloned()
            .ok_or_else(|| AppError::Message(SECRET_NOT_FOUND.into()))
    }

    fn delete(&self, key: &str) -> AppResult<()> {
        self.values
            .lock()
            .map_err(|_| AppError::Message("secret store unavailable".into()))?
            .remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trips_and_isolates_keys() {
        let store = MemoryStore::default();
        store.set("account:1", "secret").unwrap();
        assert_eq!(store.get("account:1").unwrap(), "secret");
        store.set("account:2", "other").unwrap();
        assert_eq!(store.get("account:2").unwrap(), "other");
        store.delete("account:1").unwrap();
        assert!(store.get("account:1").is_err());
        assert_eq!(store.get("account:2").unwrap(), "other");
    }

    #[test]
    fn keys_are_namespaced() {
        assert_eq!(account_key(7), "account:7");
        assert_eq!(AI_KEY, "ai");
    }

    fn temp_store() -> (tempfile::TempDir, FileStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::open(dir.path().join("secrets.json")).unwrap();
        (dir, store)
    }

    #[test]
    fn file_store_round_trips_and_reports_missing_keys() {
        let (_dir, store) = temp_store();
        assert_eq!(
            store.get("account:1").unwrap_err().to_string(),
            SECRET_NOT_FOUND
        );
        store.set("account:1", "secret").unwrap();
        assert_eq!(store.get("account:1").unwrap(), "secret");
        store.set("account:1", "rotated").unwrap();
        assert_eq!(store.get("account:1").unwrap(), "rotated");
        store.delete("account:1").unwrap();
        assert_eq!(
            store.get("account:1").unwrap_err().to_string(),
            SECRET_NOT_FOUND
        );
        store.delete("account:1").unwrap();
    }

    #[test]
    fn file_store_writes_the_versioned_envelope_on_disk() {
        let (dir, store) = temp_store();
        store.set("ai:one", "a").unwrap();
        store.set("ai:two", "b").unwrap();

        let raw = std::fs::read_to_string(dir.path().join("secrets.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed["version"], 1);
        assert_eq!(parsed["migrated"], false);
        assert_eq!(parsed["secrets"]["ai:two"], "b");

        let reopened = FileStore::open(dir.path().join("secrets.json")).unwrap();
        assert_eq!(reopened.get("ai:one").unwrap(), "a");
    }

    #[cfg(unix)]
    #[test]
    fn file_store_is_owner_only_readable() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, store) = temp_store();
        store.set("account:1", "secret").unwrap();
        store.set("account:2", "secret").unwrap();

        let mode = std::fs::metadata(dir.path().join("secrets.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn a_corrupt_file_is_quarantined_instead_of_wiped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        std::fs::write(&path, "not json").unwrap();

        let store = FileStore::open(&path).unwrap();
        assert_eq!(
            store.get("account:1").unwrap_err().to_string(),
            SECRET_NOT_FOUND
        );

        let quarantined: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with("secrets.json.corrupt-"))
            .collect();
        assert_eq!(quarantined.len(), 1);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(&quarantined[0])).unwrap(),
            "not json"
        );
    }

    #[test]
    fn migrations_flag_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        {
            let store = FileStore::open(&path).unwrap();
            assert!(store.migration_pending());
            store.mark_migrated().unwrap();
            assert!(!store.migration_pending());
        }
        let reopened = FileStore::open(&path).unwrap();
        assert!(!reopened.migration_pending());
    }

    #[test]
    fn concurrent_writers_keep_every_key() {
        use std::sync::Arc;
        let (_dir, store) = temp_store();
        let store = Arc::new(store);
        let handles: Vec<_> = (0..8)
            .map(|index| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    for round in 0..5 {
                        store
                            .set(&format!("account:{index}-{round}"), "value")
                            .unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        for index in 0..8 {
            for round in 0..5 {
                assert_eq!(
                    store.get(&format!("account:{index}-{round}")).unwrap(),
                    "value"
                );
            }
        }
    }
}
