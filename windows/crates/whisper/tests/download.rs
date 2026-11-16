#![allow(clippy::field_reassign_with_default)] // tests read better one field at a time

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use muesli_core::{Paths, Preferences, WhisperModel};
use muesli_whisper::{download, DownloadError, DownloadSpec, ModelManager, Phase};

/// Serves `body` once with the given status line and optional lying Content-Length.
fn serve(status: &'static str, body: Vec<u8>, content_length: Option<usize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/model.bin", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let length = content_length.unwrap_or(body.len());
            let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n");
            let _ = stream.write_all(&body);
        }
    });
    url
}

fn spec(url: String, dir: &std::path::Path, min: u64) -> DownloadSpec {
    DownloadSpec { url, dest: dir.join("ggml-small.bin"), min_bytes: min, approximate_bytes: 3_000_000 }
}

#[test]
fn downloads_validates_and_moves_into_place() {
    let dir = tempfile::tempdir().unwrap();
    let url = serve("200 OK", vec![7u8; 3_000_000], None);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    download(&spec(url, dir.path(), 2_000_000), &mut |r, t| sink.lock().unwrap().push((r, t)), &AtomicBool::new(false)).unwrap();
    assert_eq!(std::fs::metadata(dir.path().join("ggml-small.bin")).unwrap().len(), 3_000_000);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1); // no .part left
    let seen = seen.lock().unwrap();
    assert!(seen.len() >= 2);
    assert_eq!(*seen.last().unwrap(), (3_000_000, 3_000_000));
}

#[test]
fn rejects_http_errors_and_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let url = serve("404 Not Found", b"nope".to_vec(), None);
    let err = download(&spec(url, dir.path(), 1), &mut |_, _| {}, &AtomicBool::new(false)).unwrap_err();
    assert_eq!(err, DownloadError::Http(404));
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
}

#[test]
fn rejects_files_that_are_too_small() {
    let dir = tempfile::tempdir().unwrap();
    let url = serve("200 OK", vec![1u8; 1000], None);
    let err = download(&spec(url, dir.path(), 2_000_000), &mut |_, _| {}, &AtomicBool::new(false)).unwrap_err();
    assert_eq!(err, DownloadError::Incomplete);
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
}

#[test]
fn a_cancelled_download_leaves_no_partial_file() {
    let dir = tempfile::tempdir().unwrap();
    let url = serve("200 OK", vec![1u8; 3_000_000], None);
    let cancel = AtomicBool::new(false);
    let err = download(&spec(url, dir.path(), 1), &mut |_, _| cancel.store(true, Ordering::Relaxed), &cancel).unwrap_err();
    assert_eq!(err, DownloadError::Cancelled);
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
}

#[test]
fn manager_reports_missing_downloading_and_ready() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path(), dir.path().join("t"));
    let manager = ModelManager::new(paths.clone(), Preferences::default());
    assert_eq!(manager.phase(), Phase::Missing(WhisperModel::Small));
    assert!(!manager.is_ready());

    let url = serve("200 OK", vec![9u8; 3_000_000], None);
    let mut spec = spec(url, dir.path(), 2_000_000);
    spec.dest = WhisperModel::Small.path(&paths);
    manager.download_spec(WhisperModel::Small, spec, || {});
    assert!(manager.is_downloading() || manager.is_ready());
    manager.wait();
    // The test file is far smaller than a real model, so it does not count as installed.
    assert_eq!(manager.phase(), Phase::Missing(WhisperModel::Small));
}

#[test]
fn a_custom_model_replaces_the_download() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path(), dir.path().join("t"));
    let custom = dir.path().join("mine.bin");
    let mut prefs = Preferences::default();
    prefs.custom_model_path = Some(custom.to_string_lossy().into_owned());
    let manager = ModelManager::new(paths, prefs.clone());
    assert!(matches!(manager.phase(), Phase::Failed(_)));
    std::fs::write(&custom, b"model").unwrap();
    assert_eq!(manager.phase(), Phase::Ready(custom));
    manager.download_selected_if_needed(|| {});
    assert!(!manager.is_downloading()); // a custom model never triggers a download
}

#[test]
fn failures_are_remembered_until_the_next_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path(), dir.path().join("t"));
    let manager = ModelManager::new(paths.clone(), Preferences::default());
    let mut s = spec(serve("500 Internal Server Error", vec![], None), dir.path(), 1);
    s.dest = WhisperModel::Small.path(&paths);
    manager.download_spec(WhisperModel::Small, s, || {});
    manager.wait();
    assert!(matches!(manager.phase(), Phase::Failed(m) if m.contains("500")));
}

/// A slow server: sends the body in pieces so a download can be replaced while it runs.
fn serve_slowly(body_len: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/model.bin", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n");
            for _ in 0..body_len / 100_000 {
                if stream.write_all(&[1u8; 100_000]).is_err() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
        }
    });
    url
}

#[test]
fn starting_another_model_replaces_the_running_download() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path(), dir.path().join("t"));
    let mut prefs = Preferences::default();
    let manager = ModelManager::new(paths.clone(), prefs.clone());

    let mut slow = spec(serve_slowly(5_000_000), dir.path(), 1);
    slow.dest = WhisperModel::Small.path(&paths);
    manager.download_spec(WhisperModel::Small, slow, || {});
    std::thread::sleep(std::time::Duration::from_millis(60));
    assert!(matches!(manager.phase(), Phase::Downloading { model: WhisperModel::Small, .. }));

    // The user picks Medium while Small is still coming in: no waiting, no timer.
    prefs.model = WhisperModel::Medium;
    manager.apply(prefs);
    let mut fast = spec(serve("200 OK", vec![2u8; 2_000_000], None), dir.path(), 1);
    fast.dest = WhisperModel::Medium.path(&paths);
    manager.download_spec(WhisperModel::Medium, fast, || {});
    assert!(matches!(manager.phase(), Phase::Downloading { model: WhisperModel::Medium, .. }));
    manager.wait();
    assert_eq!(std::fs::metadata(WhisperModel::Medium.path(&paths)).unwrap().len(), 2_000_000);
    std::thread::sleep(std::time::Duration::from_millis(150)); // let the cancelled one clean up
    assert!(!WhisperModel::Small.path(&paths).exists());
    let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".part")).collect();
    assert!(leftovers.is_empty());
}

#[test]
fn cancel_takes_effect_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path(), dir.path().join("t"));
    let manager = ModelManager::new(paths.clone(), Preferences::default());
    let mut slow = spec(serve_slowly(5_000_000), dir.path(), 1);
    slow.dest = WhisperModel::Small.path(&paths);
    manager.download_spec(WhisperModel::Small, slow, || {});
    manager.cancel();
    assert!(!manager.is_downloading());
    assert_eq!(manager.phase(), Phase::Missing(WhisperModel::Small));
}

#[test]
fn partial_files_from_an_interrupted_run_are_cleared() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("ggml-small.bin.123-0.part"), b"half").unwrap();
    std::fs::write(dir.path().join("config.json"), b"{}").unwrap();
    let _ = ModelManager::new(Paths::new(dir.path(), dir.path().join("t")), Preferences::default());
    let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(names, ["config.json"]);
}
