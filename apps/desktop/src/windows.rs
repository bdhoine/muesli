//! Muesli's windows. One page (`ui/index.html`) shows whichever view the window's label names.

use muesli_engine::Window;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Opens (or focuses) a window.
pub fn show(handle: &AppHandle, window: Window) {
    let label = window.label();
    if let Some(existing) = handle.get_webview_window(label) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return;
    }
    let (title, width, height) = match window {
        Window::Main => ("Muesli", 980.0, 640.0),
        Window::Settings => ("Muesli Settings", 620.0, 720.0),
        Window::Setup => ("Welcome to Muesli", 560.0, 600.0),
        Window::Prompt => ("Muesli", 380.0, 150.0),
    };
    let mut builder = WebviewWindowBuilder::new(handle, label, WebviewUrl::App("index.html".into())).title(title).inner_size(width, height);
    match window {
        Window::Prompt => {
            builder = builder.decorations(false).always_on_top(true).resizable(false).skip_taskbar(true).focused(false);
            if let Ok(Some(monitor)) = handle.primary_monitor() {
                let screen = monitor.size().to_logical::<f64>(monitor.scale_factor());
                builder = builder.position(screen.width - width - 20.0, 48.0);
            }
        }
        Window::Setup => builder = builder.resizable(false).center(),
        Window::Settings => builder = builder.min_inner_size(520.0, 480.0).center(),
        Window::Main => builder = builder.min_inner_size(720.0, 440.0).center(),
    }
    if let Ok(w) = builder.build() {
        let _ = w.set_focus();
    }
}

pub fn close(handle: &AppHandle, window: Window) {
    if let Some(w) = handle.get_webview_window(window.label()) {
        let _ = w.close();
    }
}
