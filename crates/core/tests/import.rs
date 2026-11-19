use std::path::Path;

use muesli_core::import::is_audio;

#[test]
fn only_audio_files_count() {
    for yes in ["a.mp3", "a.M4A", "a.wav", "a.flac", "a.ogg"] {
        assert!(is_audio(Path::new(yes)), "{yes}");
    }
    for no in ["a.txt", "a.pdf", "mp3", "a.mp3.part"] {
        assert!(!is_audio(Path::new(no)), "{no}");
    }
}
