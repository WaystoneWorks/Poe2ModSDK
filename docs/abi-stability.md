# ABI stability rules

There are two ABIs, independent of each other and versioned separately:

| ABI | Crate | Version constant | Manifest key | Direction |
|---|---|---|---|---|
| Client mod | `poe-hook-mod-api` (Poe2ClientHook) | `HOOK_API_VERSION` | `client.hook_api` / `game.hook_api` | the client hook offers `HookApi` to a mod DLL |
| Server mod | `poe-mod-api` (this repository) | `SERVER_API_VERSION` | `server.server_api` | the server offers `ServerApi` to a mod library |

A mod is compiled against one version of its ABI and states that number in `mod.toml`. The host offers **exactly one** version: a mod built for
a newer one is refused ("update the server"), one built for an older one is refused ("rebuild the mod"). The manifest is checked before the
library is mapped; the library's `poe_server_mod_abi_version` / `poe_mod_abi_version` export is checked right after mapping, before any other
call into the mod.

## What crosses the boundary

Only `#[repr(C)]` data and `extern "C"` functions. The host and every mod are separate binaries with their own Rust runtime and allocator,
so **nothing is allocated on one side and freed on the other**, no Rust types (`String`, `Vec`, `Box`, `Option<&T>`, trait objects) are
passed, and a panic must never unwind across (the SDK catches it). Strings are borrowed UTF-8 (`Str`: pointer and length) valid for the call
only; the strings inside `ServerApi` live for the process. Data a callback receives (events, snapshots, players) is valid until the callback
returns; copy what you keep.

## Changing the ABI

**Allowed without a version bump (additive):**

* Appending a field at the **end** of `ServerApi`, `ModInfo`, `Event`, `Snapshot`, `PlayerInfo`, `Command`, `CommandSpec`. Every struct starts with its
  `size` (or `ServerApi::size`); a reader uses a field only when `size` covers it. The side that fills the struct sets `size` to its own
  `size_of`; the side that reads it uses `min(size, its own size_of)`. A mod must not write `ModInfo` fields past the `size` the server gave.
* A new event kind, command kind, role or error code. Mods ignore event kinds they do not know (`Event::Unknown` in the SDK); the server
  ignores command kinds it does not know (logged, dropped) and only sends events a mod subscribed to.

**Needs a bump of `SERVER_API_VERSION` (and a CHANGELOG line, and a rebuild of every mod):**

* Changing the type, meaning, order or position of an existing field, function signature or constant.
* Removing anything. Reusing a retired number for something else.
* Changing when or on which thread a callback runs, what a return value means, or any other rule of the contract documented in `poe-mod-api`.

The layout test (`layouts_are_pinned` in `crates/poe-mod-api/src/lib.rs`) pins every size and offset. When it fails you are changing the ABI:
decide whether it is additive (update the test with the new fields appended) or breaking (bump the version, then update it).

## How the host and the SDK keep it safe

* `poe_server_mod_init` checks `api.abi_version`, `api.size >= size_of::<ServerApi>()` and `info.size >= size_of::<ModInfo>()` before it
  touches anything, and returns an error code otherwise.
* Every callback is wrapped in `catch_unwind`; a panic is logged and the call counts as "continue". The host may disable a mod that keeps
  failing or overruns its time budget.
* Host functions validate their arguments (null, not UTF-8, unknown player) and answer with an error code; they never trust the mod.
* A mod is never unloaded and never hot-reloaded while the server runs; changing server mods needs a restart.

## Client ABI

The same rules apply to `HookApi`; its authoritative description is [docs/mod-manifest.md](https://github.com/WaystoneWorks/Poe2ClientHook/blob/main/docs/mod-manifest.md#the-mod-abi)
in the hook repository. The SDK re-exports the crate at a pinned commit; the pin moves when `HOOK_API_VERSION` or `HookApi` change.
