#![allow(dead_code)]
#![allow(unused_imports)]

pub mod bridge;
pub mod ports;
pub mod runtime;

use tauri::{AppHandle, Emitter};

pub use bridge::{sidecar_path, ProtonAccessLimited, ProtonBridge, ProtonEvents};
pub use ports::BridgePorts;

pub struct TauriProtonEvents {
    app: AppHandle,
}

impl TauriProtonEvents {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl ProtonEvents for TauriProtonEvents {
    fn access_limited(&self, status: ProtonAccessLimited) {
        let _ = self.app.emit("proton://access_limited", status);
    }
}
