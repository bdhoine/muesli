//! Start when I sign in: a value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
//! that starts this executable (the same thing `tauri-plugin-autostart` wrote before).

use muesli_core::ports::LoginItem;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The `LoginItem` adapter.
pub struct RunKey {
    name: &'static str,
}

impl RunKey {
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }
}

impl LoginItem for RunKey {
    fn is_enabled(&self) -> bool {
        windows_registry::CURRENT_USER.open(RUN).and_then(|k| k.get_string(self.name)).is_ok()
    }

    fn set_enabled(&self, on: bool) -> Result<(), String> {
        let key = windows_registry::CURRENT_USER.create(RUN).map_err(|e| e.to_string())?;
        if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            key.set_string(self.name, format!("\"{}\"", exe.display())).map_err(|e| e.to_string())
        } else {
            match key.remove_value(self.name) {
                Ok(()) => Ok(()),
                Err(_) if !self.is_enabled() => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }
}
