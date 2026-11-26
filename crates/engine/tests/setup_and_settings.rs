//! First-launch setup, Settings and the config migration.

mod common;

use common::Harness;
use muesli_core::ports::{Access, Permission, Permissions, PlatformWords};
use muesli_core::Preferences;
use muesli_engine::fake::{FakeSpeech, HostCall};
use muesli_engine::{Intent, ModelStatus, SpeechModel, Window};
use muesli_platform_fake::{FakePermissions, FakePlatform};

fn first_launch(words: PlatformWords, permissions: FakePermissions) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let mut fakes = FakePlatform::with_words(dir.path(), words);
    fakes.permissions = std::sync::Arc::new(permissions);
    Harness::with(dir, fakes, FakeSpeech::missing(1000))
}

fn mac_permissions() -> FakePermissions {
    FakePermissions::new()
}

fn windows_permissions() -> FakePermissions {
    FakePermissions::with([
        (Permission::Microphone, Access::Allowed),
        (Permission::Notifications, Access::Allowed),
        (Permission::SystemAudio, Access::NotNeeded),
        (Permission::Calendar, Access::NotNeeded),
    ])
}

#[test]
fn setup_walks_through_four_screens_and_downloads_on_the_way() {
    let h = first_launch(PlatformWords::MAC, mac_permissions());
    assert!(h.host.is_open(Window::Setup));
    assert_eq!(h.speech.downloads(), 0, "nothing downloads before Get started");

    let welcome = h.engine.setup_view();
    assert_eq!((welcome.step, welcome.steps), (0, 4));
    assert_eq!(welcome.title, "Meet Muesli");
    assert_eq!(welcome.promises, ["Runs entirely on this Mac", "Keeps the text, never the audio", "No bot in your calls"]);
    assert_eq!(welcome.primary, "Get started");
    assert!(welcome.model.is_none());

    h.engine.intent(Intent::SetupNext);
    assert_eq!(h.speech.downloads(), 1, "Get started starts the download");
    h.speech.progress(250);
    let languages = h.engine.setup_view();
    assert_eq!(languages.step, 1);
    assert_eq!(languages.model.unwrap().text, "Downloading · 25%");
    let chosen: Vec<_> = languages.languages.chosen.iter().map(|c| c.code.as_str()).collect();
    assert_eq!(chosen, ["nl", "en"], "the system language and English");
    h.engine.intent(Intent::AddLanguage { code: "fr".into() });
    h.engine.intent(Intent::RemoveLanguage { code: "nl".into() });
    let chosen: Vec<_> = h.engine.setup_view().languages.chosen.iter().map(|c| c.code.clone()).collect();
    assert_eq!(chosen, ["en", "fr"]);

    h.engine.intent(Intent::SetupNext);
    let permissions = h.engine.setup_view();
    assert_eq!(permissions.step, 2);
    let rows: Vec<_> = permissions.permissions.iter().map(|p| (p.title.as_str(), p.required)).collect();
    assert_eq!(rows, [("Microphone", true), ("System audio", true), ("Calendar", false), ("Notifications", false)]);
    assert!(permissions.permissions.iter().all(|p| p.button.as_deref() == Some("Allow…")));
    assert!(!permissions.primary_enabled, "Continue waits for the required permissions");
    assert!(h.fakes.permissions.requests().is_empty(), "nothing asks by itself");

    h.fakes.permissions.request(Permission::Microphone).unwrap();
    assert!(!h.engine.setup_view().primary_enabled);
    h.fakes.permissions.request(Permission::SystemAudio).unwrap();
    assert!(h.engine.setup_view().primary_enabled, "optional ones are not needed");

    h.engine.intent(Intent::SetupNext);
    let done = h.engine.setup_view();
    assert_eq!(done.title, "Muesli is in your menu bar");
    assert_eq!(done.primary, "Open Muesli");
    assert!(done.notice.unwrap().starts_with("Please tell the people you meet with that you are transcribing"));
    assert!(!h.engine.preferences().onboarding_done);

    h.engine.intent(Intent::FinishSetup);
    assert!(h.engine.preferences().onboarding_done);
    assert!(Preferences::load(&h.engine.paths()).onboarding_done, "persisted");
    assert!(!h.host.is_open(Window::Setup));
    assert!(h.host.is_open(Window::Main));
}

