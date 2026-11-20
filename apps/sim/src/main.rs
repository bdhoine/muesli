//! muesli-sim: the real engine with fake adapters, and the real web UI in a browser.
//!
//!     cargo run -p muesli-sim -- [--skin mac|windows] [--scenario a-day-of-meetings] [--port 7777]
//!
//! Open the printed address. The page shows the tray menu, notifications and Muesli's open
//! windows (each the real `ui/` page in a frame), with buttons that play the outside world: a call
//! starts, people talk, the call ends, time passes. Scenarios are JSON files in
//! `fixtures/scenarios/`. The UI tests (`ui/tests`) drive the same page.

mod http;
mod world;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use http::{Method, Request};
use muesli_engine::{Intent, Window};
use serde_json::{json, Value};
use world::{Scenario, World};

struct Sim {
    world: Mutex<World>,
    scenario: Mutex<String>,
    version: Arc<AtomicU64>,
    repo: PathBuf,
    /// Whether time runs by itself (one simulated second per real second).
    running: std::sync::atomic::AtomicBool,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("the repository")
}

fn scenarios(repo: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(repo.join("fixtures/scenarios"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
}

fn load_scenario(repo: &Path, name: &str) -> Result<Scenario, String> {
    if name.is_empty() || name.contains(['/', '\\', '.']) {
        return Err(format!("no scenario {name:?}"));
    }
    let text = std::fs::read_to_string(repo.join("fixtures/scenarios").join(format!("{name}.json"))).map_err(|e| format!("{name}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{name}: {e}"))
}

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let repo = repo_root();
    let skin = arg("--skin").unwrap_or_else(|| "mac".into());
    let scenario_name = arg("--scenario").unwrap_or_else(|| "a-day-of-meetings".into());
    let port = arg("--port").unwrap_or_else(|| "7777".into());
    let scenario = load_scenario(&repo, &scenario_name).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    let version = Arc::new(AtomicU64::new(1));
    let world = World::new(&skin, &scenario, version.clone(), repo.join("fixtures"));
    let sim = Arc::new(Sim {
        world: Mutex::new(world),
        scenario: Mutex::new(scenario_name),
        version,
        repo,
        running: std::sync::atomic::AtomicBool::new(arg("--paused").is_none() && !std::env::args().any(|a| a == "--paused")),
    });

    let listener = std::net::TcpListener::bind(format!("127.0.0.1:{port}")).expect("a free port");
    // Tests start the simulator on port 0 and read the address from this line.
    println!("muesli-sim listening on http://{}/", listener.local_addr().unwrap());

    let ticking = sim.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        if ticking.running.load(Ordering::SeqCst) {
            ticking.world.lock().unwrap().tick();
        }
    });

    for stream in listener.incoming().flatten() {
        let sim = sim.clone();
        std::thread::spawn(move || {
            if let Some(request) = Request::read(stream) {
                handle(&sim, request);
            }
        });
    }
}

fn json_reply(request: Request, value: Value) {
    request.respond(200, "application/json", value.to_string().as_bytes());
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css",
        Some("js") | Some("mjs") => "text/javascript",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}

fn file(request: Request, base: &Path, relative: &str) {
    file_with(request, base, relative, "")
}

fn file_with(request: Request, base: &Path, relative: &str, extra: &str) {
    if relative.split('/').any(|part| part == "..") {
        return request.respond(403, "text/plain", b"");
    }
    let path = base.join(relative);
    match std::fs::read(&path) {
        Ok(bytes) => request.respond_with(200, content_type(&path), &bytes, extra),
        Err(_) => request.respond(404, "text/plain", b""),
    }
}

/// The app's Content Security Policy (`apps/desktop/tauri.conf.json`), so the UI tests catch what
/// the app would block (a `data:` image, an inline script). The simulator's API is the page's own
/// origin, where the app uses `ipc:`.
fn app_csp(repo: &Path) -> Option<String> {
    let config: Value = serde_json::from_slice(&std::fs::read(repo.join("apps/desktop/tauri.conf.json")).ok()?).ok()?;
    let policy = config.pointer("/app/security/csp")?.as_str()?;
    Some(policy.replace("connect-src ", "connect-src 'self' "))
}

fn handle(sim: &Sim, request: Request) {
    let path = request.path.clone();
    match (request.method, path.as_str()) {
        (Method::Get, "/") => file(request, &sim.repo.join("apps/sim/web"), "sim.html"),
        (Method::Get, p) if p.starts_with("/sim/") => file(request, &sim.repo.join("apps/sim/web"), &p["/sim/".len()..]),
        (Method::Get, p) if p.starts_with("/ui/") => {
            let csp = app_csp(&sim.repo).map(|policy| format!("Content-Security-Policy: {policy}\r\n")).unwrap_or_default();
            file_with(request, &sim.repo.join("ui"), &p["/ui/".len()..], &csp)
        }
        (Method::Get, "/api/version") => json_reply(request, json!({ "version": sim.version.load(Ordering::SeqCst) })),
        (Method::Get, p) if p.starts_with("/api/view/") => {
            let window = Window::from_label(&p["/api/view/".len()..]);
            let engine = sim.world.lock().unwrap().engine.clone();
            json_reply(request, window.map(|w| engine.view(w)).unwrap_or(Value::Null));
        }
        (Method::Post, "/api/intent") => {
            let intent: Result<Intent, _> = serde_json::from_value(request.json());
            match intent {
                Ok(intent) => {
                    let engine = sim.world.lock().unwrap().engine.clone();
                    engine.intent(intent);
                    json_reply(request, json!({ "ok": true }));
                }
                Err(e) => request.respond(400, "text/plain", e.to_string().as_bytes()),
            }
        }
        (Method::Get, "/api/sim") => {
            let world = sim.world.lock().unwrap();
            let mut state = world.state();
            state["scenarios"] = json!(scenarios(&sim.repo));
            state["scenario"] = json!(sim.scenario.lock().unwrap().clone());
            state["running"] = json!(sim.running.load(Ordering::SeqCst));
            drop(world);
            json_reply(request, state);
        }
        (Method::Post, "/api/sim") => {
            let step = request.json();
            let result = match step.get("do").and_then(Value::as_str) {
                Some("reset") => {
                    let skin = step.get("skin").and_then(Value::as_str).unwrap_or("mac").to_string();
                    let name = step.get("scenario").and_then(Value::as_str).unwrap_or("a-day-of-meetings").to_string();
                    load_scenario(&sim.repo, &name).map(|scenario| {
                        let fresh = World::new(&skin, &scenario, sim.version.clone(), sim.repo.join("fixtures"));
                        let old = std::mem::replace(&mut *sim.world.lock().unwrap(), fresh);
                        old.engine.shutdown();
                        *sim.scenario.lock().unwrap() = name.clone();
                        sim.version.fetch_add(1, Ordering::SeqCst);
                    })
                }
                Some("pause") => {
                    sim.running.store(!step.get("paused").and_then(Value::as_bool).unwrap_or(true), Ordering::SeqCst);
                    sim.version.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                _ => sim.world.lock().unwrap().act(&step),
            };
            match result {
                Ok(()) => json_reply(request, json!({ "ok": true })),
                Err(why) => request.respond(400, "text/plain", why.as_bytes()),
            }
        }
        _ => request.respond(404, "text/plain", b""),
    }
}
