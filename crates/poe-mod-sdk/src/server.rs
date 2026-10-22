//! Safe wrappers over the server mod ABI ([`poe_mod_api`]): implement [`ServerMod`] for your
//! state, call [`declare_server_mod!`](crate::declare_server_mod) once, and the exports, the
//! pointer checks, the panic containment and the event decoding are done for you.
//!
//! ```ignore
//! use poe_mod_sdk::server::*;
//!
//! struct Hello;
//!
//! impl ServerMod for Hello {
//!     const EVENTS: Events = Events::NONE; // the !hello command is always delivered
//!
//!     fn init(host: &Host) -> Result<Self, String> {
//!         host.register_command("hello", "Says hello.", Role::Player)
//!             .map_err(|e| e.to_string())?;
//!         Ok(Hello)
//!     }
//!
//!     fn event(&self, _host: &Host, event: &Event<'_>, out: &mut Out<'_>) -> Flow {
//!         if let Event::Command { player, .. } = event {
//!             let _ = out.send_chat(player.id(), &format!("Hello, {}!", player.character()));
//!         }
//!         Flow::Continue
//!     }
//! }
//!
//! poe_mod_sdk::declare_server_mod!(Hello);
//! ```

use std::{
    ffi::c_void,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
};

pub use poe_mod_api as api;
use poe_mod_api as raw;

/// Log severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Level {
    fn code(self) -> u32 {
        match self {
            Level::Error => raw::LOG_ERROR,
            Level::Warn => raw::LOG_WARN,
            Level::Info => raw::LOG_INFO,
            Level::Debug => raw::LOG_DEBUG,
            Level::Trace => raw::LOG_TRACE,
        }
    }
}

/// Who may run a registered chat command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Player,
    Admin,
}

/// A host function refused a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiError(pub i32);

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let why = match self.0 {
            raw::ERR_BAD_ARGUMENT => "bad argument",
            raw::ERR_NOT_ALLOWED => "not allowed now",
            raw::ERR_NOT_FOUND => "not found",
            raw::ERR_EXISTS => "already exists",
            raw::ERR_LIMIT => "limit reached",
            _ => "unknown error",
        };
        write!(f, "{why} (code {})", self.0)
    }
}

impl std::error::Error for ApiError {}

fn check(code: i32) -> Result<(), ApiError> {
    if code == raw::OK {
        Ok(())
    } else {
        Err(ApiError(code))
    }
}

/// The server, as a mod sees it: logging, configuration, command registration. Valid for the
/// whole process.
pub struct Host {
    api: *const raw::ServerApi,
}

// The table is read-only and its functions are thread-safe by contract.
unsafe impl Send for Host {}
unsafe impl Sync for Host {}

impl Host {
    fn api(&self) -> &raw::ServerApi {
        // Checked non-null in `__init`; lives for the process.
        unsafe { &*self.api }
    }

    /// The mod's id.
    pub fn id(&self) -> &str {
        unsafe { self.api().mod_id.as_str() }
    }

    /// The mod's folder, for its data files.
    pub fn mod_dir(&self) -> &str {
        unsafe { self.api().mod_dir.as_str() }
    }

    pub fn server_dir(&self) -> &str {
        unsafe { self.api().server_dir.as_str() }
    }

    pub fn server_version(&self) -> &str {
        unsafe { self.api().server_version.as_str() }
    }

    /// The game client version the server speaks, e.g. `0.5.5e`.
    pub fn game_version(&self) -> &str {
        unsafe { self.api().game_version.as_str() }
    }

    pub fn log(&self, level: Level, message: &str) {
        let a = self.api();
        (a.log)(a.ctx, level.code(), raw::Str::new(message));
    }

    pub fn error(&self, message: &str) {
        self.log(Level::Error, message);
    }

    pub fn warn(&self, message: &str) {
        self.log(Level::Warn, message);
    }

    pub fn info(&self, message: &str) {
        self.log(Level::Info, message);
    }

    pub fn debug(&self, message: &str) {
        self.log(Level::Debug, message);
    }

