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

pub trait SecretStore: Send + Sync {
    fn set(&self, key: &str, value: &str) -> AppResult<()>;
    fn get(&self, key: &str) -> AppResult<String>;
    fn delete(&self, key: &str) -> AppResult<()>;
}

pub struct KeyringStore;

fn keyring_get_error(err: keyring::Error) -> AppError {
    match err {
        keyring::Error::NoEntry => AppError::Message(SECRET_NOT_FOUND.into()),
        _ => AppError::Message("secret store unavailable".into()),
    }
}

impl SecretStore for KeyringStore {
    fn set(&self, key: &str, value: &str) -> AppResult<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, key)
            .map_err(|_| AppError::Message("secret store unavailable".into()))?;
        entry
            .set_password(value)
            .map_err(|_| AppError::Message("could not store secret".into()))
    }

    fn get(&self, key: &str) -> AppResult<String> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, key)
            .map_err(|_| AppError::Message("secret store unavailable".into()))?;
        entry.get_password().map_err(keyring_get_error)
    }

    fn delete(&self, key: &str) -> AppResult<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, key)
            .map_err(|_| AppError::Message("secret store unavailable".into()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AppError::Message("could not delete secret".into())),
        }
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
