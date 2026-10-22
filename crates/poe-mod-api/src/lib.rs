//! The **server mod ABI** (server API 1): what a native server mod exports, what the server
//! hands it and what it hands back. Stability rules: `docs/abi-stability.md`. Guide for authors:
//! `docs/writing-server-mods.md`. The client's counterpart is `poe-hook-mod-api` (`HookApi`);
//! the two are independent and never share a number or a struct.
//!
//! A server mod is a `cdylib` (`.so` on Linux, `.dll` on Windows) named in the package's
//! `mod.toml` under `[server.library]`, built against the same `SERVER_API_VERSION` it states as
//! `server.server_api`. It exports
//!
//! ```text
//! extern "C" fn poe_server_mod_abi_version() -> u32                        // required
//! extern "C" fn poe_server_mod_init(api: *const ServerApi, info: *mut ModInfo) -> i32   // required
//! ```
//!
//! The server maps the library, calls `poe_server_mod_abi_version` first and refuses the mod
//! (before calling anything else) when it differs from its own [`SERVER_API_VERSION`]. In
//! `poe_server_mod_init` the mod reads the [`ServerApi`] table, registers its chat commands and
//! fills in [`ModInfo`] (its state pointer, the events it wants and its callbacks). From then on
//! the server calls [`ModInfo::on_event`] for those events. There is no unloading and no hot
//! reload: a mod stays loaded until the server process ends, when [`ModInfo::shutdown`] runs.
//!
//! **The world is never shared.** An event carries a read-only [`Snapshot`] (plain data copied
//! at that moment, valid for the call). A mod changes the world by submitting [`Command`]s
//! ([`ServerApi::submit`]); the server validates them and applies them on the task that owns the
//! instance after the callback returns. A mod never holds a reference into the server.
//!
//! **Threads.** Callbacks for different instances run concurrently on different threads; callbacks
//! of one instance never overlap, and [`EVENT_COMMAND`] runs on the caller's session task. A mod's
//! state must therefore be `Send + Sync` (the Rust SDK enforces it). A callback must return
//! quickly: a tick has a budget of milliseconds and the server may disable a mod that overruns.
//!
//! Only `#[repr(C)]` data and `extern "C"` functions cross the boundary: the server and every mod
//! have their own Rust runtime and allocator, so nothing is allocated on one side and freed on
//! the other. Strings are borrowed UTF-8 ([`Str`]) valid for the call only (except the ones
//! inside [`ServerApi`], which live for the process).
//!
//! **Versioning.** [`SERVER_API_VERSION`] is bumped whenever a field or function below changes
//! meaning, moves or is removed, or an existing event kind or command kind changes meaning.
//! *Appending* a field at the end of [`ServerApi`], [`ModInfo`], [`Event`], [`Snapshot`],
//! [`PlayerInfo`], [`Command`] or [`CommandSpec`], and adding new event or command kinds, does
//! not need a bump: every struct starts with its `size`, and a reader uses a field only when
//! `size` covers it. The layout test in this file fails when anything changes, so the bump is not
//! forgotten.

use std::ffi::c_void;

/// The server mod ABI version (`server.server_api` in `mod.toml`).
pub const SERVER_API_VERSION: u32 = 1;
/// Name of the required export [`ModAbiVersion`].
pub const MOD_ABI_VERSION: &[u8] = b"poe_server_mod_abi_version\0";
/// Name of the required entry point [`ModInit`].
pub const MOD_INIT: &[u8] = b"poe_server_mod_init\0";

pub type ModAbiVersion = unsafe extern "C" fn() -> u32;
/// Returns 0 when the mod started. Anything else: the server logs it, unloads the library and
/// goes on without the mod, so the mod must have undone what it did.
pub type ModInit = unsafe extern "C" fn(api: *const ServerApi, info: *mut ModInfo) -> i32;
/// An event callback. Returns [`EVENT_CONTINUE`] or [`EVENT_CONSUMED`].
pub type OnEvent = extern "C" fn(user: *mut c_void, event: *const Event) -> i32;
/// Runs once when the server shuts down.
pub type OnShutdown = extern "C" fn(user: *mut c_void);

