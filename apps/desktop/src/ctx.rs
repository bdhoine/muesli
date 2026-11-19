use std::sync::Arc;

use tauri::{AppHandle, Emitter};

use crate::state::{App, Status};

/// Everything a background thread needs: the shared state and the Tauri handle.
#[derive(Clone)]
pub struct Ctx {
    pub app: Arc<App>,
    pub handle: AppHandle,
}

impl Ctx {
    pub fn set_status(&self, status: Status) {
        *self.app.status.lock().unwrap() = status;
        self.changed();
    }

    /// Something changed: refresh `state.json`, the open windows, and the tray if its menu differs.
    pub fn changed(&self) {
        self.heartbeat();
        let _ = self.handle.emit("state-changed", ());
    }

    /// Keeps `state.json` fresh for other processes. Also picks up changes made outside the app
    /// (an assistant renaming a transcript) in the tray's recent list.
    pub fn heartbeat(&self) {
        let _ = self.app.app_status().write(&self.app.paths());
        crate::tray::refresh(self);
    }
}
