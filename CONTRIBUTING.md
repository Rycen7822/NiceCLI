# Contributing

Thanks for contributing to NiceCLI.

## Project Shape

- NiceCLI is a local-only Windows desktop application.
- The active codebase is this repository root: `apps/nicecli` plus `crates/*`.
- The default runtime path is the in-process Rust backend hosted by Tauri.
- Portable executable delivery is the maintained release target. Installer bundles are out of scope.

## Before You Change Code

1. Keep changes small and reviewable.
2. Prefer clear module boundaries over clever abstractions.
3. Do not reintroduce Go code, remote runtime paths, or installer-specific flows.
4. Do not rewrite frontend pages unless the task explicitly requires UI work.
5. If you touch route behavior, keep Rust contract lists, contract fixtures, and structure guards in sync.

## Local Setup

- Install Rust `1.94.1` with `rustfmt` and `clippy` from `rust-toolchain.toml`.
- Install Node.js `18+`.
- Install Microsoft Edge WebView2 Runtime on Windows.

## Verification

Run the maintained verification set from the repository root when your change affects the core runtime:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p nicecli-runtime
cargo test -p nicecli-quota
cargo test -p nicecli-backend
cargo test -p nicecli-contract-tests
```

If you touch the Tauri host, also run:

```powershell
cargo test --manifest-path .\apps\nicecli\src-tauri\Cargo.toml
cargo clippy --manifest-path .\apps\nicecli\src-tauri\Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path .\apps\nicecli\src-tauri\Cargo.toml -- --smoke-backend-host
```

If you touch tray, window lifecycle, startup, or shutdown behavior, also run:

```powershell
cargo run --manifest-path .\apps\nicecli\src-tauri\Cargo.toml -- --smoke-tray-host
```

You can also use the host-side shortcuts from `apps/nicecli`:

```powershell
npm run lint:host
npm run test
npm run verify:host
npm run verify:host:tray
```

## Documentation Expectations

- Update `README.md` or `README_CN.md` when contributor-facing behavior changes.
- Update `docs/maintenance.md` when the maintained verification flow, CI gate, or release process changes.
- Update `docs/architecture.md` when module boundaries or runtime shape change.
- Update `CHANGELOG.md` when a maintenance-facing repository change should be preserved.

## Generated Files

Do not commit local build or install artifacts such as:

- `target/`
- `apps/nicecli/node_modules/`
- `apps/nicecli/dist-web/`
- `apps/nicecli/src-tauri/target/`
- `apps/nicecli/src-tauri/bundled/`

## Pull Request Notes

- Describe the user-visible impact.
- List the verification commands you ran.
- Call out compatibility-sensitive areas such as auth, quota, config persistence, route contracts, tray lifecycle, or host startup.
