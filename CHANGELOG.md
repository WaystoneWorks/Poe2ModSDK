# Changelog

## 0.1.0 (unreleased)

* `poe-mod-api`: server mod ABI, `SERVER_API_VERSION = 1` (`ServerApi`, `ModInfo`, `Event`, `Snapshot`, `PlayerInfo`, `Command`, `CommandSpec`; events: server start/stop,
  player join/leave, command, instance created/closed, tick; commands: send chat, broadcast, teleport, kick).
* `poe-mod-sdk`: `ServerMod`, `declare_server_mod!`; `declare_client_mod!` and the re-exported client ABI (feature `client`).
* `poe-mod-cli` (`poe-mod`): `new`, `check`, `pack` (reproducible), `verify`.
* Examples `hello-client`, `hello-server`, `hello-both`; templates; CI on Linux and Windows.
