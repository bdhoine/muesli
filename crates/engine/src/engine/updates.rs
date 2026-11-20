//! Muesli's own updates (UP1 to UP8 in docs/REQUIREMENTS.md): when to check, downloading in the
//! background, when to show that an update is ready, Restart and Quit, and what happens on the
//! first start of a new version. The `Updater` port does the network and the installing.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use muesli_core::ports::{Access, Notice, Permission, Release, TrayItem, UpdateError, Updater};
use muesli_core::update::{is_newer, release_notes_url};

use super::{Activity, Engine, ImportState};
use crate::host::Window;
use crate::intent::Intent;
use crate::speech::ModelStatus;
use crate::views::{UpdateBanner, UpdatesView};

/// The first check waits this long after Muesli starts, so starting stays quick.
pub const FIRST_CHECK_AFTER: i64 = 30;
/// Later checks come this long after the last one.
pub const CHECK_EVERY: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Phase {
    Idle,
    Checking,
    Downloading {
        version: String,
        fraction: Option<f64>,
    },
    /// Downloaded and verified: Restart installs it.
    Ready {
        version: String,
    },
    /// The answer to a check from Settings: shown there until the next check.
    UpToDate,
    Failed(String),
}

#[derive(Debug, Clone)]
pub(super) struct UpdateState {
    pub phase: Phase,
    /// When Muesli started; the first check waits `FIRST_CHECK_AFTER` from here.
    pub started_at: Option<DateTime<Utc>>,
    /// When the last check began (from Settings or by itself), for the next one.
    pub last_attempt: Option<DateTime<Utc>>,
    /// The check running now came from Settings, so its answer is shown there.
    pub manual: bool,
    /// "Not now" on the banner: hidden until the next start.
    pub banner_hidden: bool,
    /// After an update macOS asked for permissions again: setup shows only that screen.
    pub permissions_after_update: bool,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            started_at: None,
            last_attempt: None,
            manual: false,
            banner_hidden: false,
            permissions_after_update: false,
        }
    }
}

fn reason(error: &UpdateError) -> String {
    let text = error.to_string();
    let text = text.trim().trim_end_matches('.');
    if text.is_empty() {
        "no answer from GitHub".into()
    } else {
        text.into()
    }
}

impl Engine {
    fn updater(&self) -> Option<Arc<dyn Updater>> {
        self.options.updater.clone()
    }

    /// The version that runs: the updater's, or the workspace version without one.
    pub(super) fn running_version(&self) -> String {
        self.updater().map(|u| u.current_version()).unwrap_or_else(|| muesli_core::VERSION.to_string())
    }

    /// Transcribing a meeting, importing an audio file or downloading the speech model: update
    /// prompts wait and Restart is not offered (UP5).
    pub(super) fn busy_for_update(&self) -> bool {
        let downloading = matches!(self.speech.status(), ModelStatus::Downloading { .. });
        let s = self.lock();
        downloading
            || matches!(s.activity, Activity::Starting | Activity::Transcribing | Activity::Saving)
            || s.imports.iter().any(|i| matches!(i.state, ImportState::Waiting | ImportState::Running { .. }))
    }

    /// The version that is downloaded and ready to install.
    pub(super) fn update_ready(&self) -> Option<String> {
        match &self.lock().update.phase {
            Phase::Ready { version } => Some(version.clone()),
            _ => None,
        }
    }

    /// Ready, and Muesli is idle: the menu item, the icon's dot, the banner and the notification.
    fn update_to_offer(&self) -> Option<String> {
        self.update_ready().filter(|_| !self.busy_for_update())
    }

    // -----------------------------------------------------------------------------------------
    // At start

