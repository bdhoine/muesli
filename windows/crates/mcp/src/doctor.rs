use chrono::{SecondsFormat, Utc};
use muesli_core::store::MeetingStore;
use muesli_core::{AppStatus, Paths, Preferences};

/// `muesli-mcp doctor`: checks each link of the chain and says which one is broken.
pub fn run(paths: &Paths) -> bool {
    let mut ok = true;
    let mut check = |pass: bool, label: &str, detail: &str| {
        let suffix = if detail.is_empty() { String::new() } else { format!(": {detail}") };
        println!("{} {label}{suffix}", if pass { "ok  " } else { "FAIL" });
        if !pass {
            ok = false;
        }
    };

    let prefs = Preferences::load(paths);
    check(paths.config().exists(), "Preferences file", &paths.config().to_string_lossy());
    check(
        prefs.mcp_enabled,
        "AI assistant access enabled",
        if prefs.mcp_enabled { "" } else { "off in Muesli > Settings > AI Assistants" },
    );

    match AppStatus::read(paths) {
        Some(status) => {
            let fresh = status.is_fresh(Utc::now());
            let detail = if fresh {
                format!("state {}", status.app_state)
            } else {
                format!("last heartbeat {}", status.updated_at.to_rfc3339_opts(SecondsFormat::Secs, true))
            };
            check(fresh, "Muesli app running", &detail);
        }
        None => check(false, "Muesli app running", "no state.json; start Muesli"),
    }
    println!("      {} meeting(s) in the store", MeetingStore::new(paths).all().len());
    println!("      The assistant (or the Claude Desktop extension) starts muesli-mcp itself over stdio; nothing else to check.");
    ok
}