/// Success.
pub const OK: i32 = 0;
/// A host function got an argument it cannot use (null, not UTF-8, out of range, bad size).
pub const ERR_BAD_ARGUMENT: i32 = -1;
/// The call is not allowed now (e.g. registering a command after `poe_server_mod_init`
/// returned, or submitting a command outside an event callback).
pub const ERR_NOT_ALLOWED: i32 = -2;
/// The thing named does not exist (an unknown player, an unknown kind).
pub const ERR_NOT_FOUND: i32 = -3;
/// Already exists (a command name that is taken).
pub const ERR_EXISTS: i32 = -4;
/// A limit was reached (too many commands, too many queued commands in one callback).
pub const ERR_LIMIT: i32 = -5;

/// `log::Level as u32`.
pub const LOG_ERROR: u32 = 1;
pub const LOG_WARN: u32 = 2;
pub const LOG_INFO: u32 = 3;
pub const LOG_DEBUG: u32 = 4;
pub const LOG_TRACE: u32 = 5;

// ---- events ----

/// The server finished loading its data and config. Server scope.
pub const EVENT_SERVER_START: u32 = 1;
/// The server is shutting down (before [`OnShutdown`]). Server scope.
pub const EVENT_SERVER_STOP: u32 = 2;
/// A player entered an instance (a login, or a transfer between areas). Instance scope:
/// [`Event::player`] is the player, [`Event::snapshot`] includes them.
pub const EVENT_PLAYER_JOIN: u32 = 3;
/// A player left an instance (logout or transfer). Instance scope; the snapshot no longer
/// includes them.
pub const EVENT_PLAYER_LEAVE: u32 = 4;
/// A player ran a chat command this mod registered. [`Event::text`] is the command name without
/// the `!`, [`Event::args`] the rest of the line, [`Event::player`] the caller. The role check of
/// the command has passed.
pub const EVENT_COMMAND: u32 = 5;
/// An instance was created. [`Event::text`] is the area id, [`Event::instance_id`] the instance.
pub const EVENT_INSTANCE_CREATED: u32 = 6;
/// An instance was closed (after its last player left or by an admin).
pub const EVENT_INSTANCE_CLOSED: u32 = 7;
/// One frame of an instance, after the simulation step. Instance scope, the hot path: keep it
/// cheap, subscribe only when needed.
pub const EVENT_TICK: u32 = 8;

/// The bit of `kind` in [`ModInfo::event_mask`].
pub const fn event_bit(kind: u32) -> u64 {
    1u64 << (kind - 1)
}
/// Every event kind of API 1.
pub const EVENTS_ALL: u64 = (1 << 8) - 1;

/// The server goes on with the next mod, or with its own handling.
pub const EVENT_CONTINUE: i32 = 0;
/// The mod handled the event. For events the server could otherwise handle itself (later API
/// versions: chat, movement) this stops the server's default; for the notification events of
/// API 1 it has no effect.
pub const EVENT_CONSUMED: i32 = 1;

// ---- commands the mod submits ----

/// Send `text` to the player `target` as a server message.
pub const COMMAND_SEND_CHAT: u32 = 1;
/// Send `text` to every player online.
pub const COMMAND_BROADCAST: u32 = 2;
/// Move the player `target` to the cell (`x`, `y`) of their current instance.
pub const COMMAND_TELEPORT: u32 = 3;
/// Disconnect the player `target`; `text` is the reason shown to them.
pub const COMMAND_KICK: u32 = 4;

/// Who may run a registered chat command.
pub const ROLE_PLAYER: u32 = 0;
pub const ROLE_ADMIN: u32 = 1;