    /// At start: remembers when Muesli started (the first check waits 30 s), and on the first start
    /// of a new version says so and, on macOS, checks the permissions again (UP7).
    pub(super) fn start_updates(&self) {
        let now = self.now();
        let version = self.running_version();
        let (setup_done, last_run) = {
            let mut s = self.lock();
            s.update.started_at = Some(now);
            (s.prefs.onboarding_done, s.prefs.last_run_version.clone())
        };
        if !setup_done {
            return; // recorded when setup finishes
        }
        if last_run.as_deref() != Some(version.as_str()) {
            self.save_prefs(|p| p.last_run_version = Some(version.clone()));
        }
        // No version recorded: a first start, or a version from before updates. Nothing to say.
        let Some(previous) = last_run else { return };
        if !is_newer(&version, &previous) {
            return;
        }
        let opener = self.me.clone();
        let notes = version.clone();
        let _ = self.platform.notifier.notify(
            &Notice {
                title: format!("Muesli updated to {version}"),
                body: "See what changed in this version.".into(),
                action: Some("What's new".into()),
            },
            Box::new(move || {
                if let Some(engine) = opener.upgrade() {
                    engine.intent(Intent::OpenReleaseNotes { version: notes.clone() });
                }
            }),
        );
        // macOS ties permissions to the app's signature; an ad-hoc signed update may lose them.
        if self.platform.words.skin == "mac" {
            let lost = [Permission::Microphone, Permission::SystemAudio]
                .into_iter()
                .any(|p| matches!(self.platform.permissions.status(p), Access::Denied | Access::NotAsked));
            if lost {
                {
                    let mut s = self.lock();
                    s.update.permissions_after_update = true;
                    s.setup_step = 2;
                }
                self.host.show(Window::Setup);
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Time

    /// Every tick: a check when one is due (UP2), and the notification for a ready update once
    /// Muesli is idle (UP4).
    pub(super) fn tick_updates(&self, now: DateTime<Utc>) {
        if self.updater().is_none() {
            return;
        }
        let due = {
            let s = self.lock();
            let waiting = matches!(s.update.phase, Phase::Idle | Phase::UpToDate | Phase::Failed(_));
            let after_start = s.update.started_at.is_some_and(|t| now - t >= Duration::seconds(FIRST_CHECK_AFTER));
            let since_last = s.update.last_attempt.is_none_or(|t| now - t >= Duration::seconds(CHECK_EVERY));
            s.prefs.onboarding_done && s.prefs.auto_update_check && waiting && after_start && since_last
        };
        if due {
            self.check_for_updates(false);
        }
        self.announce_update();
    }

    /// The system notification for a ready update: once per version, only while idle.
    fn announce_update(&self) {
        let Some(version) = self.update_to_offer() else { return };
        if self.preferences().update_notified.as_deref() == Some(version.as_str()) {
            return;
        }
        self.save_prefs(|p| p.update_notified = Some(version.clone()));
        let opener = self.me.clone();
        let _ = self.platform.notifier.notify(
            &Notice {
                title: format!("Muesli {version} is ready to install"),
                body: "Restart Muesli to update. Your transcripts and settings stay as they are.".into(),
                action: Some("Restart".into()),
            },
            Box::new(move || {
                if let Some(engine) = opener.upgrade() {
                    engine.intent(Intent::RestartToUpdate);
                }
            }),
        );
    }

    // -----------------------------------------------------------------------------------------
    // Checking and downloading

    /// Asks GitHub for a newer version and downloads it, in the background. `manual`: from
    /// Settings, whose answer is shown there; a check by itself fails silently.
    pub(super) fn check_for_updates(&self, manual: bool) {
        let Some(updater) = self.updater() else { return };
        {
            let mut s = self.lock();
            if matches!(s.update.phase, Phase::Checking | Phase::Downloading { .. } | Phase::Ready { .. }) {
                return;
            }
            s.update.phase = Phase::Checking;
            s.update.manual = manual;
            s.update.last_attempt = Some(self.now());
        }
        self.host.changed();
        let me = self.me();
        std::thread::spawn(move || me.run_update(updater));
    }

    fn run_update(&self, updater: Arc<dyn Updater>) {
        let current = self.running_version();
        let found = updater.check();
        if found.is_ok() {
            let now = self.now();
            self.save_prefs(|p| p.update_last_checked = Some(now));
        }
        let release: Release = match found {
            Ok(Some(release)) if is_newer(&release.version, &current) => release,
            Ok(_) => return self.update_done(Phase::UpToDate),
            Err(e) => return self.update_done(Phase::Failed(format!("Couldn't check for updates: {}.", reason(&e)))),
        };
        let version = release.version.clone();
        self.lock().update.phase = Phase::Downloading { version: version.clone(), fraction: None };
        self.host.changed();
        let me = self.me.clone();
        let shown = std::sync::Mutex::new(None::<u32>);
        let on_progress = Box::new(move |received: u64, total: Option<u64>| {
            let fraction = total.filter(|t| *t > 0).map(|t| (received as f64 / t as f64).min(1.0));
            let percent = fraction.map(|f| (f * 100.0).floor() as u32);
            let Some(engine) = me.upgrade() else { return };
            let mut last = shown.lock().unwrap();
            if *last == percent && percent.is_some() {
                return; // redraw once per percent
            }
            *last = percent;
            if let Phase::Downloading { fraction: f, .. } = &mut engine.lock().update.phase {
                *f = fraction;
            }
            engine.host.changed();
        });
        match updater.download(&release, on_progress) {
            Ok(()) => {
                self.update_done(Phase::Ready { version });
                self.announce_update();
                self.changed(); // the menu and its icon
            }
            Err(e) => self.update_done(Phase::Failed(format!("Couldn't check for updates: {}.", reason(&e)))),
        }
    }

    /// The end of a check. A check by itself shows nothing unless an update is ready; it is tried
    /// again at the next one.
    fn update_done(&self, phase: Phase) {
        {
            let mut s = self.lock();
            let manual = s.update.manual;
            s.update.phase = match phase {
                Phase::UpToDate | Phase::Failed(_) if !manual => Phase::Idle,
                other => other,
            };
        }
        self.host.changed();
    }

    // -----------------------------------------------------------------------------------------
    // Installing

    /// Restart: installs the update and starts the new version (UP6). Not while busy.
    pub(super) fn restart_to_update(&self) {
        let Some(updater) = self.updater() else { return };
        if self.update_to_offer().is_none() {
            return;
        }
        self.shutdown();
        if let Err(e) = updater.install_and_restart() {
            {
                let mut s = self.lock();
                s.quitting = false;
                s.update.manual = true;
                s.update.phase = Phase::Failed(format!("Couldn't install the update: {}.", reason(&e)));
            }
            self.changed();
        }
    }

    /// Quit with an update ready: it is installed, and Muesli does not start again (UP6).
    pub(super) fn install_on_quit(&self) {
        let Some(updater) = self.updater() else { return };
        if self.update_ready().is_some() {
            let _ = updater.install_on_quit();
        }
    }

    pub(super) fn open_release_notes(&self, version: &str) {
        self.host.open_url(&release_notes_url(version));
    }

    // -----------------------------------------------------------------------------------------
    // Views

    /// The menu's first item while an update is ready and Muesli is idle.
    pub(super) fn update_menu_items(&self) -> Vec<TrayItem> {
        match self.update_to_offer() {
            Some(version) => vec![
                TrayItem::Action { id: "update".into(), label: format!("Restart to Update to {version}"), enabled: true, detail: None },
                TrayItem::Separator,
            ],
            None => Vec::new(),
        }
    }

    pub(super) fn update_dot(&self) -> bool {
        self.update_to_offer().is_some()
    }

    pub(super) fn update_banner(&self) -> Option<UpdateBanner> {
        let version = self.update_to_offer()?;
        if self.lock().update.banner_hidden {
            return None;
        }
        Some(UpdateBanner { text: format!("Muesli {version} is ready to install"), version })
    }

    pub(super) fn updates_view(&self) -> UpdatesView {
        let version = self.running_version();
        let available = self.updater().is_some();
        let busy = self.busy_for_update();
        let (phase, prefs) = {
            let s = self.lock();
            (s.update.phase.clone(), s.prefs.clone())
        };
        let mut view = UpdatesView {
            title: format!("Muesli {version}"),
            version: version.clone(),
            available,
            status: None,
            tone: String::new(),
            progress: None,
            last_checked: prefs.update_last_checked.map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
            check_label: "Check for Updates".into(),
            can_check: available,
            restart: false,
            can_restart: false,
            restart_hint: None,
            auto_check: prefs.auto_update_check,
        };
        match phase {
            Phase::Idle => {}
            Phase::Checking => {
                view.check_label = "Checking…".into();
                view.can_check = false;
            }
            Phase::Downloading { version, fraction } => {
                view.status = Some(match fraction {
                    Some(f) => format!("Downloading {version}… {}%", (f * 100.0).floor() as u32),
                    None => format!("Downloading {version}…"),
                });
                view.progress = fraction;
                view.can_check = false;
            }
            Phase::Ready { version } => {
                view.status = Some(format!("{version} is ready"));
                view.tone = "ok".into();
                view.restart = true;
                view.can_restart = !busy;
                view.restart_hint = busy.then(|| "Muesli restarts to update once it has finished transcribing.".to_string());
                view.can_check = false;
            }
            Phase::UpToDate => {
                view.status = Some("Muesli is up to date".into());
                view.tone = "ok".into();
            }
            Phase::Failed(why) => {
                view.status = Some(why);
                view.tone = "warn".into();
                view.check_label = "Try again".into();
            }
        }
        view
    }
}
