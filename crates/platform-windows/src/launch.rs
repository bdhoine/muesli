//! How Muesli was started: by a person (open the main window, or setup) or at sign-in (stay in
//! the tray). The Run value of "Start when I sign in" passes [`AUTOSTART`].

use crate::login_item::AUTOSTART;

/// True when Windows started Muesli at sign-in.
pub fn launched_at_login() -> bool {
    std::env::args().skip(1).any(|a| a == AUTOSTART)
}
