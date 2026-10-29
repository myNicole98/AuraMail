use std::collections::HashMap;
use std::sync::Mutex;

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

pub trait SecretStore: Send + Sync {
    fn set(&self, key: &str, value: &str) -> AppResult<()>;
    fn get(&self, key: &str) -> AppResult<String>;
    fn delete(&self, key: &str) -> AppResult<()>;
}

pub struct KeyringStore;

/// Windows Credential Manager caps a generic credential blob at 2560 bytes.
/// Keyring encodes strings as UTF-16 there, so keep chunks comfortably below
/// the limit and store large OAuth payloads across several entries.
const MAX_SECRET_CHARS: usize = 1000;

fn chunk_manifest_key(key: &str) -> String {
    format!("{key}#chunks")
}

fn chunk_key(key: &str, index: usize) -> String {
    format!("{key}#chunk:{index}")
}

fn entry(key: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, key)
        .map_err(|_| AppError::Message("secret store unavailable".into()))
}

fn secret_chunks(value: &str) -> Vec<String> {
    value
        .chars()
        .collect::<Vec<_>>()
        .chunks(MAX_SECRET_CHARS)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

fn delete_entry(key: &str) -> AppResult<()> {
    match entry(key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(AppError::Message("could not delete secret".into())),
    }
}

fn delete_chunks(key: &str) {
    let manifest = chunk_manifest_key(key);
    let count = entry(&manifest)
        .ok()
        .and_then(|entry| entry.get_password().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    for index in 0..count {
        let _ = delete_entry(&chunk_key(key, index));
    }
    let _ = delete_entry(&manifest);
}

fn keyring_get_error(err: keyring::Error) -> AppError {
    match err {
        keyring::Error::NoEntry => AppError::Message(SECRET_NOT_FOUND.into()),
        _ => AppError::Message("secret store unavailable".into()),
    }
}

impl SecretStore for KeyringStore {
    fn set(&self, key: &str, value: &str) -> AppResult<()> {
        let chars = value.chars().count();
        if chars <= MAX_SECRET_CHARS {
            let main = entry(key)?;
            main.set_password(value)
                .map_err(|_| AppError::Message("could not store secret".into()))?;
            delete_chunks(key);
            return Ok(());
        }

        delete_chunks(key);
        let chunks = secret_chunks(value);
        for (index, part) in chunks.iter().enumerate() {
            entry(&chunk_key(key, index))?
                .set_password(part)
                .map_err(|_| AppError::Message("could not store secret".into()))?;
        }
        entry(&chunk_manifest_key(key))?
            .set_password(&chunks.len().to_string())
            .map_err(|_| AppError::Message("could not store secret".into()))?;
        // A raw value may exist from an older release; the manifest takes
        // precedence while chunked data is present.
        let _ = delete_entry(key);
        Ok(())
    }

    fn get(&self, key: &str) -> AppResult<String> {
        match entry(key)?.get_password() {
            Ok(value) => Ok(value),
            Err(keyring::Error::NoEntry) => {
                let manifest = chunk_manifest_key(key);
                let count = match entry(&manifest)?.get_password() {
                    Ok(value) => value.parse::<usize>().map_err(|_| {
                        AppError::Message("secret store unavailable".into())
                    })?,
                    Err(keyring::Error::NoEntry) => {
                        return Err(AppError::Message(SECRET_NOT_FOUND.into()))
                    }
                    Err(error) => return Err(keyring_get_error(error)),
                };
                let mut value = String::new();
                for index in 0..count {
                    let part = entry(&chunk_key(key, index))?
                        .get_password()
                        .map_err(keyring_get_error)?;
                    value.push_str(&part);
                }
                Ok(value)
            }
            Err(error) => Err(keyring_get_error(error)),
        }
    }

    fn delete(&self, key: &str) -> AppResult<()> {
        let result = delete_entry(key);
        delete_chunks(key);
        result
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

    #[test]
    fn large_secrets_split_and_reassemble_losslessly() {
        let value = "x".repeat(MAX_SECRET_CHARS * 3 + 17);
        let chunks = secret_chunks(&value);

        assert_eq!(chunks.len(), 4);
        assert!(chunks
            .iter()
            .all(|chunk| chunk.chars().count() <= MAX_SECRET_CHARS));
        assert_eq!(chunks.concat(), value);
    }

    #[test]
    fn distinguishes_missing_entry_from_unavailable_store() {
        assert_eq!(
            keyring_get_error(keyring::Error::NoEntry).to_string(),
            "secret not found"
        );
        assert_eq!(
            keyring_get_error(keyring::Error::Invalid("user".into(), "bad".into())).to_string(),
            "secret store unavailable"
        );
    }
}
