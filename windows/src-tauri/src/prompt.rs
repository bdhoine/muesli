//! The two questions Muesli asks by itself: "Looks like a meeting. Start transcribing?" and
//! "The call seems to have ended. Stop and save?". Asked as a system notification with buttons
//! where that exists (Windows), otherwise in a small window of our own.

use std::sync::atomic::{AtomicU64, Ordering};

use muesli_core::DetectedMeeting;
use muesli_notify::Question;

use crate::ctx::Ctx;
use crate::state::Prompt;
use crate::{session, windows};

/// The id Windows knows the app by; the same as the bundle identifier.
pub const APP_ID: &str = "com.github.bdhoine.muesli";

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn suggest_start(ctx: &Ctx, meeting: DetectedMeeting) {
    let question = Question {
        title: format!("Looks like a meeting: {}", meeting.title),
        body: format!("Detected via {}. Start transcribing?", meeting.source),
        accept: "Start transcribing".into(),
        decline: "Not now".into(),
    };
    *ctx.app.pending.lock().unwrap() = Some(meeting);
    ask(ctx, "start", question);
}

pub fn suggest_stop(ctx: &Ctx) {
    let question = Question {
        title: "The call seems to have ended".into(),
        body: "Muesli is still recording. Stop and save the transcript?".into(),
        accept: "Stop and save".into(),
        decline: "Keep recording".into(),
    };
    ask(ctx, "stop", question);
}

fn ask(ctx: &Ctx, kind: &str, question: Question) {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    *ctx.app.prompt.lock().unwrap() =
        Some(Prompt { id, kind: kind.into(), title: question.title.clone(), body: question.body.clone() });
    let answer_ctx = ctx.clone();
    if muesli_notify::ask(APP_ID, &question, move |accept| answer(&answer_ctx, Some(id), accept)).is_err() {
        windows::show(&ctx.handle, "prompt"); // no notification with buttons here: our own window
    }
}

/// Acts on the answer. `id` is `None` for the window, which always shows the current prompt;
/// a notification answers only the prompt it was shown for.
pub fn answer(ctx: &Ctx, id: Option<u64>, accept: bool) {
    let prompt = {
        let mut current = ctx.app.prompt.lock().unwrap();
        match (&*current, id) {
            (Some(p), Some(id)) if p.id != id => return, // an old notification, answered late
            _ => current.take(),
        }
    };
    windows::close(&ctx.handle, "prompt");
    let Some(prompt) = prompt else { return };
    if !accept {
        return;
    }
    if prompt.kind == "start" {
        if let Some(meeting) = ctx.app.pending.lock().unwrap().take() {
            session::start(ctx, meeting);
        }
    } else {
        session::stop(ctx);
    }
}