#[test]
fn allow_asks_the_system_off_the_caller_thread() {
    let h = first_launch(PlatformWords::MAC, mac_permissions());
    h.engine.intent(Intent::RequestPermission { permission: Permission::Calendar });
    common::wait_for("asked", || h.fakes.permissions.requests() == [Permission::Calendar]);
}

#[test]
fn a_refused_allow_opens_the_system_settings() {
    let permissions = FakePermissions::with([(Permission::Calendar, Access::Denied)]);
    let h = first_launch(PlatformWords::MAC, permissions);
    h.engine.intent(Intent::RequestPermission { permission: Permission::Calendar });
    common::wait_for("settings opened", || h.fakes.permissions.opened_settings() == [Permission::Calendar]);
}

#[test]
fn a_granted_allow_opens_nothing() {
    let h = first_launch(PlatformWords::MAC, mac_permissions());
    h.engine.intent(Intent::RequestPermission { permission: Permission::Microphone });
    common::wait_for("allowed", || h.fakes.permissions.status(Permission::Microphone) == Access::Allowed);
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(h.fakes.permissions.opened_settings().is_empty());
}

#[test]
fn windows_setup_asks_for_the_microphone_only() {
    let h = first_launch(PlatformWords::WINDOWS, windows_permissions());
    h.engine.intent(Intent::SetupNext);
    h.engine.intent(Intent::SetupNext);
    let view = h.engine.setup_view();
    let titles: Vec<_> = view.permissions.iter().map(|p| p.title.as_str()).collect();
    assert_eq!(titles, ["Microphone", "Notifications"]);
    assert!(view.permissions_note.unwrap().contains("Call audio needs no permission"));
    assert!(view.primary_enabled);
    h.engine.intent(Intent::SetupNext);
    assert_eq!(h.engine.setup_view().title, "Muesli is in your system tray");
    assert_eq!(h.engine.setup_view().words.skin, "windows");
}

#[test]
fn a_denied_permission_points_to_the_system_settings() {
    let h = first_launch(PlatformWords::MAC, mac_permissions());
    h.fakes.permissions.set(Permission::Microphone, Access::Denied);
    let row = h.engine.settings_view().permissions.into_iter().find(|p| p.permission == Permission::Microphone).unwrap();
    assert_eq!(row.badge, "Not allowed");
    assert_eq!(row.button.as_deref(), Some("Open System Settings…"));
    h.engine.intent(Intent::OpenPermissionSettings { permission: Permission::Microphone });
    assert_eq!(h.fakes.permissions.opened_settings(), [Permission::Microphone]);
}

#[test]
fn settings_show_the_folder_login_languages_and_model() {
    let h = Harness::ready();
    let view = h.engine.settings_view();
    assert!(view.transcripts_folder.ends_with("MeetingTranscripts"));
    assert!(!view.transcripts_folder_locked);
    assert!(!view.start_at_login);
    assert_eq!(view.words.start_at_login, "Start at login");
    assert_eq!(view.model.text, "Ready");
    assert!(!view.languages.can_reset);

    h.engine.intent(Intent::SetStartAtLogin { on: true });
    assert!(h.engine.settings_view().start_at_login);
    h.engine.intent(Intent::AddLanguage { code: "de".into() });
    assert!(h.engine.settings_view().languages.can_reset);
    h.engine.intent(Intent::ResetLanguages);
    assert_eq!(h.engine.preferences().languages, None);

    h.engine.intent(Intent::ChooseTranscriptsFolder);
    assert_eq!(h.host.calls().last(), Some(&HostCall::PickTranscriptsFolder));
    let elsewhere = h.dir.path().join("Elsewhere");
    h.engine.intent(Intent::SetTranscriptsFolder { path: elsewhere.clone() });
    assert_eq!(h.engine.settings_view().transcripts_folder, elsewhere.to_string_lossy());
}

