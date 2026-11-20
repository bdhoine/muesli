//! The Tauri shell: picks the adapters for this OS, runs the engine, draws the tray and the
//! windows. Everything Muesli decides is in `muesli-engine`; keep this thin.

mod host;
mod speech;
mod tray;
mod updater;
mod windows;

use std::sync::Arc;
use std::time::Duration;

use muesli_core::ports::{Platform, Tray};
use muesli_core::{languages, Paths};
use muesli_engine::{Engine, Intent, Options, Window};
use tauri::{DragDropEvent, Manager, RunEvent, State, WindowEvent};

use crate::host::TauriHost;

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

type Shared<'a> = State<'a, Arc<Engine>>;

/// A window's view state. The page knows which window it is from its label.
#[tauri::command]
fn view(engine: Shared, window: String) -> serde_json::Value {
    Window::from_label(&window).map(|w| engine.view(w)).unwrap_or(serde_json::Value::Null)
}

/// What a window asks the engine to do.
#[tauri::command]
fn intent(engine: Shared, intent: Intent) {
    let engine = engine.inner().clone();
    // Intents may wait on files or threads; never on the UI thread.
    std::thread::spawn(move || engine.intent(intent));
}

pub fn run() {
    let tray = Arc::new(tray::TauriTray::default());
    let host = Arc::new(TauriHost::default());
    let platform = platform(tray.clone());
    let speech = Arc::new(speech::WhisperSpeech::new(Paths::from_platform(&*platform.paths)));
    let context = tauri::generate_context!();
    // Release builds made with tauri.release.conf.json carry the updater's settings; debug builds
    // and builds without them never check for updates.
    let updates = !cfg!(debug_assertions) && context.config().plugins.0.contains_key("updater");
    let updater = Arc::new(updater::TauriUpdater::default());
    let mut options = Options::new(languages::system_defaults());
    if updates {
        options = options.with_updater(updater.clone());
    }
    let engine = Engine::new(platform, speech, host.clone(), options);

    let for_setup = (engine.clone(), host.clone());
    let for_second = engine.clone();
    let for_drops = engine.clone();
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(move |_, _, _| for_second.intent(Intent::OpenMuesli)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());
    if updates {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }
    builder
        .manage(engine.clone())
        .invoke_handler(tauri::generate_handler![view, intent])
        // Audio files dropped on Muesli's window are transcribed, like files dropped on the
        // menu bar icon (macOS). Windows' tray icons cannot take drops; see docs/PLATFORMS.md.
        .on_window_event(move |window, event| match event {
            WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
                let (engine, paths) = (for_drops.clone(), paths.clone());
                std::thread::spawn(move || engine.intent(Intent::TranscribeFiles { paths }));
            }
            WindowEvent::Destroyed => windows::closed(window.app_handle(), window.label()),
            _ => {}
        })
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory); // menu bar only, no Dock icon
            let (engine, host) = for_setup;
            host.attach(app.handle(), &engine);
            updater.attach(app.handle());
            tray.attach(app.handle(), host.clone())?;
            engine.start();
            let ticking = engine.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(1));
                ticking.tick();
            });
            Ok(())
        })
        .build(context)
        .expect("error while building Muesli")
        .run(move |_handle, event| match event {
            // Closing the last window must not quit a tray app.
            RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
            RunEvent::Exit => engine.shutdown(),
            _ => {}
        });
}
