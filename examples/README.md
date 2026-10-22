# Examples

| Example | What it shows |
|---|---|
| [hello-client](hello-client) | A client mod: a DLL the client hook loads; logs a hello line. Manifest v1. |
| [hello-server](hello-server) | A native server mod: adds the `!hello` chat command. Manifest v2, `side = "server"`. |
| [hello-both](hello-both) | One package with a client DLL (`client/`) and a server library (`server/`). Manifest v2, `side = "both"`. |

They are exactly what `poe-mod new` writes (a test, `examples_match_their_templates`, keeps them in sync) and are built by the workspace, so they always compile against the current SDK.
To regenerate after changing a template:

```sh
A="WaystoneWorks and contributors"
cargo run -p poe-mod-cli -- new client hello-client --dir examples/hello-client --sdk-path ../.. --author "$A" --force
cargo run -p poe-mod-cli -- new server hello-server --dir examples/hello-server --sdk-path ../.. --author "$A" --force
cargo run -p poe-mod-cli -- new both hello-both --dir examples/hello-both --sdk-path ../../.. --author "$A" --force
```
