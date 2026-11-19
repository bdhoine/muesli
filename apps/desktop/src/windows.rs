use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Opens (or focuses) one of Muesli's windows. The page decides what to show from its label.
pub fn show(handle: &AppHandle, label: &str) {
    if let Some(window) = handle.get_webview_window(label) {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let (title, width, height) = match label {
        "onboarding" => ("Welcome to Muesli", 560.0, 640.0),
        "prompt" => ("Muesli", 380.0, 150.0),
        "transcribe" => ("Transcribing Audio", 460.0, 320.0),
        _ => ("Muesli Settings", 820.0, 600.0),
    };
    let mut builder = WebviewWindowBuilder::new(handle, label, WebviewUrl::App("index.html".into())).title(title).inner_size(width, height);
    if label == "prompt" {
        builder = builder.decorations(false).always_on_top(true).resizable(false).skip_taskbar(true).focused(false);
        if let Ok(Some(monitor)) = handle.primary_monitor() {
            let scale = monitor.scale_factor();
            let screen = monitor.size().to_logical::<f64>(scale);
            builder = builder.position(screen.width - width - 20.0, 48.0);
        }
    } else if label == "transcribe" || label == "onboarding" {
        builder = builder.resizable(false).center();
    } else {
        builder = builder.min_inner_size(640.0, 460.0).center();
    }
    if let Ok(window) = builder.build() {
        let _ = window.set_focus();
    }
}

pub fn close(handle: &AppHandle, label: &str) {
    if let Some(window) = handle.get_webview_window(label) {
        let _ = window.close();
    }
}
