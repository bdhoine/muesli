#![allow(clippy::field_reassign_with_default)] // tests read better one field at a time

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use muesli_core::whisper_model::{coreml_installed, COREML_ENCODER, DEFAULT_MODEL, LEGACY_FILES, USES_COREML, VAD_MODEL};
use muesli_core::Paths;
use muesli_whisper::{check_reachable, download, DownloadError, DownloadSpec, Extra, ModelManager, Phase};

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
    DownloadSpec { url, dest: dir.join("ggml-small.bin"), min_bytes: min, approximate_bytes: 3_000_000, unpack: false }
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

fn manager(dir: &std::path::Path) -> (ModelManager, Paths) {
    let paths = Paths::new(dir, dir.join("t"));
    (ModelManager::new(paths.clone()), paths)
}

fn spec_to(url: String, dest: std::path::PathBuf, bytes: u64) -> DownloadSpec {
    DownloadSpec { url, dest, min_bytes: 1, approximate_bytes: bytes, unpack: false }
}

#[test]
fn what_is_missing_is_the_voice_model_first_and_then_the_speech_model() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = manager(dir.path());
    assert_eq!(m.phase(), Phase::Missing);
    let urls: Vec<String> = m.missing_downloads().into_iter().map(|s| s.url).collect();
    assert_eq!(urls, all_urls());
}

/// The voice model, the speech model and, on a Mac, the Core ML encoder last.
fn all_urls() -> Vec<String> {
    let mut urls = vec![VAD_MODEL.url.to_string(), DEFAULT_MODEL.url.to_string()];
    if USES_COREML {
        urls.push(COREML_ENCODER.url.to_string());
    }
    urls
}

#[test]
fn the_speech_model_is_only_ready_together_with_the_voice_model() {
    let dir = tempfile::tempdir().unwrap();
    let (m, paths) = manager(dir.path());
    std::fs::write(DEFAULT_MODEL.path(&paths), vec![0u8; DEFAULT_MODEL.minimum_valid_bytes() as usize]).unwrap();
    assert_eq!(m.phase(), Phase::Missing); // the small voice file is still missing
    assert_eq!(m.missing_downloads().len(), 1 + USES_COREML as usize);
    std::fs::write(VAD_MODEL.path(&paths), vec![0u8; VAD_MODEL.minimum_valid_bytes() as usize]).unwrap();
    // Ready: the Core ML encoder (macOS) only speeds transcribing up and never holds it back.
    assert_eq!(m.phase(), Phase::Ready(DEFAULT_MODEL.path(&paths)));
    assert!(m.is_ready());
    assert_eq!(m.missing_downloads().len(), USES_COREML as usize);
    assert_eq!(m.coreml(), USES_COREML.then_some(Extra::Waiting));
}

#[test]
fn downloads_run_in_order_and_progress_spans_all_files() {
    let dir = tempfile::tempdir().unwrap();
    let (m, paths) = manager(dir.path());
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
    let (m, paths) = manager(dir.path());
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
    let (m, paths) = manager(dir.path());
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
    let (m, paths) = manager(dir.path());
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
    let _ = ModelManager::new(Paths::new(dir.path(), dir.path().join("t")));
    let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(names, ["config.json"]);
}

#[test]
fn old_model_files_are_deleted_on_start() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(LEGACY_FILES[1]), vec![0u8; 7]).unwrap();
    let _ = manager(dir.path());
    assert!(!dir.path().join(LEGACY_FILES[1]).exists());
}

#[test]
fn a_server_that_cannot_be_reached_fails_with_the_address() {
    let dir = tempfile::tempdir().unwrap();
    let (m, paths) = manager(dir.path());
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
    let (m, _) = manager(dir.path());
    // The check itself needs the internet, so only the addresses it would ask are compared.
    let wanted: Vec<String> = m.missing_downloads().into_iter().map(|s| s.url).collect();
    assert_eq!(wanted, all_urls());
}

/// A zip like the Core ML encoder's: a folder with a file and a subfolder, and Finder's
/// `__MACOSX` leftovers.
fn encoder_zip() -> Vec<u8> {
    use zip::write::SimpleFileOptions;
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let deflated = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.add_directory("x.mlmodelc/", deflated).unwrap();
    zip.start_file("x.mlmodelc/coremldata.bin", deflated).unwrap();
    zip.write_all(&[7u8; 5000]).unwrap();
    zip.start_file("x.mlmodelc/weights/weight.bin", SimpleFileOptions::default()).unwrap();
    zip.write_all(b"weights").unwrap();
    zip.start_file("__MACOSX/x.mlmodelc/._coremldata.bin", deflated).unwrap();
    zip.write_all(b"junk").unwrap();
    zip.finish().unwrap();
    out.into_inner()
}

fn unpack_spec(url: String, dir: &std::path::Path, bytes: u64) -> DownloadSpec {
    DownloadSpec { url, dest: dir.join("x.mlmodelc.zip"), min_bytes: 1, approximate_bytes: bytes, unpack: true }
}

#[test]
fn a_zip_is_unpacked_into_place_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = manager(dir.path());
    let zip = encoder_zip();
    let url = serve("200 OK", zip.clone(), None);
    m.download_all(vec![unpack_spec(url, dir.path(), zip.len() as u64)], || {});
    m.wait();
    assert_eq!(m.last_error(), None);
    assert_eq!(std::fs::read(dir.path().join("x.mlmodelc/coremldata.bin")).unwrap(), vec![7u8; 5000]);
    assert_eq!(std::fs::read(dir.path().join("x.mlmodelc/weights/weight.bin")).unwrap(), b"weights");
    assert!(!dir.path().join("x.mlmodelc.zip").exists(), "the zip is deleted");
    assert!(!dir.path().join("__MACOSX").exists(), "Finder's leftovers are skipped");
    let leftovers: Vec<_> =
        std::fs::read_dir(dir.path()).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with(".unpack")).collect();
    assert!(leftovers.is_empty());
}

#[test]
fn a_zip_left_by_an_earlier_run_is_unpacked_without_downloading() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = manager(dir.path());
    std::fs::write(dir.path().join("x.mlmodelc.zip"), encoder_zip()).unwrap();
    // Nothing listens at this address: a download would fail.
    m.download_all(vec![unpack_spec("http://127.0.0.1:9/x.zip".into(), dir.path(), 1)], || {});
    m.wait();
    assert_eq!(m.last_error(), None);
    assert!(dir.path().join("x.mlmodelc/coremldata.bin").exists());
}

#[test]
fn a_broken_zip_fails_and_leaves_no_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _) = manager(dir.path());
    let url = serve("200 OK", b"not a zip at all".to_vec(), None);
    m.download_all(vec![unpack_spec(url, dir.path(), 16)], || {});
    m.wait();
    assert!(m.last_error().unwrap().contains("Not a valid archive"));
    assert!(!dir.path().join("x.mlmodelc").exists());
}

#[test]
fn the_core_ml_encoder_counts_as_installed_with_its_two_files() {
    let dir = tempfile::tempdir().unwrap();
    let (m, paths) = manager(dir.path());
    let folder = paths.models.join(muesli_core::whisper_model::COREML_DIR);
    std::fs::create_dir_all(folder.join("weights")).unwrap();
    std::fs::write(folder.join("coremldata.bin"), b"x").unwrap();
    assert!(!coreml_installed(&paths));
    std::fs::write(folder.join("weights/weight.bin"), b"x").unwrap();
    assert!(coreml_installed(&paths));
    assert_eq!(m.coreml(), USES_COREML.then_some(Extra::Ready));
}
