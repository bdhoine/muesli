#![allow(clippy::field_reassign_with_default)] // tests read better one field at a time

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use muesli_core::whisper_model::{DEFAULT_MODEL, LEGACY_FILES, VAD_MODEL};
use muesli_core::{Paths, Preferences};
use muesli_whisper::{check_reachable, download, DownloadError, DownloadSpec, ModelManager, Phase};

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

fn manager(dir: &std::path::Path, prefs: Preferences) -> (ModelManager, Paths) {
    let paths = Paths::new(dir, dir.join("t"));
    (ModelManager::new(paths.clone(), prefs), paths)
}

fn spec_to(url: String, dest: std::path::PathBuf, bytes: u64) -> DownloadSpec {
    DownloadSpec { url, dest, min_bytes: 1, approximate_bytes: bytes }
}

#[test]
fn what_is_missing_is_the_voice_model_first_and_then_the_speech_model() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = manager(dir.path(), Preferences::default());
    assert_eq!(m.phase(), Phase::Missing);
    let urls: Vec<String> = m.missing_downloads().into_iter().map(|s| s.url).collect();
    assert_eq!(urls, [VAD_MODEL.url, DEFAULT_MODEL.url]);
}

#[test]
fn the_speech_model_is_only_ready_together_with_the_voice_model() {
    let dir = tempfile::tempdir().unwrap();
    let (m, paths) = manager(dir.path(), Preferences::default());
    std::fs::write(DEFAULT_MODEL.path(&paths), vec![0u8; DEFAULT_MODEL.minimum_valid_bytes() as usize]).unwrap();
    assert_eq!(m.phase(), Phase::Missing); // the small voice file is still missing
    assert_eq!(m.missing_downloads().len(), 1);
    std::fs::write(VAD_MODEL.path(&paths), vec![0u8; VAD_MODEL.minimum_valid_bytes() as usize]).unwrap();
    assert_eq!(m.phase(), Phase::Ready(DEFAULT_MODEL.path(&paths)));
    assert!(m.is_ready() && m.missing_downloads().is_empty());
}

#[test]
fn a_custom_model_replaces_the_speech_download_but_not_the_voice_model() {
    let dir = tempfile::tempdir().unwrap();
    let custom = dir.path().join("mine.bin");
    let mut prefs = Preferences::default();
    prefs.custom_model_path = Some(custom.to_string_lossy().into_owned());
    let (m, paths) = manager(dir.path(), prefs);
    assert!(matches!(m.phase(), Phase::Failed(_))); // the file is not there yet
    std::fs::write(&custom, b"model").unwrap();
    assert_eq!(m.phase(), Phase::Missing); // still needs the voice model
    assert_eq!(m.missing_downloads().len(), 1);
    assert_eq!(m.missing_downloads()[0].url, VAD_MODEL.url);
    std::fs::write(VAD_MODEL.path(&paths), vec![0u8; VAD_MODEL.minimum_valid_bytes() as usize]).unwrap();
    assert_eq!(m.phase(), Phase::Ready(custom));
    m.ensure_ready(|| {});
    assert!(!m.is_downloading());
}

#[test]
fn downloads_run_in_order_and_progress_spans_all_files() {
    let dir = tempfile::tempdir().unwrap();
    let (m, paths) = manager(dir.path(), Preferences::default());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let probe = m.clone();
    let specs = vec![
        spec_to(serve("200 OK", vec![1u8; 1_000_000], None), VAD_MODEL.path(&paths), 1_000_000),
        spec_to(serve("200 OK", vec![2u8; 2_000_000], None), DEFAULT_MODEL.path(&paths), 2_000_000),
    ];
    m.download_all(specs, move || {
        if let Some(p) = probe.download_progress() {
            sink.lock().unwrap().push(p);
        }
    });
    assert!(matches!(m.phase(), Phase::Downloading { total: 3_000_000, .. }));
    m.wait();
    let seen = seen.lock().unwrap();
    assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "progress never goes backwards: {seen:?}");
    assert_eq!(seen.last().unwrap().1, 3_000_000);
    assert_eq!(std::fs::metadata(VAD_MODEL.path(&paths)).unwrap().len(), 1_000_000);
    assert_eq!(std::fs::metadata(DEFAULT_MODEL.path(&paths)).unwrap().len(), 2_000_000);
}

