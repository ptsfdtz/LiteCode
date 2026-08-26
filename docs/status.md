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

## Current focus

- Add mobile QR scanning and platform-secure credential storage for the implemented
  pairing exchange.
- Add TLS certificate generation and QR fingerprint pinning before enabling LAN bind.
- Add durable task events and reconnect replay.

## Blockers

- None.

## Next actions

1. Implement the Flutter QR pairing flow and secure device credential storage.
2. Add Agent TLS identity, certificate fingerprint pinning, and revocation commands.
3. Add host-side workspace configuration and capability discovery.
4. Implement reconnect/event replay behavior.
5. Add explicit task cancellation and approval handling.
