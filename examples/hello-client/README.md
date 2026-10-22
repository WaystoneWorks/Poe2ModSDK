# Hello client

A client mod (a DLL that runs inside the game process through the poe-hook loader).

```sh
# Build the DLL for Windows (the game): on Linux with cargo-xwin, on Windows with plain cargo.
cargo xwin build --release --target x86_64-pc-windows-msvc     # or: cargo build --release
# Pack it: reads mod.toml, takes the DLL from the build folder, writes hello-client-0.1.0.zip(.sha256).
poe-mod pack . --artifacts target/x86_64-pc-windows-msvc/release
poe-mod verify hello-client-0.1.0.zip
```

Install the zip with the launcher's Mods page. Docs: https://github.com/WaystoneWorks/Poe2ModSDK