#[test]
fn a_failed_download_shows_why_and_where_and_can_be_retried() {
    let h = Harness::build(PlatformWords::WINDOWS, true, FakeSpeech::missing(10));
    h.speech.fail("Could not connect", "https://huggingface.co/model.bin");
    let model = h.engine.settings_view().model;
    assert_eq!(model.text, "Download failed");
    assert_eq!(model.error.as_deref(), Some("Could not connect"));
    assert_eq!(model.url.as_deref(), Some("https://huggingface.co/model.bin"));
    assert!(model.can_retry);
    h.engine.intent(Intent::DownloadModelAgain);
    assert!(matches!(h.speech.status(), ModelStatus::Downloading { .. }));
    assert_eq!(h.menu_labels()[0], "# Getting Ready");
}

#[test]
fn claude_desktop_through_its_settings_file_counts_as_connected() {
    use muesli_core::assistants::ClaudeDesktopConfig;
    let h = Harness::ready();
    *h.host.helper.lock().unwrap() = Some("/Applications/Muesli.app/Contents/MacOS/muesli-mcp".into());
    *h.host.claude.lock().unwrap() = true;
    *h.host.extension.lock().unwrap() = Some("/Applications/Muesli.app/Contents/Resources/Muesli.mcpb".into());
    std::fs::create_dir_all(ClaudeDesktopConfig::default_directory(&*h.fakes.paths)).unwrap();
    h.engine.intent(Intent::SetAssistantsEnabled { on: true });
    h.engine.intent(Intent::AddToClaudeConfig);

    let view = h.engine.settings_view().assistants;
    let row = view.claude_desktop.unwrap();
    assert_eq!(row.status, "Connected through Claude Desktop's settings file");
    assert_eq!(row.tone, "ok");
    assert_eq!(row.button, None, "nothing to connect");
    assert_eq!(row.detail.as_deref(), Some("Quit and reopen Claude Desktop to load it."));
    assert!(!view.can_add_to_claude_config);
}

#[test]
fn ai_assistants_are_off_until_switched_on() {
    let h = Harness::ready();
    *h.host.helper.lock().unwrap() = Some("/Applications/Muesli.app/Contents/MacOS/muesli-mcp".into());
    let view = h.engine.settings_view().assistants;
    assert!(!view.enabled);
    assert!(view.privacy.contains("Anthropic"));
    assert!(view.claude_desktop.is_none());

    *h.host.claude.lock().unwrap() = true;
    *h.host.extension.lock().unwrap() = Some("/Applications/Muesli.app/Contents/Resources/Muesli.mcpb".into());
    h.engine.intent(Intent::SetAssistantsEnabled { on: true });
    assert!(Preferences::load(&h.engine.paths()).mcp_enabled);
    let view = h.engine.settings_view().assistants;
    assert_eq!(view.claude_desktop.unwrap().button.as_deref(), Some("Connect Claude Desktop"));
    assert!(view.can_add_to_claude_config);
    let names: Vec<_> = view.assistants.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["Claude Code", "Codex CLI", "Gemini CLI", "VS Code (Copilot)", "Cursor"]);
    assert!(view.command.unwrap().contains("muesli-mcp"));

    h.engine.intent(Intent::PickAssistant { name: "Cursor".into() });
    assert!(h.engine.settings_view().assistants.assistants.iter().any(|a| a.name == "Cursor" && a.selected));
    h.engine.intent(Intent::ConnectClaudeDesktop);
    assert!(matches!(h.host.calls().last(), Some(HostCall::Open(p)) if p.ends_with("Muesli.mcpb")));
}

