//! A notification that asks a yes/no question with two buttons, for "Looks like a meeting.
//! Start transcribing?" and "The call seems to have ended. Stop and save?".
//!
//! On Windows this is a toast with two buttons. A desktop app that is not a Store package gets
//! the button click through the toast's `Activated` event, which only works while the app is
//! running; Muesli always is. Elsewhere `ask` returns an error and the caller shows its own window.

/// The question to ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub title: String,
    pub body: String,
    /// Label of the button that means yes, e.g. "Start transcribing".
    pub accept: String,
    /// Label of the button that means no, e.g. "Not now".
    pub decline: String,
}

/// What a click on the toast means. `None`: the body was clicked, which answers nothing.
pub fn answer_for(action: Option<&str>) -> Option<bool> {
    match action {
        Some(ACCEPT) => Some(true),
        Some(DECLINE) => Some(false),
        _ => None,
    }
}

const ACCEPT: &str = "accept";
const DECLINE: &str = "decline";

#[cfg(windows)]
mod imp {
    use tauri_winrt_notification::{Scenario, Toast};

    use super::{answer_for, Question, ACCEPT, DECLINE};

    /// Makes `app_id` known to Windows under `display_name`, so toasts are shown under Muesli's
    /// name and not PowerShell's. Safe to call on every start.
    pub fn register_app(app_id: &str, display_name: &str) -> Result<(), String> {
        let key = windows_registry::CURRENT_USER
            .create(format!("Software\\Classes\\AppUserModelId\\{app_id}"))
            .map_err(|e| e.to_string())?;
        key.set_string("DisplayName", display_name).map_err(|e| e.to_string())
    }

    pub fn ask(app_id: &str, question: &Question, on_answer: impl Fn(bool) + Send + Sync + 'static) -> Result<(), String> {
        let on_answer = std::sync::Arc::new(on_answer);
        let dismissed = on_answer.clone();
        Toast::new(app_id)
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
}

#[cfg(not(windows))]
mod imp {
    use super::Question;

    pub fn register_app(_app_id: &str, _display_name: &str) -> Result<(), String> {
        Ok(())
    }

    pub fn ask(_app_id: &str, _question: &Question, _on_answer: impl Fn(bool) + Send + Sync + 'static) -> Result<(), String> {
        Err("Notifications with buttons are only implemented on Windows.".into())
    }
}

pub use imp::{ask, register_app};

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

    #[cfg(not(windows))]
    #[test]
    fn other_platforms_report_that_they_cannot_ask() {
        let q = Question { title: "t".into(), body: "b".into(), accept: "y".into(), decline: "n".into() };
        assert!(ask("id", &q, |_| {}).is_err());
        assert!(register_app("id", "Muesli").is_ok());
    }
}
