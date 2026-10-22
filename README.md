# Poe2ModSDK

The mod SDK of the [Poe2ModdingProject](https://github.com/WaystoneWorks/Poe2ModdingProject), a private server for
**Path of Exile 2 client 0.5.5e** (unofficial, personal and educational use; never used against official servers).
It has everything to write, pack and verify a mod:

| Part | What it is |
|---|---|
| [`poe-mod-api`](crates/poe-mod-api) | The **server mod C ABI** (`ServerApi`, `ModInfo`, events, commands). No dependencies, `repr(C)`, versioned. |
| [`poe-mod-sdk`](crates/poe-mod-sdk) | Safe Rust wrappers and macros: `ServerMod` + `declare_server_mod!` for server mods, `declare_client_mod!` and the client ABI (re-exported) for client mods. |
| [`poe-mod-cli`](crates/poe-mod-cli) | The `poe-mod` tool: `new` (from a template), `check`, `pack` (reproducible `<id>-<version>.zip` + `.sha256`), `verify`. |
| [`examples/`](examples) | `hello-client`, `hello-server` (adds `!hello`) and `hello-both` (one package, two halves). Built and tested in CI. |
| [`docs/`](docs) | ABI stability rules, writing server mods, writing client mods, the package format. |

## What a mod is

One package per mod, with a `mod.toml` that says **where it runs**: `side = "client"`, `"server"` or `"both"`.

* A **client mod** is a DLL the client hook (`Poe2ClientHook`) loads into the game process.
* A **server mod** is a native Rust library (`.so` on Linux, `.dll` on Windows) the server loads, or just data (chat commands, game-data overlays).
* A **both** mod carries the two halves in one package; a server can require its players to have it.

The manifest format (v1 and v2) is specified in the hook repository:
[docs/mod-manifest.md](https://github.com/WaystoneWorks/Poe2ClientHook/blob/main/docs/mod-manifest.md).

> **Status.** This is the SDK foundation. You can already write, build, pack and verify client mods (the hook loads them today),
> and write, build, pack and verify server mods against the ABI. The **server's loader** for server mods, the server
> advertising required mods and the launcher's mod sync come in the next work packages of the main project; until the loader
> lands, a server mod package is valid and loadable by tests (this repository loads the example the way the server will), but a
> running server does not load it yet.

## Quick start

```sh
cargo install --git https://github.com/WaystoneWorks/Poe2ModSDK poe-mod-cli      # installs `poe-mod`

poe-mod new server my-mod --author "You"        # or: client, both
cd my-mod
cargo build --release                           # target/release/libmy_mod.so
poe-mod check .
poe-mod pack . --artifacts target/release       # -> my-mod-0.1.0.zip + .sha256 (then verified)
poe-mod verify my-mod-0.1.0.zip
```

A server mod is this much code (see [`examples/hello-server`](examples/hello-server)):

```rust
use poe_mod_sdk::server::*;

struct Hello;

impl ServerMod for Hello {
    fn init(host: &Host) -> Result<Self, String> {
        host.register_command("hello", "Says hello.", Role::Player).map_err(|e| e.to_string())?;
        Ok(Hello)
    }

    fn event(&self, _host: &Host, event: &Event<'_>, out: &mut Out<'_>) -> Flow {
        if let Event::Command { player, .. } = event {
            let _ = out.send_chat(player.id(), &format!("Hello, {}!", player.character()));
            return Flow::Consumed;
        }
        Flow::Continue
    }
}

poe_mod_sdk::declare_server_mod!(Hello);
```

The mod never touches the world directly: events carry read-only snapshots, and the mod answers with commands (`send_chat`,
`teleport`, ...) that the server applies on the task that owns the instance. Details: [docs/writing-server-mods.md](docs/writing-server-mods.md).

## Why git dependencies

The client half of the SDK is the client hook's own `poe-hook-mod-api` (the `HookApi` ABI) and `poe-hook-mods` (the manifest
parser and the package verifier). They are **git dependencies pinned to a commit** of
[Poe2ClientHook](https://github.com/WaystoneWorks/Poe2ClientHook) (see `[workspace.dependencies]` in `Cargo.toml`) rather than
a vendored copy: the ABI crate and the parser have exactly one source of truth, so the SDK can never drift from the loader that
reads the mods, and the layout tests that pin the ABI travel with the commit. Updating the pin is a one-line change plus CI.
The SDK's own ABI crate, `poe-mod-api`, is the server's counterpart and lives here.
Projects made by `poe-mod new` depend on `poe-mod-sdk` by git tag (`v<version>` of this repository).

## Build and test

```sh
cargo build --workspace          # builds the example libraries too
cargo test --workspace           # unit tests, the template/example sync check, loads libhello_server like the server does
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

CI runs this on Linux and Windows, smoke-tests every template (what `poe-mod new` writes must build), and packs and verifies the
examples with both platforms' libraries, checking that packing twice gives identical bytes.

## Safety

A mod is native code. A client mod runs inside the game process, a server mod inside the server process; neither the SDK nor the
package check can make that safe. Install only mods you trust. The package check (hash, paths, symlinks, limits) protects
against a corrupt or malicious *archive*, not a malicious *library*. The client hook loads no mod in official mode.

## License

MIT, Copyright (c) 2026 WaystoneWorks and contributors. See [LICENSE](LICENSE), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md).
