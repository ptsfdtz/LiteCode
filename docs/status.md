# Project status

Last updated: 2026-08-26

## Current phase

M1 - Local vertical slice

## Completed

- Defined the mobile task-control product boundary and MVP journey.
- Created the Rust agent, core, and protocol workspace.
- Created the Flutter application source and widget-test skeleton.
- Generated the official Android and iOS Flutter platform shells.
- Added architecture, protocol, security, roadmap, and ADR documentation.
- Added continuous integration for Rust on Windows, macOS, and Linux.
- Validated Flutter analysis, widget tests, and an Android debug APK build.
- Added the Flutter Windows development target and built the desktop client.
- Added the loopback WebSocket Agent and structured JSON command/event transport.
- Added supervised `codex exec` streaming with a startup-authorized workspace.
- Verified Flutter -> Agent -> Codex end to end with exact file-content validation.
- Added a stable Agent identity and persistent hashed device registry.
- Added a five-minute, single-use pairing invitation and rate-limited pairing endpoint.
- Required a valid device bearer credential before WebSocket upgrade while retaining
  the loopback-only bind restriction.
- Accepted the QR pairing, TLS pinning, device credential, and revocation design in
  ADR-0003.
- Added Flutter QR scanning, invitation validation, and platform-secure credential
  storage with a desktop paste fallback.
- Added per-installation TLS identity generation, QR certificate fingerprint pinning,
  and an explicit TLS-only path for non-loopback binding.
- Added device listing and revocation commands; revoked credentials cannot establish
  new sessions.

## Current focus

- Add durable task events and reconnect replay.
- Add mDNS discovery and invalidate active sessions immediately after device revocation.

## Blockers

- None.

## Next actions

1. Implement reconnect/event replay behavior.
2. Add mDNS discovery and active-session revocation.
3. Add host-side workspace configuration and capability discovery.
4. Add explicit task cancellation and approval handling.
5. Validate native pairing and secure storage on Android and iOS devices.
