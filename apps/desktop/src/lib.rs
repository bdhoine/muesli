mod bridge;
mod commands;
mod ctx;
mod files;
mod prompt;
mod session;
mod state;
mod tray;
mod windows;

use std::time::Duration;

use std::sync::Arc;

use muesli_core::ports::{Platform, Tray};
use muesli_core::store::MeetingStore;
use muesli_core::{DetectionEvent, DetectionLogic};
use tauri::RunEvent;

use crate::ctx::Ctx;
use crate::state::App;

/// The adapters for this OS; the shell draws the tray.
fn platform(tray: Arc<dyn Tray>) -> Platform {
    #[cfg(target_os = "macos")]
    return muesli_platform_macos::platform(tray);
    #[cfg(windows)]
    return muesli_platform_windows::platform(tray);
    // Linux, for development only: real folders and clock, fakes for audio, detection,
    // notifications and permissions.
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let root = std::env::temp_dir().join("muesli-dev");
        let mut p = muesli_platform_fake::FakePlatform::new(&root).platform();
        p.clock = Arc::new(muesli_platform_common::SystemClock);
        p.paths = Arc::new(muesli_platform_common::SystemPaths);
        p.tray = tray;
        p
    }
}

pub fn run() {
    let tray = Arc::new(tray::TauriTray::default());
    let app_state = App::new(platform(tray.clone()));

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|handle, _, _| windows::show(handle, "settings")))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(app_state.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::update_prefs,
            commands::download_model,
            commands::cancel_download,
            commands::transcribe_files,
            commands::cancel_file,
            commands::cancel_all_files,
            commands::close_file_window,
            commands::dismiss_file_error,
            commands::choose_drop_folder,
            commands::check_connection,
            commands::remove_legacy_models,
            commands::open_models_folder,
            commands::use_system_languages,
            commands::set_start_at_login,
            commands::choose_model_file,
            commands::choose_transcripts_folder,
            commands::start_recording,
            commands::stop_recording,
            commands::dismiss,
            commands::get_prompt,
            commands::prompt_response,
            commands::finish_onboarding,
            commands::request_permission,
            commands::open_privacy_settings,
            commands::open_repository,
            commands::open_path,
            commands::install_extension,
            commands::show_window,
            commands::close_window,
        ])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // menu bar only, no Dock icon
            let ctx = Ctx { app: app_state.clone(), handle: app.handle().clone() };
            tray.attach(app.handle())?;
            boot(&ctx);
            Ok(())
        });

    builder.build(tauri::generate_context!()).expect("error while building Muesli").run(|_handle, event| {
        // Closing the last window must not quit a tray app.
        if let RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}

fn boot(ctx: &Ctx) {
    let app = &ctx.app;
    if !cfg!(debug_assertions) && !app.prefs().onboarding_done {
        let _ = app.platform.login_item.set_enabled(true); // start at login unless switched off in Settings
    }

    let paths = app.paths();
    let store = MeetingStore::new(&paths);
    store.recover_interrupted();
    store.import_transcripts(&app.paths().transcripts);
    *app.last_ready.lock().unwrap() =
        store.latest_ready().map(|m| muesli_core::status::LastReady { id: m.id, title: m.title, ended_at: m.ended_at });
    bridge::apply(app, &app.prefs());

    tray::refresh(ctx);

    // Download the speech model and the voice activity model on the first start, and whenever
    // one of them went missing.
    let c = ctx.clone();
    app.model.ensure_ready(move || c.changed());
    if !app.prefs().onboarding_done {
        windows::show(&ctx.handle, "onboarding");
    } else if app.model.is_downloading() {
        windows::show(&ctx.handle, "settings");
    }
    ctx.changed();

    // state.json is the heartbeat other processes read; keep it fresh.
    let c = ctx.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(10));
        c.heartbeat();
    });

    // The drop folder, every few seconds (nothing happens until the user has chosen one).
    let c = ctx.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(5));
        if c.app.prefs().onboarding_done {
            files::poll_folder(&c);
        }
    });

    // Meeting detection, every few seconds, once onboarding is done.
    let c = ctx.clone();
    std::thread::spawn(move || {
        let mut detector = DetectionLogic::default();
        let (clock, sampler) = (c.app.platform.clock.clone(), c.app.platform.detector.clone());
        loop {
            std::thread::sleep(Duration::from_secs(5));
            if !c.app.prefs().onboarding_done {
                continue;
            }
            detector.end_delay = chrono::Duration::seconds(c.app.prefs().end_delay_seconds as i64);
            let now = clock.now();
            let events = detector.tick_sample(now, c.app.is_recording(), sampler.sample(now));
            *c.app.detected.lock().unwrap() = detector.in_progress.clone();
            for event in events {
                match event {
                    DetectionEvent::Suggest(m) => suggest(&c, m),
                    DetectionEvent::CallEnded => call_ended(&c),
                    DetectionEvent::InProgressChanged => c.changed(),
                }
            }
        }
    });
}

fn suggest(ctx: &Ctx, meeting: muesli_core::DetectedMeeting) {
    if ctx.app.is_idle() && ctx.app.model.is_ready() {
        prompt::suggest_start(ctx, meeting);
    }
}

fn call_ended(ctx: &Ctx) {
    if !ctx.app.is_recording() {
        return;
    }
    if ctx.app.prefs().auto_stop {
        session::stop(ctx);
    } else {
        prompt::suggest_stop(ctx);
    }
}
