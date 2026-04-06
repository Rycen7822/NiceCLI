# Security Policy

## Supported Scope

Security reports are relevant for the actively maintained NiceCLI desktop codebase in this repository, especially:

- local management endpoints
- auth file handling
- OAuth callback handling
- config persistence
- local runtime startup and process management

## Reporting A Vulnerability

Do not open a public issue for an exploitable security problem.

Preferred process:

1. Use GitHub private vulnerability reporting if it is enabled for the repository.
2. If private reporting is not available, contact the maintainer through a private support channel first.
3. Include a minimal reproduction, affected version, impact, and any mitigation you already tested.

## Response Expectations

- The maintainer should confirm receipt after reviewing the report.
- Fixes should be coordinated before public disclosure when the issue is still exploitable.
- Public write-ups should wait until users have a reasonable path to update.

## Non-Security Issues

General bugs, build failures, and feature requests should go to the normal support path in `SUPPORT.md`.
