//! # poe-mod-sdk
//!
//! Safe wrappers and macros for the mods of the PoE2 private-server project.
//!
//! * **Server mods** (feature `server`, on by default): implement [`server::ServerMod`], call
//!   [`declare_server_mod!`], build a `cdylib`. The raw C ABI is [`server::api`]
//!   (`poe-mod-api`).
//! * **Client mods** (feature `client`): [`declare_client_mod!`] exports the entry points the
//!   client hook's loader expects; the raw ABI (`HookApi`) is [`client::api`], re-exported from
//!   the hook's own `poe-hook-mod-api` crate at a pinned commit.
//!
//! A mod that has both halves is two crates (a client DLL for Windows, a server library per
//! platform) packed into one package with `poe-mod pack`. Guides: `docs/` in the repository.

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;