    /// A value of this mod's configuration (`[mods.config.<id>]` of the server's config), as text.
    pub fn config(&self, key: &str) -> Option<String> {
        let a = self.api();
        let mut buf = vec![0u8; 256];
        loop {
            let n = (a.config_get)(a.ctx, raw::Str::new(key), buf.as_mut_ptr(), buf.len());
            if n < 0 {
                return None;
            }
            let n = n as usize;
            if n <= buf.len() {
                buf.truncate(n);
                return String::from_utf8(buf).ok();
            }
            buf.resize(n, 0);
        }
    }

    /// Registers the chat command `!name`. Only from [`ServerMod::init`].
    pub fn register_command(&self, name: &str, help: &str, role: Role) -> Result<(), ApiError> {
        let a = self.api();
        let spec = raw::CommandSpec {
            size: size_of::<raw::CommandSpec>() as u32,
            role: match role {
                Role::Player => raw::ROLE_PLAYER,
                Role::Admin => raw::ROLE_ADMIN,
            },
            name: raw::Str::new(name),
            help: raw::Str::new(help),
        };
        check((a.register_command)(a.ctx, &spec))
    }
}

/// A player in a snapshot.
#[derive(Clone, Copy)]
pub struct Player<'a> {
    raw: &'a raw::PlayerInfo,
}

impl<'a> Player<'a> {
    /// The player's id for this session; pass it to [`Out`] methods.
    pub fn id(&self) -> u64 {
        self.raw.id
    }

    /// The entity id in the instance (0 when not in one).
    pub fn entity(&self) -> u64 {
        self.raw.entity
    }

    pub fn account(&self) -> &'a str {
        unsafe { self.raw.account.as_str() }
    }

    pub fn character(&self) -> &'a str {
        unsafe { self.raw.character.as_str() }
    }

    /// Position in grid cells of the instance.
    pub fn position(&self) -> (f32, f32) {
        (self.raw.x, self.raw.y)
    }

    pub fn is_admin(&self) -> bool {
        self.raw.is_admin != 0
    }
}

impl fmt::Debug for Player<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Player")
            .field("id", &self.id())
            .field("character", &self.character())
            .field("position", &self.position())
            .finish()
    }
}

/// A read-only copy of an instance at one moment.
#[derive(Clone, Copy)]
pub struct Snapshot<'a> {
    raw: &'a raw::Snapshot,
}

impl<'a> Snapshot<'a> {
    /// The instance's frame clock.
    pub fn frame(&self) -> u64 {
        self.raw.frame
    }

    pub fn instance_id(&self) -> u64 {
        self.raw.instance_id
    }

    pub fn area(&self) -> &'a str {
        unsafe { self.raw.area.as_str() }
    }

    pub fn player_count(&self) -> usize {
        self.raw.player_count
    }

    pub fn players(&self) -> impl Iterator<Item = Player<'a>> + 'a {
        unsafe { self.raw.players() }
            .iter()
            .map(|raw| Player { raw })
    }
}

/// Whether the server goes on with its own handling after this mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Consumed,
}

/// An event, decoded. New kinds arrive as [`Event::Unknown`]; match with a wildcard.
#[derive(Debug)]
#[non_exhaustive]
pub enum Event<'a> {
    /// The server finished loading its data and config.
    ServerStart,
    /// The server is shutting down.
    ServerStop,
    /// A player entered an instance.
    PlayerJoin {
        player: Player<'a>,
        snapshot: Option<Snapshot<'a>>,
    },
    /// A player left an instance.
    PlayerLeave {
        player: Player<'a>,
        snapshot: Option<Snapshot<'a>>,
    },
    /// A player ran a command this mod registered.
    Command {
        player: Player<'a>,
        /// The command name without `!`.
        name: &'a str,
        /// The rest of the line.
        args: &'a str,
    },
    InstanceCreated {
        instance_id: u64,
        area: &'a str,
    },
    InstanceClosed {
        instance_id: u64,
        area: &'a str,
    },
    /// One frame of an instance.
    Tick {
        snapshot: Snapshot<'a>,
    },
    /// A kind this SDK version does not know.
    Unknown {
        kind: u32,
    },
}

impl fmt::Debug for Snapshot<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Snapshot")
            .field("frame", &self.frame())
            .field("instance_id", &self.instance_id())
            .field("area", &self.area())
            .field("players", &self.player_count())
            .finish()
    }
}

