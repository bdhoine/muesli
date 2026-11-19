use std::path::Path;

use muesli_core::import::{clean_up, is_audio, AudioCleanup};
use muesli_core::{DropFolder, Paths};

fn touch(path: &Path, bytes: usize) {
    std::fs::write(path, vec![1u8; bytes]).unwrap();
}

fn setup() -> (tempfile::TempDir, Paths, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("support"), dir.path().join("transcripts"));
    std::fs::create_dir_all(&paths.support).unwrap();
    let drop = dir.path().join("drop");
    std::fs::create_dir_all(&drop).unwrap();
    (dir, paths, drop)
}

#[test]
fn only_audio_files_count() {
    for yes in ["a.mp3", "a.M4A", "a.wav", "a.flac", "a.ogg"] {
        assert!(is_audio(Path::new(yes)), "{yes}");
    }
    for no in ["a.txt", "a.pdf", "mp3", "a.mp3.part"] {
        assert!(!is_audio(Path::new(no)), "{no}");
    }
}

#[test]
fn a_file_is_offered_once_it_stops_changing() {
    let (_dir, paths, drop) = setup();
    let mut watch = DropFolder::new(&paths);
    touch(&drop.join("call.mp3"), 100);
    assert!(watch.poll(&drop).is_empty(), "first sighting: it may still be copying");
    assert_eq!(watch.poll(&drop), vec![drop.join("call.mp3")]);
    assert!(watch.poll(&drop).is_empty(), "offered once");
}

#[test]
fn a_file_that_is_still_growing_waits() {
    let (_dir, paths, drop) = setup();
    let mut watch = DropFolder::new(&paths);
    touch(&drop.join("call.wav"), 100);
    watch.poll(&drop);
    touch(&drop.join("call.wav"), 5000); // grew
    assert!(watch.poll(&drop).is_empty());
    assert_eq!(watch.poll(&drop).len(), 1);
}

#[test]
fn temporary_hidden_empty_and_other_files_are_ignored() {
    let (_dir, paths, drop) = setup();
    let mut watch = DropFolder::new(&paths);
    for name in [".hidden.mp3", "~lock.mp3", "x.mp3.part", "movie.crdownload", "notes.txt"] {
        touch(&drop.join(name), 100);
    }
    touch(&drop.join("empty.mp3"), 0);
    std::fs::create_dir(drop.join("folder.mp3")).unwrap();
    watch.poll(&drop);
    assert!(watch.poll(&drop).is_empty());
}

#[test]
fn a_kept_file_is_not_transcribed_again_even_after_a_restart() {
    let (_dir, paths, drop) = setup();
    let file = drop.join("call.mp3");
    touch(&file, 100);
    let mut watch = DropFolder::new(&paths);
    watch.poll(&drop);
    assert_eq!(watch.poll(&drop).len(), 1);
    watch.mark_done(&file);

    let mut again = DropFolder::new(&paths); // the app was restarted
    again.poll(&drop);
    assert!(again.poll(&drop).is_empty());

    touch(&file, 200); // replaced by a different file with the same name
    again.poll(&drop);
    assert_eq!(again.poll(&drop).len(), 1);
}

#[test]
fn files_come_oldest_first() {
    let (_dir, paths, drop) = setup();
    let mut watch = DropFolder::new(&paths);
    touch(&drop.join("b.mp3"), 10);
    std::thread::sleep(std::time::Duration::from_millis(20));
    touch(&drop.join("a.mp3"), 10);
    watch.poll(&drop);
    let names: Vec<_> = watch.poll(&drop).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["b.mp3", "a.mp3"]);
}

#[test]
fn cleanup_modes() {
    let (dir, _paths, drop) = setup();
    let trash = muesli_platform_fake::FolderTrash::new(dir.path().join("Trash"), "Trash");
    let keep = drop.join("keep.mp3");
    touch(&keep, 10);
    clean_up(&keep, AudioCleanup::Keep, &trash).unwrap();
    assert!(keep.exists());
    let gone = drop.join("gone.mp3");
    touch(&gone, 10);
    clean_up(&gone, AudioCleanup::Delete, &trash).unwrap();
    assert!(!gone.exists());
    assert!(clean_up(&gone, AudioCleanup::Delete, &trash).is_err(), "already gone");
}

#[test]
fn the_cleanup_setting_is_written_in_lower_case() {
    assert_eq!(serde_json::to_string(&AudioCleanup::Trash).unwrap(), "\"trash\"");
    assert_eq!(serde_json::from_str::<AudioCleanup>("\"delete\"").unwrap(), AudioCleanup::Delete);
    assert_eq!(muesli_core::Preferences::default().drop_folder_cleanup, AudioCleanup::Keep);
    assert_eq!(muesli_core::Preferences::default().drop_folder, None);
}
