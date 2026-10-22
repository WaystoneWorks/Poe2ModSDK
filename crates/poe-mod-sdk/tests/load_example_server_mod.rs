//! Loads the real `hello-server` example library the way the server does (`dlopen` /
//! `LoadLibrary`, the two exports, a host table, an event) and checks what it does. The library
//! is built by `cargo build --workspace` (CI does that first). Locally a missing library skips
//! the test unless `POE_SDK_REQUIRE_EXAMPLES=1`.

use std::{ffi::c_void, path::PathBuf, sync::Mutex};

use poe_mod_api::*;

#[derive(Default)]
struct Host {
    commands: Mutex<Vec<(String, String, u32)>>,
    submitted: Mutex<Vec<(u32, u64, String)>>,
    logs: Mutex<Vec<String>>,
    in_init: Mutex<bool>,
}

fn host<'a>(ctx: *const c_void) -> &'a Host {
    unsafe { &*ctx.cast::<Host>() }
}

extern "C" fn log(ctx: *const c_void, _level: u32, m: Str) {
    host(ctx)
        .logs
        .lock()
        .unwrap()
        .push(unsafe { m.as_str() }.to_owned());
}

extern "C" fn config_get(_: *const c_void, _: Str, _: *mut u8, _: usize) -> isize {
    ERR_NOT_FOUND as isize
}

extern "C" fn register_command(ctx: *const c_void, spec: *const CommandSpec) -> i32 {
    let h = host(ctx);
    if !*h.in_init.lock().unwrap() {
        return ERR_NOT_ALLOWED;
    }
    let s = unsafe { &*spec };
    h.commands.lock().unwrap().push((
        unsafe { s.name.as_str() }.to_owned(),
        unsafe { s.help.as_str() }.to_owned(),
        s.role,
    ));
    OK
}

extern "C" fn submit(ctx: *const c_void, scope: *const c_void, c: *const Command) -> i32 {
    if scope.is_null() {
        return ERR_NOT_ALLOWED;
    }
    let c = unsafe { &*c };
    host(ctx).submitted.lock().unwrap().push((
        c.kind,
        c.target,
        unsafe { c.text.as_str() }.to_owned(),
    ));
    OK
}

fn library() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "hello_server.dll"
    } else {
        "libhello_server.so"
    };
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
    // `cargo build` and `cargo test` use the debug profile; release works too.
    ["debug", "release"]
        .iter()
        .map(|p| target.join(p).join(name))
        .find(|p| p.is_file())
}

#[test]
fn the_example_server_mod_loads_registers_hello_and_answers_it() {
    let Some(lib_path) = library() else {
        assert!(
            std::env::var_os("POE_SDK_REQUIRE_EXAMPLES").is_none(),
            "hello-server is not built: run `cargo build --workspace` first"
        );
        eprintln!("skipped: build the workspace first (cargo build --workspace)");
        return;
    };
    let lib = unsafe { libloading::Library::new(&lib_path) }.expect("map the library");
    let abi: libloading::Symbol<ModAbiVersion> =
        unsafe { lib.get(MOD_ABI_VERSION) }.expect("poe_server_mod_abi_version export");
    assert_eq!(unsafe { abi() }, SERVER_API_VERSION);
    let init: libloading::Symbol<ModInit> =
        unsafe { lib.get(MOD_INIT) }.expect("poe_server_mod_init export");

    // The server keeps this table for the whole process.
    let h: &'static Host = Box::leak(Box::default());
    let api: &'static ServerApi = Box::leak(Box::new(ServerApi {
        abi_version: SERVER_API_VERSION,
        size: size_of::<ServerApi>() as u32,
        ctx: std::ptr::from_ref(h).cast(),
        mod_id: Str::new("hello-server"),
        mod_dir: Str::new("mods/hello-server"),
        server_dir: Str::new("."),
        server_version: Str::new("9.9.9"),
        game_version: Str::new("0.5.5e"),
        log,
        config_get,
        register_command,
        submit,
    }));
    let mut info = ModInfo {
        size: size_of::<ModInfo>() as u32,
        reserved: 0,
        event_mask: 0,
        user: std::ptr::null_mut(),
        on_event: None,
        shutdown: None,
    };
    *h.in_init.lock().unwrap() = true;
    assert_eq!(unsafe { init(api, &mut info) }, OK);
    *h.in_init.lock().unwrap() = false;

    assert_eq!(
        h.commands.lock().unwrap().as_slice(),
        [("hello".to_owned(), "Says hello.".to_owned(), ROLE_PLAYER)]
    );
    assert!(
        h.logs
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("loaded, server 9.9.9"))
    );

    // A player runs !hello.
    let player = PlayerInfo {
        size: size_of::<PlayerInfo>() as u32,
        is_admin: 0,
        id: 42,
        entity: 1,
        account: Str::new("acc"),
        character: Str::new("Witch"),
        x: 0.0,
        y: 0.0,
    };
    let scope = 1u8;
    let event = Event {
        size: size_of::<Event>() as u32,
        kind: EVENT_COMMAND,
        frame: 0,
        instance_id: 0,
        scope: std::ptr::from_ref(&scope).cast(),
        snapshot: std::ptr::null(),
        player: &player,
        text: Str::new("hello"),
        args: Str::EMPTY,
    };
    let r = (info.on_event.expect("on_event"))(info.user, &event);
    assert_eq!(r, EVENT_CONSUMED);
    assert_eq!(
        h.submitted.lock().unwrap().as_slice(),
        [(COMMAND_SEND_CHAT, 42, "Hello, Witch!".to_owned())]
    );

    (info.shutdown.expect("shutdown"))(info.user);
    // Mods are never unloaded in the server; dropping `lib` here only ends the test.
    drop(lib);
}