#[test]
fn old_settings_are_migrated_silently_on_start() {
    let dir = tempfile::tempdir().unwrap();
    let fakes = FakePlatform::new(dir.path());
    let paths = muesli_core::Paths::from_platform(&*fakes.paths);
    std::fs::create_dir_all(&paths.support).unwrap();
    std::fs::write(
        paths.config(),
        r#"{"autoStop":true,"endDelaySeconds":300,"recentLimit":10,"customModelPath":"/m.bin","savedLanguages":["fr"],"dropFolder":"/d","dropFolderCleanup":"delete","languages":["de"],"onboardingDone":true}"#,
    )
    .unwrap();
    let h = Harness::with(dir, fakes, FakeSpeech::ready());
    let text = std::fs::read_to_string(paths.config()).unwrap();
    assert!(!text.contains("autoStop") && !text.contains("dropFolder") && !text.contains("customModelPath"), "{text}");
    assert_eq!(h.engine.preferences().languages, Some(vec!["de".to_string()]));
    assert!(!h.host.is_open(Window::Setup));
}

#[test]
fn intents_arrive_from_the_ui_as_json() {
    let parse = |s: &str| serde_json::from_str::<Intent>(s).unwrap();
    assert_eq!(parse(r#"{"type":"rename","id":"A","title":"B"}"#), Intent::Rename { id: "A".into(), title: "B".into() });
    assert_eq!(parse(r#"{"type":"answerPrompt","accept":true}"#), Intent::AnswerPrompt { prompt: None, accept: true });
    assert_eq!(
        parse(r#"{"type":"requestPermission","permission":"systemAudio"}"#),
        Intent::RequestPermission { permission: Permission::SystemAudio }
    );
    assert_eq!(parse(r#"{"type":"setupNext"}"#), Intent::SetupNext);
}

#[test]
fn assistant_setup_speaks_the_platform_s_words() {
    let h = Harness::build(PlatformWords::WINDOWS, true, FakeSpeech::ready());
    *h.host.helper.lock().unwrap() = Some(r"C:\Program Files\Muesli\muesli-mcp.exe".into());
    h.engine.intent(Intent::SetAssistantsEnabled { on: true });
    let view = h.engine.settings_view().assistants;
    assert!(view.instructions.contains("PowerShell"), "{}", view.instructions);
    assert!(view.command.unwrap().contains(r"'C:\Program Files\Muesli\muesli-mcp.exe'"));
    let mac = Harness::ready();
    mac.engine.intent(Intent::SetAssistantsEnabled { on: true });
    assert!(mac.engine.settings_view().assistants.instructions.contains("Terminal"));
}

/// An older Muesli extension in Claude Desktop: a system notification with Update (once per
/// version) and a banner in Muesli's window until the update is done or put away. Update opens the
/// bundled extension, so Claude shows its own update dialog; nothing opens by itself.
#[test]
fn an_outdated_claude_extension_is_announced() {
    use common::languages;
    use muesli_core::assistants::{ClaudeDesktopConfig, ClaudeDesktopExtension};
    use muesli_engine::fake::RecordingHost;
    use muesli_engine::{Engine, Options};
    use std::sync::Arc;

    let dir = tempfile::tempdir().unwrap();
    let fakes = FakePlatform::with_words(dir.path(), PlatformWords::MAC);
    let paths = muesli_core::Paths::from_platform(&*fakes.paths);
    std::fs::create_dir_all(&paths.support).unwrap();
    Preferences { onboarding_done: true, mcp_enabled: true, ..Default::default() }.save(&paths).unwrap();
    let claude = ClaudeDesktopConfig::default_directory(&*fakes.paths);
    let installed = ClaudeDesktopExtension::extensions_directory(&claude).join("local.mcpb.muesli.muesli");
    std::fs::create_dir_all(&installed).unwrap();
    let manifest = |version: &str| {
        std::fs::write(installed.join("manifest.json"), format!(r#"{{"name": "muesli", "version": "{version}"}}"#)).unwrap()
    };
    manifest("0.0.0");
    let bundle = std::path::PathBuf::from("/Applications/Muesli.app/Contents/Resources/Muesli.mcpb");
    let start = || {
        let host = Arc::new(RecordingHost::default());
        *host.claude.lock().unwrap() = true;
        *host.extension.lock().unwrap() = Some(bundle.clone());
        let engine = Engine::new(fakes.platform(), Arc::new(FakeSpeech::ready()), host.clone(), Options::new(languages()));
        engine.start();
        (engine, host)
    };
    let opened = |host: &RecordingHost| host.calls().into_iter().filter(|c| *c == HostCall::Open(bundle.clone())).count();

    let (engine, host) = start();
    assert_eq!(opened(&host), 0, "nothing opens by itself");
    let notice = fakes.notifier.pending_notice().expect("a system notification");
    assert_eq!(notice.title, "Claude Desktop extension update");
    assert_eq!(notice.action.as_deref(), Some("Update"));
    assert_eq!(engine.main_view().extension_update.as_deref(), Some("A newer Muesli extension for Claude Desktop is ready."));
    assert!(fakes.notifier.click_action());
    assert_eq!(opened(&host), 1, "Update opens the extension in Claude");

    // Next start: no second notification for this version, the banner stays until put away.
    let notices = fakes.notifier.notices().len();
    let (engine, _host) = start();
    assert_eq!(fakes.notifier.notices().len(), notices);
    assert!(engine.main_view().extension_update.is_some());
    engine.intent(Intent::DismissExtensionUpdate);
    assert!(engine.main_view().extension_update.is_none());

    // Once Claude has the new version, nothing is shown.
    manifest(env!("CARGO_PKG_VERSION"));
    Preferences { onboarding_done: true, mcp_enabled: true, ..Default::default() }.save(&paths).unwrap();
    let (engine, _host) = start();
    assert!(engine.main_view().extension_update.is_none());
    assert_eq!(fakes.notifier.notices().len(), notices);
}

#[test]
fn the_vocabulary_is_edited_in_settings_and_shared_through_its_file() {
    let h = Harness::ready();
    assert!(h.engine.settings_view().vocabulary.terms.is_empty());
    h.engine.intent(Intent::AddTerm { term: "  LocalVQE ".into() });
    h.engine.intent(Intent::AddTerm { term: "localvqe".into() });
    assert_eq!(h.engine.settings_view().vocabulary.terms, ["LocalVQE"]);
    // An AI assistant (muesli-mcp, another process) adds one: Settings shows it.
    let paths = h.engine.paths();
    let mut v = muesli_core::Vocabulary::load(&paths);
    v.add(&["Anna Peeters".to_string()]).unwrap();
    v.save(&paths).unwrap();
    assert_eq!(h.engine.settings_view().vocabulary.terms, ["LocalVQE", "Anna Peeters"]);
    h.engine.intent(Intent::AddTerm { term: "x".repeat(61) });
    assert!(h.engine.settings_view().vocabulary.error.unwrap().contains("longer than 60"));
    h.engine.intent(Intent::RemoveTerm { term: "LocalVQE".into() });
    let view = h.engine.settings_view().vocabulary;
    assert_eq!(view.terms, ["Anna Peeters"]);
    assert_eq!(view.error, None);
}

#[test]
fn the_model_is_prepared_at_start_and_when_a_download_moves_on() {
    use std::sync::atomic::Ordering;
    let h = Harness::ready();
    assert!(h.speech.prepared.load(Ordering::SeqCst) >= 1, "at start, so a first transcription does not wait");
    let h = Harness::build(PlatformWords::MAC, true, FakeSpeech::missing(1000));
    let before = h.speech.prepared.load(Ordering::SeqCst);
    h.speech.finish();
    assert!(h.speech.prepared.load(Ordering::SeqCst) > before, "again once the files are in");
}
