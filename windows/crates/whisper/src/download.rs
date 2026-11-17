use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DownloadSpec {
    pub url: String,
    pub dest: PathBuf,
    /// Anything smaller is rejected as an incomplete or wrong file.
    pub min_bytes: u64,
    /// Used for progress until the server says how large the file is.
    pub approximate_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    Cancelled,
    Http(u16),
    Incomplete,
    Failed(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "The download was cancelled."),
            Self::Http(code) => write!(f, "The download server answered HTTP {code}."),
            Self::Incomplete => write!(f, "The download is incomplete. Try again."),
            Self::Failed(why) => write!(f, "Download failed: {why}"),
        }
    }
}

/// One agent for downloads and checks, with timeouts on connecting and on the first answer, so an
/// unreachable server fails with a message instead of leaving the download hanging at 0 MB. The
/// body has no overall limit: the model is large and the connection may be slow.
fn agent() -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_resolve(Some(Duration::from_secs(10)))
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .build(),
    )
}

/// The outcome of asking one server whether a file can be fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reach {
    pub url: String,
    pub ok: bool,
    /// For people: "Reachable (HTTP 200)" or why not.
    pub detail: String,
}

/// Asks the server for the file's headers only (nothing is downloaded).
pub fn check_reachable(url: &str) -> Reach {
    let (ok, detail) = match agent().head(url).call() {
        Ok(r) => (true, format!("Reachable (HTTP {})", r.status().as_u16())),
        Err(ureq::Error::StatusCode(code)) => (false, format!("The server answered HTTP {code}")),
        Err(e) => (false, e.to_string()),
    };
    Reach { url: url.to_string(), ok, detail }
}

/// Downloads into `<dest>.part`, then moves the file into place only when it is complete.
/// `progress(received, total)` is called about once per megabyte.
pub fn download(
    spec: &DownloadSpec,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<(), DownloadError> {
    // A name of its own, so a download that was replaced cannot disturb its successor's file.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut name = spec.dest.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}-{n}.part", std::process::id()));
    let part = spec.dest.with_file_name(name);
    let result = run(spec, &part, progress, cancel);
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

fn run(
    spec: &DownloadSpec,
    part: &std::path::Path,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<(), DownloadError> {
    if let Some(dir) = spec.dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| DownloadError::Failed(e.to_string()))?;
    }
    let response = match agent().get(&spec.url).call() {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(code)) => return Err(DownloadError::Http(code)),
        Err(e) => return Err(DownloadError::Failed(format!("Could not connect: {e}"))),
    };
    let expected = response.body().content_length();
    let total = expected.unwrap_or(spec.approximate_bytes);
    let mut reader = response.into_body().into_reader();
    let mut file = File::create(part).map_err(|e| DownloadError::Failed(format!("Could not save the model: {e}")))?;

    let mut buf = vec![0u8; 64 * 1024];
    let (mut received, mut reported) = (0u64, 0u64);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::Cancelled);
        }
        let n = reader.read(&mut buf).map_err(|e| DownloadError::Failed(e.to_string()))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| DownloadError::Failed(format!("Could not save the model: {e}")))?;
        received += n as u64;
        if received / 1_000_000 != reported / 1_000_000 {
            reported = received;
            progress(received, total);
        }
    }
    file.flush().map_err(|e| DownloadError::Failed(e.to_string()))?;
    drop(file);

    if received < spec.min_bytes || expected.is_some_and(|e| e != received) {
        return Err(DownloadError::Incomplete);
    }
    progress(received, total);
    let _ = std::fs::remove_file(&spec.dest);
    std::fs::rename(part, &spec.dest).map_err(|e| DownloadError::Failed(format!("Could not save the model: {e}")))
}
