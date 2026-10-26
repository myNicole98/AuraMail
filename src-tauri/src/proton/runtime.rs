use std::sync::{Arc, OnceLock, RwLock};

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

pub fn forget_account(username: &str) -> AppResult<()> {
    current()?.forget_account(username)
}
