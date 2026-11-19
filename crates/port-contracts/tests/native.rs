//! Every contract against the adapters of the OS the tests run on.
//!
//! Contracts that change the machine (start at login) or need hardware and a person (audio
//! devices, permission dialogs, notifications) are `#[ignore]`d: run them by hand on a real
//! machine with `cargo test -p muesli-port-contracts --test native -- --ignored`.

use muesli_platform_common::{SystemClock, SystemPaths};
use muesli_port_contracts as contract;

#[test]
fn clock() {
    contract::clock(&SystemClock);
}

#[test]
fn paths() {
    contract::platform_paths(&SystemPaths);
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::Arc;

    use muesli_core::ports::Platform;
    use muesli_platform_fake::FakeTray;
    use muesli_port_contracts as contract;

    fn platform() -> Platform {
        muesli_platform_macos::platform(Arc::new(FakeTray::default()))
    }

    #[test]
    fn trash() {
        let dir = tempfile::tempdir().unwrap();
        contract::trash(&*platform().trash, dir.path());
    }

    #[test]
    fn permissions() {
        contract::permissions(&*platform().permissions);
    }

    #[test]
    fn meeting_detector() {
        contract::meeting_detector(&*platform().detector);
    }

    #[test]
    #[ignore = "changes the login items of this Mac"]
    fn login_item() {
        contract::login_item(&*platform().login_item);
    }

    #[test]
    #[ignore = "shows a notification"]
    fn notifier() {
        contract::notifier(&*platform().notifier);
    }

    #[test]
    #[ignore = "needs a microphone and the permission"]
    fn microphone() {
        let p = platform();
        contract::audio_source(&|s, e| p.mic.start(s, e), &|| {}, true);
    }

    #[test]
    #[ignore = "needs something playing and the System Audio Recording permission"]
    fn system_audio() {
        let p = platform();
        contract::audio_source(&|s, e| p.system_audio.start(s, e), &|| {}, true);
    }
}

#[cfg(windows)]
mod windows {
    use std::sync::Arc;

    use muesli_core::ports::Platform;
    use muesli_platform_fake::FakeTray;
    use muesli_port_contracts as contract;

    fn platform() -> Platform {
        muesli_platform_windows::platform(Arc::new(FakeTray::default()))
    }

    #[test]
    fn trash() {
        let dir = tempfile::tempdir().unwrap();
        contract::trash(&*platform().trash, dir.path());
    }

    #[test]
    fn permissions() {
        contract::permissions(&*platform().permissions);
    }

    #[test]
    fn meeting_detector() {
        contract::meeting_detector(&*platform().detector);
    }

    #[test]
    #[ignore = "changes the Run key of this PC"]
    fn login_item() {
        contract::login_item(&*platform().login_item);
    }

    #[test]
    #[ignore = "shows a toast"]
    fn notifier() {
        contract::notifier(&*platform().notifier);
    }

    #[test]
    #[ignore = "needs a microphone"]
    fn microphone() {
        let p = platform();
        contract::audio_source(&|s, e| p.mic.start(s, e), &|| {}, true);
    }

    #[test]
    #[ignore = "needs something playing"]
    fn system_audio() {
        let p = platform();
        contract::audio_source(&|s, e| p.system_audio.start(s, e), &|| {}, true);
    }
}