impl<'a> Event<'a> {
    /// # Safety
    /// `ev` must be a live event from the server (or null).
    unsafe fn decode(ev: *const raw::Event) -> Option<(Event<'a>, *const c_void, u64)> {
        let ev = unsafe { ev.as_ref() }?;
        let snapshot = unsafe { ev.snapshot.as_ref() }.map(|raw| Snapshot { raw });
        let player = unsafe { ev.player.as_ref() }.map(|raw| Player { raw });
        let text = unsafe { ev.text.as_str() };
        let args = unsafe { ev.args.as_str() };
        let event = match ev.kind {
            raw::EVENT_SERVER_START => Event::ServerStart,
            raw::EVENT_SERVER_STOP => Event::ServerStop,
            raw::EVENT_PLAYER_JOIN => Event::PlayerJoin {
                player: player?,
                snapshot,
            },
            raw::EVENT_PLAYER_LEAVE => Event::PlayerLeave {
                player: player?,
                snapshot,
            },
            raw::EVENT_COMMAND => Event::Command {
                player: player?,
                name: text,
                args,
            },
            raw::EVENT_INSTANCE_CREATED => Event::InstanceCreated {
                instance_id: ev.instance_id,
                area: text,
            },
            raw::EVENT_INSTANCE_CLOSED => Event::InstanceClosed {
                instance_id: ev.instance_id,
                area: text,
            },
            raw::EVENT_TICK => Event::Tick {
                snapshot: snapshot?,
            },
            kind => Event::Unknown { kind },
        };
        Some((event, ev.scope, ev.frame))
    }
}

/// The changes a callback asks for. They are queued; the server validates them and applies them
/// on the task that owns the instance after the callback returns.
pub struct Out<'a> {
    host: &'a Host,
    scope: *const c_void,
}

impl Out<'_> {
    fn submit(&mut self, cmd: &raw::Command) -> Result<(), ApiError> {
        let a = self.host.api();
        check((a.submit)(a.ctx, self.scope, cmd))
    }

    /// Sends a server message to one player.
    pub fn send_chat(&mut self, player_id: u64, text: &str) -> Result<(), ApiError> {
        let mut c = raw::Command::new(raw::COMMAND_SEND_CHAT);
        c.target = player_id;
        c.text = raw::Str::new(text);
        self.submit(&c)
    }

    /// Sends a server message to everyone online.
    pub fn broadcast(&mut self, text: &str) -> Result<(), ApiError> {
        let mut c = raw::Command::new(raw::COMMAND_BROADCAST);
        c.text = raw::Str::new(text);
        self.submit(&c)
    }

    /// Moves a player to a cell of their instance.
    pub fn teleport(&mut self, player_id: u64, x: f64, y: f64) -> Result<(), ApiError> {
        let mut c = raw::Command::new(raw::COMMAND_TELEPORT);
        c.target = player_id;
        c.x = x;
        c.y = y;
        self.submit(&c)
    }

    /// Disconnects a player; `reason` is shown to them.
    pub fn kick(&mut self, player_id: u64, reason: &str) -> Result<(), ApiError> {
        let mut c = raw::Command::new(raw::COMMAND_KICK);
        c.target = player_id;
        c.text = raw::Str::new(reason);
        self.submit(&c)
    }
}

/// A set of event kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Events(pub u64);

impl Events {
    pub const NONE: Events = Events(0);
    pub const ALL: Events = Events(raw::EVENTS_ALL);
    pub const SERVER_START: Events = Events(raw::event_bit(raw::EVENT_SERVER_START));
    pub const SERVER_STOP: Events = Events(raw::event_bit(raw::EVENT_SERVER_STOP));
    pub const PLAYER_JOIN: Events = Events(raw::event_bit(raw::EVENT_PLAYER_JOIN));
    pub const PLAYER_LEAVE: Events = Events(raw::event_bit(raw::EVENT_PLAYER_LEAVE));
    pub const INSTANCE_CREATED: Events = Events(raw::event_bit(raw::EVENT_INSTANCE_CREATED));
    pub const INSTANCE_CLOSED: Events = Events(raw::event_bit(raw::EVENT_INSTANCE_CLOSED));
    pub const TICK: Events = Events(raw::event_bit(raw::EVENT_TICK));