/// Borrowed UTF-8 string.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Str {
    pub ptr: *const u8,
    pub len: usize,
}

// Only ever points at data that outlives its use (leaked strings, or a callback's arguments).
unsafe impl Send for Str {}
unsafe impl Sync for Str {}

impl Str {
    pub const EMPTY: Str = Str {
        ptr: std::ptr::null(),
        len: 0,
    };

    pub fn new(s: &str) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }

    /// # Safety
    /// The pointer must be valid for `len` bytes of UTF-8 for `'a`.
    pub unsafe fn as_str<'a>(&self) -> &'a str {
        if self.len == 0 || self.ptr.is_null() {
            return "";
        }
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.ptr, self.len)) }
    }
}

/// What the server hands `poe_server_mod_init`: one instance per mod, valid for the whole
/// process. A later API version only appends fields; read `size` before using a field this
/// crate's version did not have.
#[repr(C)]
pub struct ServerApi {
    /// [`SERVER_API_VERSION`] of the server.
    pub abi_version: u32,
    /// `size_of::<ServerApi>()` of the server: fields past this size do not exist.
    pub size: u32,
    /// Pass as the first argument of every host function below.
    pub ctx: *const c_void,
    /// The mod's id (its folder name under `mods/`).
    pub mod_id: Str,
    /// The mod's folder (`<server dir>/mods/<id>`), for its data files.
    pub mod_dir: Str,
    /// The server's working directory.
    pub server_dir: Str,
    /// The server's version, e.g. `0.3.1`.
    pub server_version: Str,
    /// The game client version the server speaks, e.g. `0.5.5e`.
    pub game_version: Str,
    /// Writes a record to the server log, tagged `mod:<id>`. `level` is `LOG_*`.
    pub log: extern "C" fn(ctx: *const c_void, level: u32, message: Str),
    /// Reads `key` of this mod's configuration (`[mods.config.<id>]` in `server.toml`, rendered as
    /// text) into `buf` (up to `cap` bytes, not NUL-terminated). Returns the value's full length
    /// in bytes (larger than `cap` means it was cut; call again with a bigger buffer), or a
    /// negative [`ERR_NOT_FOUND`] when the key is not set. `buf` may be null with `cap` 0 to ask
    /// for the length.
    pub config_get: extern "C" fn(ctx: *const c_void, key: Str, buf: *mut u8, cap: usize) -> isize,
    /// Registers a chat command (`!name`). Only during `poe_server_mod_init`. The server copies
    /// the strings. Returns [`OK`] or an error ([`ERR_EXISTS`] when the name is taken by the
    /// server or another mod, [`ERR_BAD_ARGUMENT`] for an invalid name, [`ERR_NOT_ALLOWED`]
    /// after init, [`ERR_LIMIT`]). The command's events arrive as [`EVENT_COMMAND`] whatever the
    /// event mask says.
    pub register_command: extern "C" fn(ctx: *const c_void, spec: *const CommandSpec) -> i32,
    /// Queues a [`Command`] for the world. Only while an event callback runs: `scope` is the
    /// [`Event::scope`] of that event. The server validates the command and applies it on the
    /// task that owns the instance after the callback returns; an invalid command is logged and
    /// dropped. Returns [`OK`], [`ERR_NOT_ALLOWED`] (no callback running), [`ERR_BAD_ARGUMENT`]
    /// or [`ERR_LIMIT`] (too many queued).
    pub submit:
        extern "C" fn(ctx: *const c_void, scope: *const c_void, command: *const Command) -> i32,
}

unsafe impl Send for ServerApi {}
unsafe impl Sync for ServerApi {}