#[test]
fn failures_are_remembered_until_the_next_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let (m, paths) = manager(dir.path(), Preferences::default());
    let url = serve("500 Internal Server Error", vec![], None);
    m.download_all(vec![spec_to(url, VAD_MODEL.path(&paths), 1)], || {});
    m.wait();
    assert!(matches!(m.phase(), Phase::Failed(e) if e.contains("500")));
    assert!(m.last_error().unwrap().contains("500"));
    assert!(m.last_url().unwrap().ends_with("/model.bin")); // the address that failed is kept for the UI
    let url = serve("200 OK", vec![1u8; 1000], None);
    m.download_all(vec![spec_to(url, VAD_MODEL.path(&paths), 1000)], || {});
    assert_eq!(m.last_error(), None); // a new attempt clears the old error
    m.wait();
}

/// A slow server: sends the body in pieces so a download can be cancelled while it runs.
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
fn cancel_takes_effect_at_once_and_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let (m, paths) = manager(dir.path(), Preferences::default());
    m.download_all(vec![spec_to(serve_slowly(5_000_000), DEFAULT_MODEL.path(&paths), 5_000_000)], || {});
    std::thread::sleep(std::time::Duration::from_millis(60));
    assert!(m.is_downloading());
    m.cancel();
    assert!(!m.is_downloading());
    assert_eq!(m.phase(), Phase::Missing);
    std::thread::sleep(std::time::Duration::from_millis(200)); // let the cancelled thread clean up
    assert!(std::fs::read_dir(dir.path()).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().contains("ggml")));
    // A new download can start right after a cancel.
    m.download_all(vec![spec_to(serve("200 OK", vec![1u8; 1000], None), VAD_MODEL.path(&paths), 1000)], || {});
    m.wait();
    assert!(VAD_MODEL.path(&paths).exists());
}

#[test]
fn a_second_start_while_downloading_does_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (m, paths) = manager(dir.path(), Preferences::default());
    m.download_all(vec![spec_to(serve_slowly(1_000_000), VAD_MODEL.path(&paths), 1_000_000)], || {});
    m.download_all(vec![spec_to("http://127.0.0.1:9/none".into(), DEFAULT_MODEL.path(&paths), 1)], || {});
    m.wait();
    assert!(m.last_error().is_none());
    assert!(VAD_MODEL.path(&paths).exists());
}

#[test]
fn partial_files_from_an_interrupted_run_are_cleared() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("ggml-large-v3-turbo-q5_0.bin.123-0.part"), b"half").unwrap();
    std::fs::write(dir.path().join("config.json"), b"{}").unwrap();
    let _ = ModelManager::new(Paths::new(dir.path(), dir.path().join("t")), Preferences::default());
    let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(names, ["config.json"]);
}

#[test]
fn old_model_files_can_be_listed_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = manager(dir.path(), Preferences::default());
    std::fs::write(dir.path().join(LEGACY_FILES[1]), vec![0u8; 7]).unwrap();
    assert_eq!(m.legacy_models().1, 7);
    assert_eq!(m.remove_legacy_models(), 1);
    assert_eq!(m.legacy_models(), (vec![], 0));
}

#[test]
fn a_server_that_cannot_be_reached_fails_with_the_address() {
    let dir = tempfile::tempdir().unwrap();
    let (m, paths) = manager(dir.path(), Preferences::default());
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/model.bin", closed.local_addr().unwrap());
    drop(closed); // nothing listens here any more
    m.download_all(vec![spec_to(url.clone(), VAD_MODEL.path(&paths), 1)], || {});
    m.wait();
    assert!(matches!(m.phase(), Phase::Failed(e) if e.contains("Could not connect")));
    assert_eq!(m.last_url(), Some(url));
}

#[test]
fn the_connection_check_reports_each_address() {
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/model.bin", closed.local_addr().unwrap());
    drop(closed);
    let reach = check_reachable(&url);
    assert!(!reach.ok && reach.url == url && !reach.detail.is_empty());

    let up = serve("200 OK", vec![], None);
    let reach = check_reachable(&up);
    assert!(reach.ok, "{}", reach.detail);
    assert!(reach.detail.contains("200"));
}

#[test]
fn the_downloads_cover_the_addresses_the_check_asks() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = manager(dir.path(), Preferences::default());
    // The check itself needs the internet, so only the addresses it would ask are compared.
    let wanted: Vec<String> = m.missing_downloads().into_iter().map(|s| s.url).collect();
    assert_eq!(wanted, vec![VAD_MODEL.url.to_string(), DEFAULT_MODEL.url.to_string()]);
}
