//! The System Audio Recording permission, read through the private TCC framework.
//!
//! macOS has no public call that says whether "System Audio Recording Only" is allowed. This reads
//! the TCC service that pane controls (`kTCCServiceAudioCapture`), as the former Swift app did.
//! It is the one private API in Muesli: re-check it on every major macOS release. If the
//! framework or its symbols are missing, the status is `Unknown` and asking falls back to opening
//! the tap, which makes macOS ask.

use std::ffi::{c_void, CStr};
use std::sync::{mpsc, OnceLock};
use std::time::Duration;

use block2::RcBlock;
use muesli_core::ports::Access;
use objc2_core_foundation::CFString;

const FRAMEWORK: &CStr = c"/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC";
const SERVICE: &str = "kTCCServiceAudioCapture";

/// `int TCCAccessPreflight(CFStringRef service, CFDictionaryRef options)`: 0 allowed, 1 denied,
/// anything else not determined.
type Preflight = unsafe extern "C" fn(*const CFString, *const c_void) -> i32;
/// `void TCCAccessRequest(CFStringRef service, CFDictionaryRef options, void (^)(Boolean))`.
type Request = unsafe extern "C" fn(*const CFString, *const c_void, *mut block2::Block<dyn Fn(u8)>);

struct Symbols {
    preflight: Option<Preflight>,
    request: Option<Request>,
}

fn symbols() -> &'static Symbols {
    static SYMBOLS: OnceLock<Symbols> = OnceLock::new();
    SYMBOLS.get_or_init(|| unsafe {
        let handle = libc::dlopen(FRAMEWORK.as_ptr(), libc::RTLD_NOW);
        if handle.is_null() {
            return Symbols { preflight: None, request: None };
        }
        let find = |name: &CStr| {
            let symbol = libc::dlsym(handle, name.as_ptr());
            (!symbol.is_null()).then_some(symbol)
        };
        Symbols {
            preflight: find(c"TCCAccessPreflight").map(|s| std::mem::transmute::<*mut c_void, Preflight>(s)),
            request: find(c"TCCAccessRequest").map(|s| std::mem::transmute::<*mut c_void, Request>(s)),
        }
    })
}

pub fn system_audio() -> Access {
    let Some(preflight) = symbols().preflight else { return Access::Unknown };
    let service = CFString::from_static_str(SERVICE);
    match unsafe { preflight(&*service, std::ptr::null()) } {
        0 => Access::Allowed,
        1 => Access::Denied,
        _ => Access::NotAsked,
    }
}

/// Shows the system's question. `None` when the private call is not available.
pub fn request_system_audio() -> Option<Result<(), String>> {
    let request = symbols().request?;
    let (tx, rx) = mpsc::channel::<bool>();
    let block = RcBlock::new(move |granted: u8| {
        let _ = tx.send(granted != 0);
    });
    let service = CFString::from_static_str(SERVICE);
    unsafe { request(&*service, std::ptr::null(), RcBlock::as_ptr(&block)) };
    Some(match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(true) => Ok(()),
        _ => Err("System audio recording was not allowed.".into()),
    })
}
