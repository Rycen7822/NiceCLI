# Architecture

## Runtime Shape

NiceCLI is a local-only desktop application with a Tauri shell and a Rust backend.

- `apps/nicecli`
  - desktop UI and Tauri host
- `crates/nicecli-backend`
  - local loopback HTTP management and public API surface
- `crates/nicecli-runtime`
  - request routing, execution, auth selection, and state writeback
- `crates/nicecli-auth`
  - login, OAuth, device flow, and auth import
- `crates/nicecli-quota`
  - quota refresh, snapshot, and aggregation
- `crates/nicecli-models`
  - model catalog loading and refresh
- `crates/nicecli-config`
  - config parsing and lossless YAML writeback
- `crates/nicecli-contract-tests`
  - route, fixture, and structure guards

## Runtime Boundaries

- The frontend talks to the backend through local loopback HTTP.
- The backend route contract is anchored in `crates/nicecli-backend/src/contract.rs`.
- The default desktop path is the in-process Rust backend hosted by Tauri.
- `rust-external` remains only as an explicit fallback for transition or debugging scenarios.

## Build Shape

1. `build-windows.ps1` is the root build entrypoint.
2. `scripts/build-windows.ps1` prepares frontend assets and runs the app build.
3. `apps/nicecli/src-tauri/build.rs` no longer builds or bundles a separate backend payload.
4. The final Windows artifact is a single portable Tauri executable that links the Rust backend directly.

## Compatibility Assumptions

- NiceCLI stays `local-only`.
- Distribution stays portable single-exe.
- Existing `config.yaml`, auth files, remarks, workspace naming, and quota grouping semantics must remain compatible.
- Frontend pages are not part of the current migration or restructuring work unless explicitly requested.

## Change Guidance

- Keep new behavior inside existing boundaries before introducing new crates or registries.
- When changing backend routes, keep `contract.rs`, contract tests, and structure guards aligned.
- When changing auth, quota, or config persistence, treat them as high-risk compatibility areas.

For current maintenance workflow, see [maintenance.md](./maintenance.md).
