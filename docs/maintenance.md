# Maintenance

## Current Baseline

- `apps/nicecli` plus `crates/*` are the active source of truth for NiceCLI.
- NiceCLI stays `local-only` and ships as a single portable executable.
- The main desktop runtime path is the in-process Rust backend hosted by Tauri.
- The repository build path is Rust plus Node only.
- `docs/*` is reserved for current maintenance documents, not historical notes.

## Repository Governance

- Contributor workflow: `CONTRIBUTING.md`
- Conduct policy: `CODE_OF_CONDUCT.md`
- Security reporting: `SECURITY.md`
- Support path: `SUPPORT.md`
- Maintenance-facing change record: `CHANGELOG.md`
- Root editing policy: `.editorconfig`
- Line-ending and binary policy: `.gitattributes`

## Repository Hygiene

Generated directories stay local and should not be committed:

- `target/`
- `apps/nicecli/node_modules/`
- `apps/nicecli/dist-web/`
- `apps/nicecli/src-tauri/target/`
- `apps/nicecli/src-tauri/bundled/`

Tracked screenshots and logos at the repository root are documentation assets, not disposable build output.

## Maintenance Boundaries

- Pure Rust migration is already complete.
- Current engineering work is limited to small cleanup or hardening slices.
- Do not force structural splits just for appearance.
- Only reopen a paused refactor when a later task exposes a clearly isolated hotspot.
- Treat auth, quota, config persistence, route contracts, tray lifecycle, and host startup as high-risk change areas.

## Build And Verification

Root entrypoints:

- `build-windows.ps1`
- `scripts/build-windows.ps1`

Maintained verification from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\build-windows.ps1
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p nicecli-runtime
cargo test -p nicecli-quota
cargo test -p nicecli-backend
cargo test -p nicecli-contract-tests
cargo test --manifest-path .\apps\nicecli\src-tauri\Cargo.toml
cargo clippy --manifest-path .\apps\nicecli\src-tauri\Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path .\apps\nicecli\src-tauri\Cargo.toml -- --smoke-backend-host
```

Maintained host-side shortcuts from `apps/nicecli`:

```powershell
npm run verify:js
npm run lint:host
npm run test
npm run verify:host
npm run verify:host:tray
```

`npm run verify:js` is the frontend guardrail entrypoint. It does not rewrite page code; it only checks browser-side JS syntax, validates hard-coded management/public route references against `crates/nicecli-backend/src/contract.rs`, and verifies `window.__TAURI__.core.invoke(...)` calls against the registered Tauri command list.

Use tray smoke whenever a change touches tray, window lifecycle, startup, or host shutdown behavior.

## CI

- Workflow source of truth: `.github/workflows/windows-rust-ci.yml`
- The source tree already contains the workflow.
- Remote CI closure is still pending explicit authorization to sync the workflow into `D:\dev\wincli\NiceCLI`.

Current workflow coverage:

- Rust formatting
- workspace clippy
- Tauri host clippy
- root Windows build entrypoint
- runtime, quota, backend, and contract tests
- Tauri host tests
- backend host smoke
- tray host smoke

## Release Workflow

- Run `build-windows.ps1` from the repository root.
- Re-run the maintained verification set before release.
- Use tray smoke for releases that touch tray, startup, or window lifecycle behavior.
- Canonical desktop output: `apps/nicecli/src-tauri/target/release/nicecli.exe`
- Rename or copy the final portable artifact to `nicecli_windows_amd64.exe` only during release packaging.
- Publish the portable executable only. Do not publish setup or installer bundles.

## Known Caveat

- `--smoke-tray-host` can print the Windows WebView cleanup log `Chrome_WidgetWin_0 ... Error = 1412`.
- Treat that log as non-blocking if the command exits successfully and the smoke result stays successful.

## Toolchain Baseline

- Root toolchain file: `rust-toolchain.toml`
- Pinned Rust toolchain: `1.94.1` with `rustfmt` and `clippy`
- CI Node lane: `22`
- Maintained app-side scripts live in `apps/nicecli/package.json`

## Dependency Audit Policy

- Lightweight maintained dependency shape check:

```powershell
cargo metadata --manifest-path .\Cargo.toml
```

- Root `deny.toml` is now the repository policy file for cargo-deny.
- Current cargo-deny scope is intentionally limited to `advisories`, `bans`, and `sources`.
- License gating is deferred for now; it should only be enabled after the repository confirms an explicit SPDX allowlist for the full transitive graph.
- If `cargo-deny` is installed locally, use:

```powershell
cargo deny check advisories bans sources
```

- `cargo-deny` is not yet part of the maintained mandatory baseline because the tool is not installed in every local environment and the publish repository CI workflow is still waiting for sync authorization.

## Upgrade Guidance

1. Re-run the maintained verification set on the current baseline first.
2. Upgrade Rust and host-side dependencies in small batches.
3. Re-run host tests and backend smoke after Rust or Tauri host changes.
4. Re-run tray smoke after tray, startup, window, or shutdown changes.
5. Update docs when the maintained baseline, workflow, or release process changes.
6. Update `CHANGELOG.md` when a maintenance-facing change should be preserved for future contributors.

## Rollback Guidance

- If an upgrade breaks the maintained baseline, revert the smallest dependency batch first.
- Revert both manifest changes and `Cargo.lock` / `package-lock.json` changes together.
- Restore the last passing verification set before attempting a narrower retry.

## Naming And Compatibility

- Keep host command and event names on the current Rust-first naming:
  - `start_local_runtime`
  - `restart_local_runtime`
  - `local-runtime-restarted`
- Do not reintroduce older host command naming into active app code.
- Treat user data compatibility as a separate concern from internal refactors.
