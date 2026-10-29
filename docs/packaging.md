# Packages

A distributable mod is **one zip plus a checksum file**:

```
my-mod-0.1.0.zip           the package
my-mod-0.1.0.zip.sha256    <64 hex digits>  my-mod-0.1.0.zip       (sha256sum format)
```

The SHA-256 of the whole zip is the package's identity: a server advertises it, a launcher pins it, `poe-mod verify` checks it.

## Layout

```
mod.toml                       at the root, not inside a folder
client/my_mod.dll              the client half (Windows DLL)
server/libmy_mod.so            the server half, one library per platform
server/my_mod.dll
data/...                       data overlays, if the manifest names them
README.md, LICENSE, ...        optional documents at the root
```

The folder names are a convention; what counts is the paths in `mod.toml` (`client.dll` / `entry.dll`, `server.library.linux`, `server.library.windows`,
`tables`, `opcodes`, `config_defaults`). The package contains `mod.toml`, exactly the files and folders it names, and root documents (README, LICENSE, NOTICE,
CHANGELOG, COPYING). Sources and build output next to the manifest never travel.

## poe-mod pack

```sh
poe-mod pack <mod folder> [--out <dir>] [--artifacts <dir>]... [--add <path-in-package>=<file>]...
```

* Files the manifest names are taken from the mod folder; those that are not there (compiled libraries) are looked up **by file name** in each
  `--artifacts` folder (a cargo `target/.../release`), or supplied one by one with `--add`.
* The result is verified before the command succeeds; on any error nothing is left on disk.
* **Reproducible**: entries are sorted by path, with one fixed timestamp (1980-01-01), fixed permissions (0644) and Deflate at a fixed level, so the same
  inputs give the same bytes (same tool version) and the same hash. CI packs every example twice and compares.

## poe-mod verify

Runs `poe_hook_mods::verify_package`, the same check the launcher and the server use. It refuses:

* a missing or wrong `.sha256` (and a sidecar that names another file);
* a file name other than `<id>-<version>.zip` of the manifest;
* no valid `mod.toml` at the root; a file the manifest names that is not in the zip (libraries and `config_defaults` must be files);
* absolute paths, `..`, empty path parts, backslashes, drive colons, control characters, symlinks, encrypted entries, duplicate paths (compared
  case-insensitively);
* archives over the limits (512 MiB package, 4096 files, 1 GiB per file, 2 GiB unpacked, 240-byte paths) and entries whose unpacked size or CRC does
  not match the zip's own record.

Warnings (not refusals): unknown manifest keys, no `game.versions`, a server library for only one platform.

## Versions

* `version` is semver. A server pins **exact version and hash**; two versions of a mod can sit side by side in a launcher's cache.
* `game.versions` lists the client builds the mod was made for (e.g. `0.5.5e.hotfix1`).
* The ABI numbers (`hook_api`, `server_api`) must equal what the host offers; see [abi-stability.md](abi-stability.md).
* `manifest_version = 2` is required for `side = "server"` and `"both"`. Programs that read only v1 refuse such a manifest on purpose, so a server-only
  mod can never be mistaken for a client mod. A client-only mod can stay at v1 to keep working with older launchers.

## Release flow (suggested)

1. Build the libraries on each platform (CI matrix).
2. Collect them in one folder per platform and run `poe-mod pack ... --artifacts <linux dir> --artifacts <windows dir>` once.
3. `poe-mod verify`, then publish the zip and the `.sha256` (GitHub release assets work), and tell servers the version and hash.