    pub const fn or(self, other: Events) -> Events {
        Events(self.0 | other.0)
    }
}

impl std::ops::BitOr for Events {
    type Output = Events;

    fn bitor(self, rhs: Events) -> Events {
        self.or(rhs)
    }
}

/// A native server mod. The type is the mod's state: it is created in [`init`](ServerMod::init)
/// and lives until the server shuts down. Callbacks take `&self` and may run on several threads
/// (different instances run concurrently), so keep mutable state behind a lock or atomics.
pub trait ServerMod: Send + Sync + Sized + 'static {
    /// The events [`event`](ServerMod::event) wants. Commands registered by the mod are always
    /// delivered. `Events::TICK` runs every frame of every instance: subscribe only if needed.
    const EVENTS: Events = Events::NONE;

    /// Called once, right after the library is loaded. Register commands here. An `Err` is
    /// logged and the server goes on without the mod.
    fn init(host: &Host) -> Result<Self, String>;

    /// Called for every subscribed event and for the mod's commands. A panic is contained and
    /// logged; the server may disable a mod that keeps failing.
    fn event(&self, host: &Host, event: &Event<'_>, out: &mut Out<'_>) -> Flow {
        let _ = (host, event, out);
        Flow::Continue
    }

    /// Called once when the server shuts down.
    fn shutdown(&self, host: &Host) {
        let _ = host;
    }
}

struct Instance<M> {
    host: Host,
    state: M,
}

const INIT_FAILED: i32 = 1;
const BAD_HOST: i32 = 2;
const PANICKED: i32 = 3;

fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic".to_owned())
}

/// The body of the generated `poe_server_mod_init`.
///
/// # Safety
/// `api` and `info` are the server's pointers (or null), valid for the call; `api` for the
/// process.
#[doc(hidden)]
pub unsafe fn __init<M: ServerMod>(api: *const raw::ServerApi, info: *mut raw::ModInfo) -> i32 {
    let (Some(a), Some(info)) = (unsafe { api.as_ref() }, unsafe { info.as_mut() }) else {
        return BAD_HOST;
    };
    // API 1 fixes every field the wrapper uses: a host with a smaller table is not API 1.
    if a.abi_version != raw::SERVER_API_VERSION
        || (a.size as usize) < size_of::<raw::ServerApi>()
        || (info.size as usize) < size_of::<raw::ModInfo>()
    {
        return BAD_HOST;
    }
    let host = Host { api };
    match catch_unwind(AssertUnwindSafe(|| M::init(&host))) {
        Ok(Ok(state)) => {
            let boxed = Box::new(Instance { host, state });
            info.event_mask = M::EVENTS.0;
            info.user = Box::into_raw(boxed).cast();
            info.on_event = Some(on_event::<M>);
            info.shutdown = Some(on_shutdown::<M>);
            raw::OK
        }
        Ok(Err(why)) => {
            host.error(&format!("init failed: {why}"));
            INIT_FAILED
        }
        Err(p) => {
            host.error(&format!("init panicked: {}", panic_text(&*p)));
            PANICKED
        }
    }
}

extern "C" fn on_event<M: ServerMod>(user: *mut c_void, ev: *const raw::Event) -> i32 {
    let Some(inst) = (unsafe { user.cast::<Instance<M>>().as_ref() }) else {
        return raw::EVENT_CONTINUE;
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        let Some((event, scope, _frame)) = (unsafe { Event::decode(ev) }) else {
            return Flow::Continue;
        };
        let mut out = Out {
            host: &inst.host,
            scope,
        };
        inst.state.event(&inst.host, &event, &mut out)
    }));
    match result {
        Ok(Flow::Continue) => raw::EVENT_CONTINUE,
        Ok(Flow::Consumed) => raw::EVENT_CONSUMED,
        Err(p) => {
            inst.host
                .error(&format!("event handler panicked: {}", panic_text(&*p)));
            raw::EVENT_CONTINUE
        }
    }
}

extern "C" fn on_shutdown<M: ServerMod>(user: *mut c_void) {
    if user.is_null() {
        return;
    }
    // Takes the state back: nothing is called after this.
    let inst = unsafe { Box::from_raw(user.cast::<Instance<M>>()) };
    if let Err(p) = catch_unwind(AssertUnwindSafe(|| inst.state.shutdown(&inst.host))) {
        inst.host
            .error(&format!("shutdown panicked: {}", panic_text(&*p)));
    }
}

