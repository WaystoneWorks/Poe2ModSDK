# Writing a server mod

A server mod is a Rust `cdylib` that the server loads. This page describes server API 1; the contract in full is the documentation
of [`poe-mod-api`](../crates/poe-mod-api/src/lib.rs), the stability rules are in [abi-stability.md](abi-stability.md).

> The server's loader is a later work package of the main project. Everything here is the contract it will implement; the SDK
> test `the_example_server_mod_loads_registers_hello_and_answers_it` drives a real built library through the same exports with a fake host.

## Start

```sh
poe-mod new server my-mod && cd my-mod
```

You get `Cargo.toml` (a `cdylib` depending on `poe-mod-sdk`), `mod.toml` (`side = "server"`, `[server] server_api`, `[server.library]` per platform),
`src/lib.rs` and a README with the build and pack commands.

## The shape of a mod

```rust
use poe_mod_sdk::server::*;

struct MyMod { /* state: must be Send + Sync */ }

impl ServerMod for MyMod {
    const EVENTS: Events = Events::PLAYER_JOIN | Events::INSTANCE_CREATED;   // what event() wants

    fn init(host: &Host) -> Result<Self, String> { /* register commands, read config */ }
    fn event(&self, host: &Host, event: &Event<'_>, out: &mut Out<'_>) -> Flow { Flow::Continue }
    fn shutdown(&self, host: &Host) {}
}

poe_mod_sdk::declare_server_mod!(MyMod);
```

`declare_server_mod!` exports `poe_server_mod_abi_version` and `poe_server_mod_init`. The library must be a `cdylib`
(`[lib] crate-type = ["cdylib"]`); the macro needs `unsafe` attributes, so do not `#![forbid(unsafe_code)]` in the crate.

## Host functions (`Host`)

| | |
|---|---|
| `id()`, `mod_dir()`, `server_dir()`, `server_version()`, `game_version()` | where you are |
| `log` / `info` / `warn` / `error` / `debug` | records tagged `mod:<id>` in the server log |
| `config(key)` | a value of your section `[mods.<id>]` of the server's config, as text (`None` when unset) |
| `register_command(name, help, role)` | a chat command `!name` (`Role::Player` or `Role::Admin`). **Only in `init`.** Names are `[a-z][a-z0-9_-]{0,31}`; a name the server or another mod has is `ERR_EXISTS`. |

## Events

Subscribe with `EVENTS` (a mask); `!commands` you registered are always delivered.

| `Event` | When | Scope |
|---|---|---|
| `ServerStart` / `ServerStop` | after data and config load / before shutdown | server |
| `PlayerJoin { player, snapshot }` / `PlayerLeave { .. }` | a player entered / left an instance (login, logout, area transfer) | instance |
| `Command { player, name, args }` | a player ran a command you registered (role already checked) | caller's session |
| `InstanceCreated { instance_id, area }` / `InstanceClosed { .. }` | an instance came and went | instance |
| `Tick { snapshot }` | every frame of every instance, after the simulation step | instance |
| `Unknown { kind }` | a kind newer than this SDK: ignore it | |

Return `Flow::Continue` or `Flow::Consumed` (for the notification events of API 1 it makes no difference; it is part of the contract so later
cancellable events — chat, movement — need no new function).

## The world: snapshots in, commands out

You never get a reference into the server. `snapshot` is a read-only copy at that moment (frame, instance id, area id, the players with id,
account, character, position in grid cells), valid for the call. To change anything, queue a command on `out`:

| `Out` method | Effect |
|---|---|
| `send_chat(player_id, text)` | a server message to one player |
| `broadcast(text)` | a server message to everyone |
| `teleport(player_id, x, y)` | move a player to a cell of their instance |
| `kick(player_id, reason)` | disconnect a player |

`player.id()` is the id for this session (not the account id). Commands are validated by the server and applied on the task that owns the
instance **after your callback returns**; an invalid one is logged and dropped. `Err(ApiError)` from a method means the host refused to queue it.

## Threads and time

* Callbacks for **different instances run concurrently** on different threads; callbacks of one instance never overlap. Your state must be
  `Send + Sync` (the trait requires it): use atomics, `Mutex`/`RwLock`, or keep per-instance data in a map keyed by `instance_id`.
* A callback must return fast. A tick has a budget of milliseconds; the server may disable a mod that overruns. Subscribe to `TICK` only when
  you need it, and do nothing slow (no file or network I/O, no sleeping) in a callback.
* The server's frame clock (`snapshot.frame()`) is not a wall clock; replay tests of the server rely on determinism, so do not make the
  *simulation* depend on time or randomness you introduce.

## Errors and panics

`init` returning `Err` (or panicking) is logged and the server goes on without your mod. A panic in `event` is caught and logged. Do not rely on
it: a mod that keeps failing is disabled.

## Build, pack, install

```sh
cargo build --release                         # libmy_mod.so (Linux); add --target x86_64-pc-windows-msvc for my_mod.dll
poe-mod pack . --artifacts target/release --artifacts target/x86_64-pc-windows-msvc/release
poe-mod verify my-mod-0.1.0.zip
```

Ship both platforms in one package when your players or hosts use both; a package with one library works on that platform only (the verifier
warns). See [packaging.md](packaging.md).

## Data-only server mods

Not everything needs code. A `side = "server"` mod with no `[server.library]` can ship `[[commands]]` (a fixed reply or an alias to another command),
game-data overlays (`[server] tables`) and config defaults (`[server] config_defaults`). That is the manifest's data tier; see the
[manifest spec](https://github.com/WaystoneWorks/Poe2ClientHook/blob/main/docs/mod-manifest.md#version-2-sides).
