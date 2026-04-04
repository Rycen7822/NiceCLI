<p align="center">
  <img src="./nicecli-logo.png" alt="NiceCLI" width="160" />
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-Windows-0078D4" alt="Windows" />
  <img src="https://img.shields.io/badge/runtime-Tauri%202-24C8DB" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/backend-CLIProxyAPI-00ADD8" alt="CLIProxyAPI" />
  <img src="https://img.shields.io/badge/distribution-portable%20exe-3A7AFE" alt="Portable exe" />
  <img src="https://img.shields.io/badge/mode-local--only-111111" alt="Local only" />
</p>

<p align="center">
  <a href="./README.md">English</a> | <a href="./README_CN.md">中文</a>
</p>

<h3 align="center">A local-first desktop control plane for CLIProxyAPI.</h3>

<p align="center">
  NiceCLI is a Windows-focused desktop application built on top of EasyCLI and CLIProxyAPI. It packages the desktop UI and the backend into a single local workflow so you can launch, manage auth files, inspect runtime state, and view Codex workspace quota without a separate backend install process.
</p>

<p align="center">
  <img src="./local_login.png" alt="NiceCLI local login" width="88%" />
</p>

<p align="center">
  <img src="./codex_workspace_quota.png" alt="NiceCLI workspace quota" width="88%" />
</p>

## Highlights

- Local-only startup flow with an embedded backend runtime
- Portable `nicecli.exe` distribution, no installer required
- Authentication file management with editable notes
- Codex Workspace Quota view across multiple workspaces under the same account
- Workspace-aware quota snapshots, reset countdown, and grouped filters
- NiceCLI-specific desktop shell, branding, and control-panel UX

## Quick Start

1. Install Microsoft Edge WebView2 Runtime on Windows if it is not already available.
2. Build or obtain `nicecli.exe`.
3. Launch `nicecli.exe`.
4. NiceCLI starts the bundled CLIProxyAPI desktop-lite runtime and opens the control panel.

## Development Prerequisites

- Node.js 18+
- Rust toolchain
- Go toolchain
- WebView2 Runtime on Windows

## Build

Build the portable executable from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\build-windows.ps1
```

Build output:

```text
apps\nicecli\src-tauri\target\release\nicecli.exe
```

Notes:

- This repository currently targets the portable executable workflow only.
- Setup or installer bundles are intentionally disabled.
- The Tauri build embeds the Go backend from `apps/cliproxyapi/cmd/desktoplite`.

## Repository Layout

- `apps/nicecli`: desktop frontend assets, Tauri shell, and embedded-backend packaging logic
- `apps/cliproxyapi`: Go backend source, management API, auth handling, and quota logic
- `scripts`: repository-level build helpers
- `docs`: architecture and maintenance notes
- `build-windows.ps1`: root build entrypoint

## Architecture

NiceCLI keeps a simple runtime shape:

- The frontend runs inside a Tauri desktop shell
- Tauri builds and embeds a desktop-lite CLIProxyAPI backend
- The UI talks to the backend through the local loopback management API

This keeps deployment simple while preserving the existing auth, quota, and management logic that already lives in CLIProxyAPI.

## Current Maintenance Direction

- Keep NiceCLI local-only
- Keep distribution as a single portable `nicecli.exe`
- Continue to evolve the auth-file and workspace-quota workflows
- Maintain the repository as one unified project instead of two parallel upstream-style trees

## License

This project is distributed under the license in [LICENSE](./LICENSE).
