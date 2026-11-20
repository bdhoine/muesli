use std::path::Path;

fn main() {
    bundle_echo_canceller();
    tauri_build::build()
}

/// Puts LocalVQE (built by `muesli-aec` on macOS and Windows) with its model and licence in
/// `resources/aec/`, which `tauri.release.conf.json` bundles as `aec/`. A release build without
/// the model fails: it would ship without echo cancellation.
fn bundle_echo_canceller() {
    let Ok(lib_dir) = std::env::var("DEP_LOCALVQE_LIB_DIR") else { return };
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../..");
    let out = manifest.join("resources/aec");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    for entry in std::fs::read_dir(&lib_dir).unwrap().flatten() {
        std::fs::copy(entry.path(), out.join(entry.file_name())).unwrap();
    }
    std::fs::copy(root.join("crates/aec/vendor/LocalVQE/LICENSE"), out.join("LICENSE-LocalVQE.txt")).unwrap();
    let model = root.join("crates/aec/model/localvqe-v1.4-aec-200K-f32.gguf");
    println!("cargo:rerun-if-changed={}", model.display());
    if model.exists() {
        std::fs::copy(&model, out.join(model.file_name().unwrap())).unwrap();
    } else if std::env::var("PROFILE").as_deref() == Ok("release") {
        panic!("The echo cancellation model is missing: run `python3 scripts/fetch-aec-model.py`");
    } else {
        println!(
            "cargo:warning=No echo cancellation model: run `python3 scripts/fetch-aec-model.py` (the microphone is transcribed as it is)"
        );
    }
}
