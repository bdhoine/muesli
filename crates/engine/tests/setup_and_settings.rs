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
    assert_eq!(welcome.promises, ["Runs entirely on this Mac.", "Keeps the text, never the audio.", "No bot in your calls."]);
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
    assert!(done.notice.unwrap().starts_with("Tell people you are transcribing"));
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
