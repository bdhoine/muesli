//! Start when I sign in: a value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
//! that starts this executable with [`AUTOSTART`], so a start at sign-in can be told from a
//! person starting Muesli (`crate::launch`).

use muesli_core::ports::LoginItem;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The argument the Run value passes: Muesli started at sign-in, so it stays in the tray.
pub const AUTOSTART: &str = "--autostart";

fn command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(format!("\"{}\" {AUTOSTART}", exe.display()))
}

/// The `LoginItem` adapter.
pub struct RunKey {
    name: &'static str,
}

impl RunKey {
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }

    /// A Run value written by an older Muesli has no [`AUTOSTART`] (or names an older path):
    /// write it again, so the next start at sign-in stays in the tray.
    pub fn upgrade(self) -> Self {
        let current = windows_registry::CURRENT_USER.open(RUN).and_then(|k| k.get_string(self.name));
        if let (Ok(value), Ok(wanted)) = (current, command()) {
            if value != wanted {
                let _ = self.set_enabled(true);
            }
        }
        self
    }
}

impl LoginItem for RunKey {
    fn is_enabled(&self) -> bool {
        windows_registry::CURRENT_USER.open(RUN).and_then(|k| k.get_string(self.name)).is_ok()
    }

    fn set_enabled(&self, on: bool) -> Result<(), String> {
        let key = windows_registry::CURRENT_USER.create(RUN).map_err(|e| e.to_string())?;
        if on {
            key.set_string(self.name, command()?).map_err(|e| e.to_string())
        } else {
            match key.remove_value(self.name) {
                Ok(()) => Ok(()),
                Err(_) if !self.is_enabled() => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }
}
