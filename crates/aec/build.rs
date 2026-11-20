//! With the `localvqe` feature: builds LocalVQE's shared library (its C API with its own,
//! patched ggml inside) with CMake. Nothing links it: `LocalVqe::load` opens it at run time, so
//! its ggml stays apart from whisper.cpp's (docs/AUDIO-SPIKE.md, "M2 step 0").
//!
//! Tells dependent build scripts where the library is (`DEP_LOCALVQE_LIB_DIR`), and the crate
//! itself (`MUESLI_LOCALVQE_LIB_DIR`, for tests and development runs).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(feature = "localvqe")]
    build();
}

#[cfg(feature = "localvqe")]
fn build() {
    use std::path::{Path, PathBuf};

    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/LocalVQE/ggml");
    if !source.join("vendor/ggml/include/ggml.h").exists() {
        panic!("LocalVQE is missing: run `git submodule update --init --recursive`");
    }
    println!("cargo:rerun-if-changed={}", source.join("localvqe_api.cpp").display());
    let mut config = cmake::Config::new(&source);
    config
        .define("LOCALVQE_BUILD_SHARED", "ON")
        .define("GGML_METAL", "OFF")
        .define("GGML_BLAS", "OFF")
        .define("GGML_OPENMP", "OFF")
        .define("GGML_NATIVE", "OFF")
        .profile("Release")
        .build_target("localvqe_shared");
    let out = config.build();
    let build = out.join("build");
    // On x86 LocalVQE's CMake builds ggml as separate shared libraries next to its own (one per
    // CPU generation, chosen at run time); elsewhere ggml is inside it. Collect whatever there is.
    let dir = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("localvqe");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut found = false;
    for candidate in [build.clone(), build.join("bin"), build.join("Release"), build.join("bin/Release")] {
        let Ok(entries) = std::fs::read_dir(&candidate) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let shared = name.ends_with(".dylib") || name.ends_with(".dll") || name.ends_with(".so");
            // LocalVQE's library under its plain name only (it is opened by path; the versioned
            // names are the same file again), and every ggml library beside it (x86).
            let wanted = if name.contains("localvqe") {
                ["liblocalvqe.dylib", "localvqe.dll", "liblocalvqe.so"].contains(&name.as_str())
            } else {
                true
            };
            if shared && wanted && entry.path().is_file() {
                std::fs::copy(entry.path(), dir.join(&name)).unwrap();
                found |= name.contains("localvqe");
            }
        }
    }
    assert!(found, "LocalVQE built, but its shared library was not found under {}", build.display());
    println!("cargo:lib_dir={}", dir.display());
    println!("cargo:rustc-env=MUESLI_LOCALVQE_LIB_DIR={}", dir.display());
}
