# Contributing

Thanks for helping. This is a small hobby project for personal and educational use; keep changes focused.

## Before you open a pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace && cargo test --workspace
```

CI runs the same on Linux and Windows.

## Rules that matter

* **The ABI is a contract.** `poe-mod-api` is a C ABI that compiled mods depend on. Read [docs/abi-stability.md](docs/abi-stability.md)
  first: appending a field or an event kind is fine, changing or removing anything bumps `SERVER_API_VERSION`. The layout test in
  `crates/poe-mod-api/src/lib.rs` fails when the layout changes; do not "fix" it without deciding on the bump.
* **Templates and examples stay in sync.** The examples are what `poe-mod new --sdk-path` writes; `examples_match_their_templates`
  fails when a template changed. Regenerate with the commands in [examples/README.md](examples/README.md).
* **The manifest and the package check are not here.** They live in `poe-hook-mods` in
  [Poe2ClientHook](https://github.com/WaystoneWorks/Poe2ClientHook) (one parser for the loader, the launcher, the server and this
  tool). Change the format there, then bump the pinned `rev` in `Cargo.toml`.
* **No official servers.** Nothing in this project is used against, or tested on, official servers. Do not add code that is.
* New behaviour needs a test; a bug fix needs a test that fails without it.
* Commit messages: a short imperative summary line, then what and why.

## Reporting problems

Bugs and ideas: a GitHub issue with the SDK version/commit, your OS and what you expected. Security problems: see [SECURITY.md](SECURITY.md).

## License

By contributing you agree that your contribution is licensed under the MIT license of this repository.
