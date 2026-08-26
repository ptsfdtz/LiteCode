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
- Added a loopback-only Agent pairing interface with host and transport details, a
  scannable QR invitation, a five-minute countdown, and invitation regeneration and
  cancellation controls.
- Added protocol version 2 task-event sequencing, Agent-side in-memory event retention,
  `resume_events` replay, and Flutter automatic reconnect with bounded backoff,
  duplicate suppression, and actionable connection errors.

## Current focus

- Add follow-up task input, cancellation, and approval handling.
- Add mDNS discovery and invalidate active sessions immediately after device revocation.

## Blockers

- None.

## Next actions

1. Add explicit follow-up input, task cancellation, and approval handling.
2. Add host-side workspace configuration and capability discovery.
3. Add mDNS discovery and active-session revocation.
4. Persist task state and events across Agent restarts.
5. Validate native pairing and secure storage on Android and iOS devices when suitable
   hardware is available.
