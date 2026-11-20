//! Unpacks a downloaded zip of folders (the Core ML encoder) next to it.

use std::fs::File;
use std::path::Path;

/// Unpacks `zip` into `dir`: into a temporary folder first, then each top-level folder is moved
/// into place in one step (an older copy is replaced), and the zip is deleted. Entries under
/// `__MACOSX/` (Finder's leftovers) are skipped; every entry's checksum is verified while reading.
pub fn unpack(zip: &Path, dir: &Path) -> Result<(), String> {
    let staging = dir.join(format!(".unpack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    let result = extract(zip, &staging).and_then(|_| move_into_place(&staging, dir));
    let _ = std::fs::remove_dir_all(&staging);
    result?;
    let _ = std::fs::remove_file(zip);
    Ok(())
}

fn extract(zip: &Path, staging: &Path) -> Result<(), String> {
    let file = File::open(zip).map_err(|e| format!("Could not open {}: {e}", zip.display()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Not a valid archive: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        // `enclosed_name` refuses absolute paths and `..`.
        let Some(name) = entry.enclosed_name() else { continue };
        if name.starts_with("__MACOSX") {
            continue;
        }
        let target = staging.join(&name);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = File::create(&target).map_err(|e| format!("Could not unpack {}: {e}", name.display()))?;
        // Reading to the end checks the entry's CRC.
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("Could not unpack {}: {e}", name.display()))?;
    }
    Ok(())
}

fn move_into_place(staging: &Path, dir: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(staging).map_err(|e| e.to_string())?.flatten() {
        let target = dir.join(entry.file_name());
        if target.is_dir() {
            let _ = std::fs::remove_dir_all(&target);
        }
        std::fs::rename(entry.path(), &target).map_err(|e| format!("Could not move {} into place: {e}", target.display()))?;
    }
    Ok(())
}

/// Folders a cut-off unpack left behind.
pub fn remove_leftovers(dir: &Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().starts_with(".unpack-") {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}
