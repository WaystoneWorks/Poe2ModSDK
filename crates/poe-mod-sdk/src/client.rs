//! Client mods: the entry points the client hook's loader (`version.dll`) calls.
//!
//! ```ignore
//! use poe_mod_sdk::client::HookApi;
//!
//! fn init(api: &HookApi) -> Result<(), String> {
//!     api.info(&format!("hello from {}: game build {}", api.id(), api.game_version()));
//!     Ok(())
//! }
//!
//! poe_mod_sdk::declare_client_mod!(init);
//! ```
//!
//! The loader runs a mod inside the game process: honour [`HookApi::read_only`], and never
//! touch anything in official mode (the loader loads no mod there).

use std::panic::{AssertUnwindSafe, catch_unwind};

pub use poe_hook_mod_api as api;
pub use poe_hook_mod_api::{
    HOOK_API_VERSION, HookApi, LOG_DEBUG, LOG_ERROR, LOG_INFO, LOG_TRACE, LOG_WARN, Str,
};

const BAD_HOST: i32 = 2;
const INIT_FAILED: i32 = 1;
const PANICKED: i32 = 3;

/// The body of the generated `poe_mod_init`.
///
/// # Safety
/// `api` is the loader's pointer (or null), valid for the process.
#[doc(hidden)]
pub unsafe fn __init(api: *const HookApi, init: fn(&HookApi) -> Result<(), String>) -> i32 {
    let Some(api) = (unsafe { api.as_ref() }) else {
        return 1;
    };
    if api.abi_version != HOOK_API_VERSION {
        return BAD_HOST;
    }
    match catch_unwind(AssertUnwindSafe(|| init(api))) {
        Ok(Ok(())) => 0,
        Ok(Err(why)) => {
            api.log(LOG_ERROR, &format!("init failed: {why}"));
            INIT_FAILED
        }
        Err(_) => {
            api.log(LOG_ERROR, "init panicked");
            PANICKED
        }
    }
}

/// Exports `poe_mod_abi_version` and `poe_mod_init` for a client mod whose entry point is
/// `fn(&HookApi) -> Result<(), String>`. An `Err` or a panic is logged and makes the loader
/// unload the DLL and skip the mod. Use it once, at the top level of the crate.
#[macro_export]
macro_rules! declare_client_mod {
    ($init:path) => {
        /// The mod ABI this mod was built against (`client.hook_api` in `mod.toml`).
        #[unsafe(no_mangle)]
        pub extern "C" fn poe_mod_abi_version() -> u32 {
            $crate::client::HOOK_API_VERSION
        }

        /// # Safety
        /// Called by the loader with its `HookApi`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn poe_mod_init(api: *const $crate::client::HookApi) -> i32 {
            unsafe { $crate::client::__init(api, $init) }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::c_void;
    use std::sync::Mutex;

    static LOGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    extern "C" fn log(_ctx: *const c_void, _level: u32, m: Str) {
        LOGS.lock().unwrap().push(unsafe { m.as_str() }.to_owned());
    }
    extern "C" fn ro() -> bool {
        false
    }
    extern "C" fn sig(_n: Str) -> usize {
        0
    }
    extern "C" fn feat(_n: Str) -> bool {
        false
    }
    extern "C" fn field(_n: Str, _o: *mut usize) -> bool {
        false
    }

    fn hook_api(version: u32) -> HookApi {
        HookApi {
            abi_version: version,
            size: size_of::<HookApi>() as u32,
            ctx: std::ptr::null(),
            mod_id: Str::new("t"),
            mod_dir: Str::new("d"),
            exe_dir: Str::new("e"),
            game_version: Str::new("0.5.5f"),
            build_revision: Str::new("1"),
            build_verified: 1,
            reserved: 0,
            log,
            read_only: ro,
            sig_addr: sig,
            feature_ok: feat,
            field,
        }
    }

    #[test]
    fn init_results_map_to_loader_codes_and_log() {
        let api = hook_api(HOOK_API_VERSION);
        assert_eq!(unsafe { __init(&api, |_| Ok(())) }, 0);
        assert_eq!(unsafe { __init(&api, |_| Err("nope".into())) }, INIT_FAILED);
        assert_eq!(unsafe { __init(&api, |_| panic!("x")) }, PANICKED);
        assert_eq!(unsafe { __init(std::ptr::null(), |_| Ok(())) }, 1);
        let old = hook_api(HOOK_API_VERSION + 1);
        assert_eq!(unsafe { __init(&old, |_| Ok(())) }, BAD_HOST);
        let logs = LOGS.lock().unwrap();
        assert!(logs.iter().any(|m| m == "init failed: nope"), "{logs:?}");
        assert!(logs.iter().any(|m| m == "init panicked"), "{logs:?}");
    }
}
