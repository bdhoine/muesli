//! One recording: microphone and system audio in, Whisper transcript out, saved as it goes.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use muesli_audio::AudioCapture;
use muesli_core::status::LastReady;
use muesli_core::store::MeetingStore;
use muesli_core::{languages, AudioChunk, DetectedMeeting, Meeting, MeetingState};
use muesli_whisper::Transcriber;

use crate::ctx::Ctx;
use crate::state::{Session, Status};

/// Starts recording in the background. Does nothing unless Muesli is idle and a model is ready.
pub fn start(ctx: &Ctx, detected: DetectedMeeting) {
    if !ctx.app.is_idle() || !ctx.app.model.is_ready() {
        return;
    }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        if let Err(message) = run_start(&ctx, detected) {
            ctx.set_status(Status::Error { message });
        }
    });
}

fn run_start(ctx: &Ctx, detected: DetectedMeeting) -> Result<(), String> {
    let prefs = ctx.app.prefs();
    let model_path = prefs.model_path(&ctx.app.paths());
    let system_defaults = languages::system_defaults();
    let vad_path = prefs.vad_model_path(&ctx.app.paths());
    let transcriber = Transcriber::new(&model_path, Some(&vad_path), prefs.effective_languages(&system_defaults))
        .ok_or_else(|| format!("The Whisper model could not be loaded: {}", model_path.display()))?;

    *ctx.app.capture_error.lock().unwrap() = None;
    let mut meeting = Meeting::new(&detected.title, &detected.source);
    meeting.attendees = detected.attendees.clone();
    meeting.state = MeetingState::Recording;
    let paths = ctx.app.paths();
    let store = MeetingStore::new(&paths);
    store.save(&meeting);
    let meeting = Arc::new(Mutex::new(meeting));

    // Chunks go to one consumer, so a single Whisper state is never used concurrently.
    let (chunk_tx, chunk_rx) = mpsc::channel::<AudioChunk>();
    let consumer = {
        let (ctx, meeting) = (ctx.clone(), meeting.clone());
        std::thread::spawn(move || {
            for chunk in chunk_rx {
                if let Ok(segments) = transcriber.transcribe(&chunk) {
                    if segments.is_empty() {
                        continue;
                    }
                    MeetingStore::new(&ctx.app.paths()).append(&mut meeting.lock().unwrap(), segments);
                    ctx.changed();
                }
            }
        })
    };

    // Audio streams are not `Send` on every platform, so one thread owns them.
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<Result<(), String>>();
    let error_ctx = ctx.clone();
    let audio = std::thread::spawn(move || {
        let mut capture = AudioCapture::new(
            move |chunk| drop(chunk_tx.send(chunk)),
            // A stream broke for good (device unplugged, permission revoked): keep what we have.
            move |message| {
                *error_ctx.app.capture_error.lock().unwrap() = Some(message);
                stop(&error_ctx);
            },
        );
        match capture.start() {
            Ok(()) => {
                let _ = started_tx.send(Ok(()));
                let _ = stop_rx.recv(); // until stop() or the sender is dropped
                capture.stop(); // emits the tails, then dropping `capture` closes the channel
            }
            Err(e) => {
                let _ = started_tx.send(Err(e.to_string()));
            }
        }
    });

    if let Err(message) = started_rx.recv().unwrap_or_else(|_| Err("Audio capture did not start.".into())) {
        let _ = audio.join();
        let _ = consumer.join();
        let mut failed = meeting.lock().unwrap().clone();
        failed.state = MeetingState::Failed;
        store.save(&failed);
        return Err(format!("Capture failed: {message}"));
    }

    let title = meeting.lock().unwrap().title.clone();
    *ctx.app.session.lock().unwrap() = Some(Session { meeting, stop_audio: stop_tx, consumer: Some(consumer), audio: Some(audio) });
    ctx.set_status(Status::Recording { title });
    Ok(())
}

/// Stops, transcribes what is left, saves the transcript. Runs in the background.
pub fn stop(ctx: &Ctx) {
    if !ctx.app.is_recording() {
        return;
    }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let Some(mut session) = ctx.app.session.lock().unwrap().take() else { return };
        ctx.set_status(Status::Saving);
        {
            let mut m = session.meeting.lock().unwrap();
            m.state = MeetingState::Finalizing;
            MeetingStore::new(&ctx.app.paths()).save(&m);
        }
        let _ = session.stop_audio.send(());
        if let Some(audio) = session.audio.take() {
            let _ = audio.join();
        }
        if let Some(consumer) = session.consumer.take() {
            let _ = consumer.join(); // finishes the last chunks
        }
        let mut m = session.meeting.lock().unwrap().clone();
        MeetingStore::new(&ctx.app.paths()).finish(&mut m, Utc::now());
        *ctx.app.last_ready.lock().unwrap() = Some(LastReady { id: m.id.clone(), title: m.title.clone(), ended_at: m.ended_at });
        ctx.set_status(match ctx.app.capture_error.lock().unwrap().take() {
            Some(why) => Status::Error { message: format!("Capture stopped: {why} The transcript so far was saved.") },
            None => Status::Idle,
        });
    });
}