/// What the mod fills in during `poe_server_mod_init`. The server zero-initialises it, sets
/// `size`, and ignores everything the mod leaves at its default.
#[repr(C)]
pub struct ModInfo {
    /// `size_of::<ModInfo>()` of the server. A mod built against a newer API than the server's
    /// must not write fields past this size.
    pub size: u32,
    pub reserved: u32,
    /// The events the mod wants, as a mask of [`event_bit`]s. 0 means none ([`EVENT_COMMAND`] for
    /// the mod's own commands is always delivered).
    pub event_mask: u64,
    /// Passed back as the first argument of every callback.
    pub user: *mut c_void,
    /// Called for each subscribed event.
    pub on_event: Option<OnEvent>,
    /// Called once at server shutdown.
    pub shutdown: Option<OnShutdown>,
}

/// A chat command a mod registers.
#[repr(C)]
pub struct CommandSpec {
    /// `size_of::<CommandSpec>()` of the mod.
    pub size: u32,
    /// [`ROLE_PLAYER`] or [`ROLE_ADMIN`].
    pub role: u32,
    /// `[a-z][a-z0-9_-]{0,31}`, without the `!`.
    pub name: Str,
    /// One line for the command list.
    pub help: Str,
}

/// A player, as seen in a snapshot.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PlayerInfo {
    /// `size_of::<PlayerInfo>()` of the server.
    pub size: u32,
    /// 1 when the account has the admin role.
    pub is_admin: u32,
    /// The player's id for this session: pass it as [`Command::target`]. It is not the account
    /// id and not stable across logins.
    pub id: u64,
    /// The entity id in the instance (0 when not in one).
    pub entity: u64,
    pub account: Str,
    pub character: Str,
    /// Position in grid cells of the instance (0, 0 when not in one).
    pub x: f32,
    pub y: f32,
}

/// A read-only copy of an instance at one moment.
#[repr(C)]
pub struct Snapshot {
    /// `size_of::<Snapshot>()` of the server.
    pub size: u32,
    pub reserved: u32,
    /// The instance's frame clock (not a wall clock).
    pub frame: u64,
    pub instance_id: u64,
    /// The area id the instance runs, e.g. `1_1_1`.
    pub area: Str,
    pub players: *const PlayerInfo,
    pub player_count: usize,
}

impl Snapshot {
    /// # Safety
    /// Valid only inside the callback that received the snapshot.
    pub unsafe fn players<'a>(&self) -> &'a [PlayerInfo] {
        if self.player_count == 0 || self.players.is_null() {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(self.players, self.player_count) }
    }
}

/// One event, valid for the call.
#[repr(C)]
pub struct Event {
    /// `size_of::<Event>()` of the server.
    pub size: u32,
    /// `EVENT_*`. A mod ignores kinds it does not know.
    pub kind: u32,
    /// The instance frame (0 for server scope events).
    pub frame: u64,
    /// The instance concerned (0 for server scope events).
    pub instance_id: u64,
    /// Pass to [`ServerApi::submit`] while this callback runs; meaningless afterwards.
    pub scope: *const c_void,
    /// The instance state (null for server scope events and for the area events before the
    /// instance exists).
    pub snapshot: *const Snapshot,
    /// The player concerned (null when none).
    pub player: *const PlayerInfo,
    /// Kind-dependent text: the command name, the area id.
    pub text: Str,
    /// Kind-dependent text: the command's arguments.
    pub args: Str,
}

/// A change the mod asks for. One flat struct for every kind; unused fields are 0.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Command {
    /// `size_of::<Command>()` of the mod.
    pub size: u32,
    /// `COMMAND_*`.
    pub kind: u32,
    /// A [`PlayerInfo::id`].
    pub target: u64,
    pub x: f64,
    pub y: f64,
    pub text: Str,
}

