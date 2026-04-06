# Docs

This directory only keeps current maintenance documents for the active NiceCLI codebase.

## Document Map

- `D:\dev\wincli\plan.md`
  - long-lived constraints, phase status, decisions, and execution order
- `architecture.md`
  - current runtime shape, module boundaries, and compatibility assumptions
- `maintenance.md`
  - day-to-day build, verification, CI, and release workflow
- `../CONTRIBUTING.md`
  - contributor workflow and verification expectations
- `../SECURITY.md`
  - security reporting policy
- `../SUPPORT.md`
  - support and bug-report entrypoint
- `../CHANGELOG.md`
  - maintenance-facing change history

## Rules

- Keep this directory small and current.
- Put long-running planning, stage tracking, and cross-session decisions in `plan.md`.
- Do not keep migration logs, temporary notes, or obsolete history here.
- If a detail changes often and is not needed for daily maintenance, remove it instead of documenting stale snapshots.
