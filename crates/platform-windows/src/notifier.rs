//! Toasts. A question ("Looks like a meeting… Start transcribing?") is a toast with two buttons; a
//! notice ("Saved: …") a toast with one. A desktop app that is not a Store package gets the button
//! click through the toast's `Activated` event, which only works while the app is running; Muesli
//! always is. When Windows will not show Muesli's toasts, `ask` fails and the engine falls back to
//! its own prompt window.

use std::sync::Arc;

use muesli_core::ports::{Access, Notice, Notifier, OnAction, OnAnswer, Question};
use tauri_winrt_notification::{Scenario, Toast};

const ACCEPT: &str = "accept";
const DECLINE: &str = "decline";
const OPEN: &str = "open";

/// What a click on the toast means. `None`: the body was clicked, which answers nothing.
pub fn answer_for(action: Option<&str>) -> Option<bool> {
    match action {
        Some(ACCEPT) => Some(true),
        Some(DECLINE) => Some(false),
        _ => None,
    }
}

/// Makes `app_id` known to Windows under `display_name`, so toasts are shown under Muesli's
/// name and not PowerShell's. Safe to call on every start.
pub fn register_app(app_id: &str, display_name: &str) -> Result<(), String> {
    let key = windows_registry::CURRENT_USER.create(format!("Software\\Classes\\AppUserModelId\\{app_id}")).map_err(|e| e.to_string())?;
    key.set_string("DisplayName", display_name).map_err(|e| e.to_string())
}

/// The `Notifier` adapter.
pub struct Toasts {
    pub app_id: &'static str,
}

impl Toasts {
    fn enabled(&self) -> Result<(), String> {
        match crate::permissions::notifications(self.app_id) {
            Access::Denied => Err("Notifications are off for Muesli.".into()),
            _ => Ok(()),
        }
    }
}

impl Notifier for Toasts {
    fn ask(&self, question: &Question, on_answer: OnAnswer) -> Result<(), String> {
        self.enabled()?;
        let on_answer = Arc::new(on_answer);
        let dismissed = on_answer.clone();
        Toast::new(self.app_id)
            .title(&question.title)
            .text1(&question.body)
            .scenario(Scenario::Reminder) // stays on screen until it is answered
            .add_button(&question.accept, ACCEPT)
            .add_button(&question.decline, DECLINE)
            .on_activated(move |action| {
                if let Some(answer) = answer_for(action.as_deref()) {
                    on_answer(answer);
                }
                Ok(())
            })
            .on_dismissed(move |_| {
                dismissed(false);
                Ok(())
            })
            .show()
            .map_err(|e| e.to_string())
    }

    fn notify(&self, notice: &Notice, on_action: OnAction) -> Result<(), String> {
        self.enabled()?;
        let mut toast = Toast::new(self.app_id).title(&notice.title).text1(&notice.body);
        if let Some(label) = &notice.action {
            toast = toast.add_button(label, OPEN);
        }
        toast
            .on_activated(move |action| {
                // The button, or a click on the toast itself: both open the transcript.
                if action.as_deref().is_none_or(|a| a == OPEN) {
                    on_action();
                }
                Ok(())
            })
            .show()
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_buttons_answer() {
        assert_eq!(answer_for(Some("accept")), Some(true));
        assert_eq!(answer_for(Some("decline")), Some(false));
        assert_eq!(answer_for(None), None); // a click on the text
        assert_eq!(answer_for(Some("something else")), None);
    }
}
