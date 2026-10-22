# Writing a client mod

A client mod is a DLL for `x86_64-pc-windows-msvc` that the client hook's loader (`version.dll`, Poe2ClientHook) maps into the game process.
The ABI is `HookApi` of `poe-hook-mod-api`, re-exported as `poe_mod_sdk::client`; the authoritative description (what the loader checks, the exports,
the log lines) is the [manifest and ABI spec](https://github.com/WaystoneWorks/Poe2ClientHook/blob/main/docs/mod-manifest.md).

```sh
poe-mod new client my-mod && cd my-mod
cargo xwin build --release --target x86_64-pc-windows-msvc     # Linux; plain `cargo build --release` on Windows
poe-mod pack . --artifacts target/x86_64-pc-windows-msvc/release
```

```rust
use poe_mod_sdk::client::HookApi;

fn init(api: &HookApi) -> Result<(), String> {
    api.info(&format!("hello from {}: game build {}", api.id(), api.game_version()));
    Ok(())
}

poe_mod_sdk::declare_client_mod!(init);
```

`declare_client_mod!` exports `poe_mod_abi_version` and `poe_mod_init`, checks the loader's ABI version, and turns an `Err` or a panic into a logged
refusal (the loader unloads the DLL and goes on). The manifest must state `hook_api` equal to `poe_mod_sdk::client::HOOK_API_VERSION`.

Rules of the road:

* `api.read_only()` is the read-only latch: true in official mode (no mod loads there at all) or once the client talked to an untrusted server. A mod
  that sends, alters or injects anything must stop while it is true.
* `api.sig_addr("name")` gives the runtime address of a signature of the loaded table; hooking and patching is your job and your responsibility.
* A client-only mod written as `manifest_version = 1` loads with every launcher and hook; the template does exactly that. Use manifest v2 only when you
  need `side = "both"` or the `[client]` section.
* A mod that is part of a `both` package: see [packaging.md](packaging.md) and `poe-mod new both`.