impl Command {
    pub fn new(kind: u32) -> Self {
        Self {
            size: size_of::<Command>() as u32,
            kind,
            target: 0,
            x: 0.0,
            y: 0.0,
            text: Str::EMPTY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    /// The layout is the ABI: when this fails, bump [`SERVER_API_VERSION`] (and `docs/abi-stability.md`,
    /// the examples' `server_api`) unless the change only appends fields.
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn layouts_are_pinned() {
        assert_eq!(SERVER_API_VERSION, 1);
        assert_eq!(size_of::<Str>(), 16);

        assert_eq!(size_of::<ServerApi>(), 128);
        assert_eq!(offset_of!(ServerApi, ctx), 8);
        assert_eq!(offset_of!(ServerApi, mod_id), 16);
        assert_eq!(offset_of!(ServerApi, game_version), 80);
        assert_eq!(offset_of!(ServerApi, log), 96);
        assert_eq!(offset_of!(ServerApi, config_get), 104);
        assert_eq!(offset_of!(ServerApi, register_command), 112);
        assert_eq!(offset_of!(ServerApi, submit), 120);

        assert_eq!(size_of::<ModInfo>(), 40);
        assert_eq!(offset_of!(ModInfo, event_mask), 8);
        assert_eq!(offset_of!(ModInfo, user), 16);
        assert_eq!(offset_of!(ModInfo, on_event), 24);
        assert_eq!(offset_of!(ModInfo, shutdown), 32);

        assert_eq!(size_of::<CommandSpec>(), 40);
        assert_eq!(offset_of!(CommandSpec, name), 8);

        assert_eq!(size_of::<PlayerInfo>(), 64);
        assert_eq!(offset_of!(PlayerInfo, id), 8);
        assert_eq!(offset_of!(PlayerInfo, account), 24);
        assert_eq!(offset_of!(PlayerInfo, character), 40);
        assert_eq!(offset_of!(PlayerInfo, x), 56);

        assert_eq!(size_of::<Snapshot>(), 56);
        assert_eq!(offset_of!(Snapshot, area), 24);
        assert_eq!(offset_of!(Snapshot, players), 40);
        assert_eq!(offset_of!(Snapshot, player_count), 48);

        assert_eq!(size_of::<Event>(), 80);
        assert_eq!(offset_of!(Event, scope), 24);
        assert_eq!(offset_of!(Event, snapshot), 32);
        assert_eq!(offset_of!(Event, player), 40);
        assert_eq!(offset_of!(Event, text), 48);
        assert_eq!(offset_of!(Event, args), 64);

        assert_eq!(size_of::<Command>(), 48);
        assert_eq!(offset_of!(Command, target), 8);
        assert_eq!(offset_of!(Command, x), 16);
        assert_eq!(offset_of!(Command, text), 32);
    }

    #[test]
    fn export_names_are_nul_terminated() {
        assert!(MOD_INIT.ends_with(&[0]) && MOD_ABI_VERSION.ends_with(&[0]));
        assert_eq!(MOD_INIT, b"poe_server_mod_init\0");
    }

    #[test]
    fn event_kinds_are_distinct_bits_inside_the_all_mask() {
        let kinds = [
            EVENT_SERVER_START,
            EVENT_SERVER_STOP,
            EVENT_PLAYER_JOIN,
            EVENT_PLAYER_LEAVE,
            EVENT_COMMAND,
            EVENT_INSTANCE_CREATED,
            EVENT_INSTANCE_CLOSED,
            EVENT_TICK,
        ];
        let mut seen = 0u64;
        for k in kinds {
            let bit = event_bit(k);
            assert_eq!(seen & bit, 0, "kind {k} repeats");
            assert_ne!(EVENTS_ALL & bit, 0);
            seen |= bit;
        }
        assert_eq!(seen, EVENTS_ALL);
    }

    #[test]
    fn str_round_trips_and_empty_is_safe() {
        assert_eq!(unsafe { Str::new("0.5.5e").as_str() }, "0.5.5e");
        assert_eq!(unsafe { Str::EMPTY.as_str() }, "");
    }

    #[test]
    fn a_new_command_carries_its_size() {
        let c = Command::new(COMMAND_BROADCAST);
        assert_eq!(c.size as usize, size_of::<Command>());
        assert_eq!(c.kind, COMMAND_BROADCAST);
    }
}
