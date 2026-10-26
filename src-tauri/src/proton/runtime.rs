use std::sync::{Arc, OnceLock, RwLock};

use crate::accounts::{self, AccountAuth};
use crate::db::Database;
use crate::error::{AppError, AppResult};

use super::bridge::ProtonBridge;

static BRIDGE: OnceLock<RwLock<Option<Arc<ProtonBridge>>>> = OnceLock::new();

fn slot() -> &'static RwLock<Option<Arc<ProtonBridge>>> {
    BRIDGE.get_or_init(|| RwLock::new(None))
}

fn read() -> std::sync::RwLockReadGuard<'static, Option<Arc<ProtonBridge>>> {
    slot()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write() -> std::sync::RwLockWriteGuard<'static, Option<Arc<ProtonBridge>>> {
    slot()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn install(bridge: Arc<ProtonBridge>) {
    let previous = write().replace(bridge);
    if let Some(previous) = previous {
        previous.shutdown_auth();
        previous.stop();
    }
}

pub fn current() -> AppResult<Arc<ProtonBridge>> {
    read()
        .clone()
        .ok_or_else(|| AppError::Message("Proton bridge is not configured".into()))
}

pub fn stop() {
    let bridge = write().take();
    if let Some(bridge) = bridge {
        bridge.shutdown_auth();
        bridge.stop();
    }
}

/// Starts the bridge only when at least one Proton account exists. Called at
/// startup before polling begins.
pub fn start_if_needed(db: &Database) -> AppResult<()> {
    let has_proton = accounts::list(db)?
        .iter()
        .any(|account| account.auth_type == AccountAuth::Proton);
    if has_proton {
        current()?.start_if_needed()?;
    }
    Ok(())
}

pub fn forget_account(username: &str) -> AppResult<()> {
    current()?.forget_account(username)
}

/// Removes several Hydroxide auth entries before a single bridge reconcile.
pub fn forget_accounts(usernames: &[String]) -> AppResult<()> {
    let bridge = current()?;
    for username in usernames {
        bridge.forget_account(username)?;
    }
    Ok(())
}

/// Restarts the bridge when Proton accounts remain, or stops it when the last
/// one was removed.
pub fn reconcile() -> AppResult<()> {
    let bridge = current()?;
    if bridge.has_proton_accounts()? {
        bridge.restart()
    } else {
        bridge.stop();
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
