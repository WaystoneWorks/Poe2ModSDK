# Hello server

A native server mod (a Rust `cdylib` loaded by the server). It adds `!hello`.

```sh
cargo build --release                       # target/release/libhello_server.so on Linux
cargo build --release --target x86_64-pc-windows-msvc   # hello_server.dll for a Windows server
# Pack: takes the libraries from the build folders. Both platforms in one package is best;
# a package with only one library works on that platform only.
poe-mod pack . --artifacts target/release --artifacts target/x86_64-pc-windows-msvc/release
poe-mod verify hello-server-0.1.0.zip
```

Install on the server: put the zip's contents into `mods/hello-server/` (or use the server's mod
command) and enable the mod; changing server mods needs a restart.
Docs: https://github.com/WaystoneWorks/Poe2ModSDK