/// Exports `poe_server_mod_abi_version` and `poe_server_mod_init` for a [`ServerMod`] type. Use it
/// once, at the top level of the crate; the library must be a `cdylib`.
#[macro_export]
macro_rules! declare_server_mod {
    ($ty:ty) => {
        /// The server API this mod was built against.
        #[unsafe(no_mangle)]
        pub extern "C" fn poe_server_mod_abi_version() -> u32 {
            $crate::server::api::SERVER_API_VERSION
        }

        /// # Safety
        /// Called by the server with its own pointers.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn poe_server_mod_init(
            api: *const $crate::server::api::ServerApi,
            info: *mut $crate::server::api::ModInfo,
        ) -> i32 {
            unsafe { $crate::server::__init::<$ty>(api, info) }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// (kind, target, x, y, text) of a submitted command.
    type Submitted = (u32, u64, f64, f64, String);

    /// A fake server: records what the mod does.
    #[derive(Default)]
    struct Fake {
        logs: Mutex<Vec<(u32, String)>>,
        commands: Mutex<Vec<(String, String, u32)>>,
        submitted: Mutex<Vec<Submitted>>,
        config: Mutex<Vec<(String, String)>>,
        in_init: Mutex<bool>,
    }

    fn fake<'a>(ctx: *const c_void) -> &'a Fake {
        unsafe { &*ctx.cast::<Fake>() }
    }

    extern "C" fn f_log(ctx: *const c_void, level: u32, message: raw::Str) {
        fake(ctx)
            .logs
            .lock()
            .unwrap()
            .push((level, unsafe { message.as_str() }.to_owned()));
    }

    extern "C" fn f_config(ctx: *const c_void, key: raw::Str, buf: *mut u8, cap: usize) -> isize {
        let f = fake(ctx);
        let key = unsafe { key.as_str() };
        let cfg = f.config.lock().unwrap();
        let Some((_, v)) = cfg.iter().find(|(k, _)| k == key) else {
            return raw::ERR_NOT_FOUND as isize;
        };
        let n = v.len().min(cap);
        if n > 0 {
            unsafe { std::ptr::copy_nonoverlapping(v.as_ptr(), buf, n) };
        }
        v.len() as isize
    }

    extern "C" fn f_register(ctx: *const c_void, spec: *const raw::CommandSpec) -> i32 {
        let f = fake(ctx);
        if !*f.in_init.lock().unwrap() {
            return raw::ERR_NOT_ALLOWED;
        }
        let spec = unsafe { &*spec };
        let name = unsafe { spec.name.as_str() }.to_owned();
        let mut cmds = f.commands.lock().unwrap();
        if cmds.iter().any(|c| c.0 == name) {
            return raw::ERR_EXISTS;
        }
        cmds.push((name, unsafe { spec.help.as_str() }.to_owned(), spec.role));
        raw::OK
    }

    extern "C" fn f_submit(
        ctx: *const c_void,
        scope: *const c_void,
        c: *const raw::Command,
    ) -> i32 {
        if scope.is_null() {
            return raw::ERR_NOT_ALLOWED;
        }
        let c = unsafe { &*c };
        fake(ctx).submitted.lock().unwrap().push((
            c.kind,
            c.target,
            c.x,
            c.y,
            unsafe { c.text.as_str() }.to_owned(),
        ));
        raw::OK
    }

    fn api_for(f: &Fake) -> raw::ServerApi {
        raw::ServerApi {
            abi_version: raw::SERVER_API_VERSION,
            size: size_of::<raw::ServerApi>() as u32,
            ctx: std::ptr::from_ref(f).cast(),
            mod_id: raw::Str::new("test-mod"),
            mod_dir: raw::Str::new("/srv/mods/test-mod"),
            server_dir: raw::Str::new("/srv"),
            server_version: raw::Str::new("9.9.9"),
            game_version: raw::Str::new("0.5.5e"),
            log: f_log,
            config_get: f_config,
            register_command: f_register,
            submit: f_submit,
        }
    }

    fn new_info() -> raw::ModInfo {
        raw::ModInfo {
            size: size_of::<raw::ModInfo>() as u32,
            reserved: 0,
            event_mask: 0,
            user: std::ptr::null_mut(),
            on_event: None,
            shutdown: None,
        }
    }

    static SHUTDOWNS: AtomicUsize = AtomicUsize::new(0);

    struct Echo;

    impl ServerMod for Echo {
        const EVENTS: Events = Events::PLAYER_JOIN.or(Events::TICK);

        fn init(host: &Host) -> Result<Self, String> {
            host.register_command("echo", "Echoes.", Role::Player)
                .map_err(|e| e.to_string())?;
            host.register_command("kickall", "Admin.", Role::Admin)
                .map_err(|e| e.to_string())?;
            if host.config("fail").is_some() {
                return Err("config says fail".into());
            }
            if host.config("panic").is_some() {
                panic!("boom in init");
            }
            Ok(Echo)
        }

        fn event(&self, host: &Host, event: &Event<'_>, out: &mut Out<'_>) -> Flow {
            match event {
                Event::Command { player, name, args } => {
                    if *args == "panic" {
                        panic!("boom");
                    }
                    out.send_chat(player.id(), &format!("{name}: {args}"))
                        .unwrap();
                    out.teleport(player.id(), 10.5, 20.0).unwrap();
                    Flow::Consumed
                }
                Event::PlayerJoin { player, snapshot } => {
                    host.info(&format!(
                        "join {} of {}",
                        player.character(),
                        snapshot.map_or(0, |s| s.player_count())
                    ));
                    out.broadcast("welcome").unwrap();
                    Flow::Continue
                }
                Event::Tick { snapshot } => {
                    host.debug(&format!("tick {} {}", snapshot.frame(), snapshot.area()));
                    Flow::Continue
                }
                _ => Flow::Continue,
            }
        }
    }

    /// Counts its own shutdowns (the other tests run in parallel and shut `Echo` down too).
    struct Counter;

    impl ServerMod for Counter {
        fn init(_host: &Host) -> Result<Self, String> {
            Ok(Counter)
        }

        fn shutdown(&self, _host: &Host) {
            SHUTDOWNS.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// The API table must outlive the mod, as it does in the server: leak it.
    fn start(f: &Fake) -> (&'static raw::ServerApi, raw::ModInfo, i32) {
        let api: &'static raw::ServerApi = Box::leak(Box::new(api_for(f)));
        let mut info = new_info();
        *f.in_init.lock().unwrap() = true;
        let code = unsafe { __init::<Echo>(api, &mut info) };
        *f.in_init.lock().unwrap() = false;
        (api, info, code)
    }

    fn player(name: &str) -> (raw::PlayerInfo, String) {
        let name = name.to_owned();
        (
            raw::PlayerInfo {
                size: size_of::<raw::PlayerInfo>() as u32,
                is_admin: 0,
                id: 77,
                entity: 5,
                account: raw::Str::new("acc"),
                character: raw::Str::new("Ranger"),
                x: 1.0,
                y: 2.0,
            },
            name,
        )
    }

    fn event(kind: u32) -> raw::Event {
        raw::Event {
            size: size_of::<raw::Event>() as u32,
            kind,
            frame: 0,
            instance_id: 0,
            scope: std::ptr::null(),
            snapshot: std::ptr::null(),
            player: std::ptr::null(),
            text: raw::Str::EMPTY,
            args: raw::Str::EMPTY,
        }
    }

    #[test]
    fn init_registers_commands_and_fills_in_the_info() {
        let f = Fake::default();
        let (_api, info, code) = start(&f);
        assert_eq!(code, raw::OK);
        assert_eq!(info.event_mask, Events::PLAYER_JOIN.0 | Events::TICK.0);
        assert!(info.on_event.is_some() && info.shutdown.is_some() && !info.user.is_null());
        let cmds = f.commands.lock().unwrap();
        assert_eq!(cmds[0], ("echo".into(), "Echoes.".into(), raw::ROLE_PLAYER));
        assert_eq!(cmds[1].2, raw::ROLE_ADMIN);
        drop(cmds);
        (info.shutdown.unwrap())(info.user);
    }

    #[test]
    fn a_command_event_reaches_the_mod_and_its_commands_are_submitted() {
        let f = Fake::default();
        let (_api, info, _) = start(&f);
        let (p, _) = player("x");
        let mut ev = event(raw::EVENT_COMMAND);
        ev.player = &p;
        ev.text = raw::Str::new("echo");
        ev.args = raw::Str::new("hi there");
        let marker = 1u8;
        ev.scope = std::ptr::from_ref(&marker).cast();
        let r = (info.on_event.unwrap())(info.user, &ev);
        assert_eq!(r, raw::EVENT_CONSUMED);
        let sub = f.submitted.lock().unwrap();
        assert_eq!(
            sub[0],
            (
                raw::COMMAND_SEND_CHAT,
                77,
                0.0,
                0.0,
                "echo: hi there".into()
            )
        );
        assert_eq!(
            sub[1],
            (raw::COMMAND_TELEPORT, 77, 10.5, 20.0, String::new())
        );
        drop(sub);
        (info.shutdown.unwrap())(info.user);
    }

    #[test]
    fn submitting_outside_a_callback_is_refused_by_the_host() {
        let f = Fake::default();
        let (_api, info, _) = start(&f);
        let (p, _) = player("x");
        let mut ev = event(raw::EVENT_COMMAND);
        ev.player = &p;
        ev.text = raw::Str::new("echo");
        // scope stays null: the fake host answers ERR_NOT_ALLOWED and the mod's unwrap panics,
        // which the wrapper contains.
        let r = (info.on_event.unwrap())(info.user, &ev);
        assert_eq!(r, raw::EVENT_CONTINUE);
        let logs = f.logs.lock().unwrap();
        assert!(
            logs.iter()
                .any(|(l, m)| *l == raw::LOG_ERROR && m.contains("panicked")),
            "{logs:?}"
        );
        drop(logs);
        (info.shutdown.unwrap())(info.user);
    }

    #[test]
    fn snapshot_events_decode_with_their_players() {
        let f = Fake::default();
        let (_api, info, _) = start(&f);
        let (p, _) = player("x");
        let snap = raw::Snapshot {
            size: size_of::<raw::Snapshot>() as u32,
            reserved: 0,
            frame: 1234,
            instance_id: 3,
            area: raw::Str::new("1_1_1"),
            players: &p,
            player_count: 1,
        };
        let marker = 1u8;
        let mut ev = event(raw::EVENT_PLAYER_JOIN);
        ev.player = &p;
        ev.snapshot = &snap;
        ev.scope = std::ptr::from_ref(&marker).cast();
        (info.on_event.unwrap())(info.user, &ev);
        let mut tick = event(raw::EVENT_TICK);
        tick.snapshot = &snap;
        (info.on_event.unwrap())(info.user, &tick);
        let logs = f.logs.lock().unwrap();
        assert!(
            logs.iter().any(|(_, m)| m == "join Ranger of 1"),
            "{logs:?}"
        );
        assert!(logs.iter().any(|(_, m)| m == "tick 1234 1_1_1"), "{logs:?}");
        drop(logs);
        assert_eq!(f.submitted.lock().unwrap()[0].0, raw::COMMAND_BROADCAST);
        (info.shutdown.unwrap())(info.user);
    }

    #[test]
    fn unknown_kinds_and_missing_players_are_ignored() {
        let f = Fake::default();
        let (_api, info, _) = start(&f);
        assert_eq!(
            (info.on_event.unwrap())(info.user, &event(999)),
            raw::EVENT_CONTINUE
        );
        // A join without a player cannot be decoded: ignored, no panic.
        assert_eq!(
            (info.on_event.unwrap())(info.user, &event(raw::EVENT_PLAYER_JOIN)),
            raw::EVENT_CONTINUE
        );
        assert_eq!(
            (info.on_event.unwrap())(info.user, std::ptr::null()),
            raw::EVENT_CONTINUE
        );
        assert!(f.logs.lock().unwrap().is_empty());
        (info.shutdown.unwrap())(info.user);
    }

    #[test]
    fn a_panic_in_a_handler_is_contained_and_logged() {
        let f = Fake::default();
        let (_api, info, _) = start(&f);
        let (p, _) = player("x");
        let mut ev = event(raw::EVENT_COMMAND);
        ev.player = &p;
        ev.text = raw::Str::new("echo");
        ev.args = raw::Str::new("panic");
        let r = (info.on_event.unwrap())(info.user, &ev);
        assert_eq!(r, raw::EVENT_CONTINUE);
        assert!(
            f.logs
                .lock()
                .unwrap()
                .iter()
                .any(|(_, m)| m.contains("event handler panicked: boom"))
        );
        (info.shutdown.unwrap())(info.user);
    }

    #[test]
    fn init_failures_and_panics_return_codes_and_log() {
        let f = Fake::default();
        f.config.lock().unwrap().push(("fail".into(), "1".into()));
        let (_a, info, code) = start(&f);
        assert_eq!(code, INIT_FAILED);
        assert!(info.on_event.is_none() && info.user.is_null());
        assert!(
            f.logs
                .lock()
                .unwrap()
                .iter()
                .any(|(_, m)| m.contains("init failed: config says fail"))
        );

        let f = Fake::default();
        f.config.lock().unwrap().push(("panic".into(), "1".into()));
        let (_a, _info, code) = start(&f);
        assert_eq!(code, PANICKED);
        assert!(
            f.logs
                .lock()
                .unwrap()
                .iter()
                .any(|(_, m)| m.contains("init panicked: boom in init"))
        );
    }

    #[test]
    fn a_mismatched_host_is_refused_without_touching_it() {
        let f = Fake::default();
        let mut api = api_for(&f);
        let mut info = new_info();
        api.abi_version = 2;
        assert_eq!(unsafe { __init::<Echo>(&api, &mut info) }, BAD_HOST);
        api.abi_version = raw::SERVER_API_VERSION;
        api.size = 16;
        assert_eq!(unsafe { __init::<Echo>(&api, &mut info) }, BAD_HOST);
        api.size = size_of::<raw::ServerApi>() as u32;
        info.size = 8;
        assert_eq!(unsafe { __init::<Echo>(&api, &mut info) }, BAD_HOST);
        assert_eq!(
            unsafe { __init::<Echo>(std::ptr::null(), std::ptr::null_mut()) },
            BAD_HOST
        );
        assert!(f.commands.lock().unwrap().is_empty());
    }

    #[test]
    fn config_grows_its_buffer_and_reports_missing_keys() {
        let f = Fake::default();
        let long = "x".repeat(1000);
        f.config.lock().unwrap().push(("long".into(), long.clone()));
        f.config.lock().unwrap().push(("short".into(), "ok".into()));
        let api = api_for(&f);
        let host = Host { api: &api };
        assert_eq!(host.config("long").as_deref(), Some(long.as_str()));
        assert_eq!(host.config("short").as_deref(), Some("ok"));
        assert_eq!(host.config("nope"), None);
        assert_eq!(host.id(), "test-mod");
        assert_eq!(host.game_version(), "0.5.5e");
        assert_eq!(host.mod_dir(), "/srv/mods/test-mod");
    }

    #[test]
    fn registering_after_init_and_duplicates_are_errors() {
        let f = Fake::default();
        let (api, info, _) = start(&f);
        let host = Host { api };
        assert_eq!(
            host.register_command("late", "x", Role::Player),
            Err(ApiError(raw::ERR_NOT_ALLOWED))
        );
        *f.in_init.lock().unwrap() = true;
        assert_eq!(
            host.register_command("echo", "x", Role::Player)
                .unwrap_err()
                .to_string(),
            "already exists (code -4)"
        );
        (info.shutdown.unwrap())(info.user);
    }

    #[test]
    fn shutdown_runs_once_and_frees_the_state() {
        let f = Fake::default();
        let api: &'static raw::ServerApi = Box::leak(Box::new(api_for(&f)));
        let mut info = new_info();
        assert_eq!(unsafe { __init::<Counter>(api, &mut info) }, raw::OK);
        assert_eq!(info.event_mask, 0);
        (info.shutdown.unwrap())(info.user);
        assert_eq!(SHUTDOWNS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn events_combine() {
        assert_eq!((Events::TICK | Events::SERVER_START).0, 0b1000_0001);
        assert_eq!(Events::ALL.0, 0xFF);
    }
}
