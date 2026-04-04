# Architecture

## Runtime Shape

- `apps/nicecli` provides the desktop UI and the Tauri host process.
- `apps/cliproxyapi` provides the embedded Go backend and management API.
- The frontend talks to the backend through the local loopback HTTP management endpoints.

## Build Chain

1. `build-windows.ps1` calls `scripts/build-windows.ps1`
2. `scripts/build-windows.ps1` runs `npm run build` inside `apps/nicecli`
3. `apps/nicecli/src-tauri/build.rs` compiles `apps/cliproxyapi/cmd/desktoplite`
4. The compiled backend binary and `config.example.yaml` are embedded into the final Tauri executable

## Sensitive Areas

- Authentication orchestration: `apps/cliproxyapi/sdk/cliproxy/auth`
- Quota snapshot and aggregation: `apps/cliproxyapi/internal/quota`
- Management API surface: `apps/cliproxyapi/internal/api/handlers/management`

## Compatibility Notes

- NiceCLI remains `local-only`
- Distribution remains a single `nicecli.exe`
- The backend is still started and managed by the Tauri shell rather than a separate installer or service
