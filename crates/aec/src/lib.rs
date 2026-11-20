//! LocalVQE (<https://github.com/localai-org/LocalVQE>, Apache-2.0) as Muesli's
//! [`EchoCanceller`]: a neural echo canceller that removes the call from the microphone and
//! keeps the near speaker, also while both talk.
//!
//! It is a shared library with its own ggml, opened at run time ([`LocalVqe::load`]) rather
//! than linked, because whisper.cpp brings another ggml with the same symbol names. A missing
//! library or model is an error the caller turns into "no canceller": the microphone is then
//! transcribed as it is.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};

use libloading::Library;
use muesli_core::aec::HOP;
use muesli_core::EchoCanceller;

/// The library's file name on this platform.
#[cfg(target_os = "macos")]
pub const LIBRARY: &str = "liblocalvqe.dylib";
#[cfg(target_os = "windows")]
pub const LIBRARY: &str = "localvqe.dll";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const LIBRARY: &str = "liblocalvqe.so";

/// The model: echo cancellation only (voice, noise and room left as they are, which suits
/// Whisper), 2.9 MB. Bundled with the app; LocalVQE refuses files not on its hash allowlist.
pub const MODEL: &str = "localvqe-v1.4-aec-200K-f32.gguf";
pub const MODEL_SHA256: &str = "b6e43138588a83bfe903ab5e143b4020b91c1e1629f5a575ac5855ff0003c731";

/// Where this build put the library, with the `localvqe` feature (tests, development runs).
pub fn built_library_dir() -> Option<PathBuf> {
    option_env!("MUESLI_LOCALVQE_LIB_DIR").map(PathBuf::from)
}

type Ctx = *mut c_void;
type Opts = *mut c_void;

/// The C API (`ggml/localvqe_api.h`), resolved once.
struct Api {
    options_new: unsafe extern "C" fn() -> Opts,
    options_free: unsafe extern "C" fn(Opts),
    options_set_model_path: unsafe extern "C" fn(Opts, *const c_char) -> c_int,
    options_set_threads: unsafe extern "C" fn(Opts, c_int) -> c_int,
    new_with_options: unsafe extern "C" fn(Opts) -> Ctx,
    free: unsafe extern "C" fn(Ctx),
    process_frame_f32: unsafe extern "C" fn(Ctx, *const f32, *const f32, c_int, *mut f32) -> c_int,
    reset: unsafe extern "C" fn(Ctx),
    last_error: unsafe extern "C" fn(Ctx) -> *const c_char,
    hop_length: unsafe extern "C" fn(Ctx) -> c_int,
}

/// A loaded LocalVQE model. One per meeting; used from one thread at a time.
pub struct LocalVqe {
    api: Api,
    ctx: Ctx,
    // Last, so it is unloaded after the context is freed.
    _library: Library,
}

// The context is only ever used by one thread at a time (the live worker owns it).
unsafe impl Send for LocalVqe {}

impl LocalVqe {
    /// Opens the library at `library` and loads `model` on one CPU thread.
    pub fn load(library: &Path, model: &Path) -> Result<Self, String> {
        if !model.exists() {
            return Err(format!("The echo cancellation model is missing: {}", model.display()));
        }
        // SAFETY: opening LocalVQE's own library; its initialisers only register ggml backends.
        let lib = unsafe { Library::new(library) }.map_err(|e| format!("Could not open {}: {e}", library.display()))?;
        // SAFETY: the signatures match localvqe_api.h at the pinned commit.
        let api = unsafe {
            Api {
                options_new: *lib.get(b"localvqe_options_new\0").map_err(|e| e.to_string())?,
                options_free: *lib.get(b"localvqe_options_free\0").map_err(|e| e.to_string())?,
                options_set_model_path: *lib.get(b"localvqe_options_set_model_path\0").map_err(|e| e.to_string())?,
                options_set_threads: *lib.get(b"localvqe_options_set_threads\0").map_err(|e| e.to_string())?,
                new_with_options: *lib.get(b"localvqe_new_with_options\0").map_err(|e| e.to_string())?,
                free: *lib.get(b"localvqe_free\0").map_err(|e| e.to_string())?,
                process_frame_f32: *lib.get(b"localvqe_process_frame_f32\0").map_err(|e| e.to_string())?,
                reset: *lib.get(b"localvqe_reset\0").map_err(|e| e.to_string())?,
                last_error: *lib.get(b"localvqe_last_error\0").map_err(|e| e.to_string())?,
                hop_length: *lib.get(b"localvqe_hop_length\0").map_err(|e| e.to_string())?,
            }
        };
        let path = CString::new(model.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        // SAFETY: the options handle is created, used and freed here; the path outlives the calls.
        let ctx = unsafe {
            let options = (api.options_new)();
            if options.is_null() {
                return Err("LocalVQE could not allocate its options".into());
            }
            (api.options_set_model_path)(options, path.as_ptr());
            // Its model is small: more threads cost more than they save next to Whisper.
            (api.options_set_threads)(options, 1);
            let ctx = (api.new_with_options)(options);
            (api.options_free)(options);
            ctx
        };
        if ctx.is_null() {
            return Err(format!("LocalVQE could not load {}", model.display()));
        }
        let canceller = Self { api, ctx, _library: lib };
        // SAFETY: ctx is a live context.
        let hop = unsafe { (canceller.api.hop_length)(canceller.ctx) };
        if hop as usize != HOP {
            return Err(format!("LocalVQE works in hops of {hop} samples, Muesli in {HOP}"));
        }
        Ok(canceller)
    }

    /// The library and model next to each other in `dir` (the app's bundled resources).
    pub fn load_from(dir: &Path) -> Result<Self, String> {
        Self::load(&dir.join(LIBRARY), &dir.join(MODEL))
    }

    fn last_error(&self) -> String {
        // SAFETY: valid until the next call on this context; copied at once.
        let message = unsafe { (self.api.last_error)(self.ctx) };
        if message.is_null() {
            return "LocalVQE failed".into();
        }
        unsafe { CStr::from_ptr(message) }.to_string_lossy().into_owned()
    }
}

impl EchoCanceller for LocalVqe {
    /// Its per-hop output comes one hop late (measured: docs/AUDIO-SPIKE.md).
    fn latency(&self) -> usize {
        HOP
    }

    fn process(&mut self, mic: &[f32], reference: &[f32], out: &mut [f32]) -> Result<(), String> {
        if mic.len() != HOP || reference.len() != HOP || out.len() != HOP {
            return Err(format!("LocalVQE takes hops of {HOP} samples"));
        }
        // SAFETY: three buffers of HOP floats, a live context.
        let code = unsafe { (self.api.process_frame_f32)(self.ctx, mic.as_ptr(), reference.as_ptr(), HOP as c_int, out.as_mut_ptr()) };
        if code != 0 {
            return Err(self.last_error());
        }
        Ok(())
    }

    fn reset(&mut self) {
        // SAFETY: a live context.
        unsafe { (self.api.reset)(self.ctx) }
    }
}

impl Drop for LocalVqe {
    fn drop(&mut self) {
        // SAFETY: freed once, before the library is unloaded (field order).
        unsafe { (self.api.free)(self.ctx) }
    }
}
