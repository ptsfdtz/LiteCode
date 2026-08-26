# Roadmap

Roadmap items are complete only when their exit criteria are met on all stated targets.

## M0 - Foundation

- [x] Establish Rust workspace and crate boundaries.
- [x] Establish Flutter application source skeleton.
- [x] Document product scope, architecture, protocol, and security model.
- [x] Add cross-platform Rust validation workflow.
- [x] Generate Android and iOS Flutter platform shells using the Flutter SDK.
- [x] Add Flutter analysis and tests to CI.

Exit criteria: Rust checks pass locally and in CI; Flutter checks pass in CI.

## M1 - Local vertical slice

- [x] Agent startup configuration with one authorized workspace.
- [x] Authenticated local WebSocket session.
- [x] Fixed loopback connection from Flutter for desktop validation.
- [x] Start a supervised Codex task and stream structured events.
- [ ] Reconnect and replay missed events.

Exit criteria: a phone can run and reconnect to a mock task without losing output.

## M2 - Codex integration

- [ ] Detect Codex CLI and report capability.
- [x] Start Codex in an authorized workspace.
- [x] Stream normalized task state and Codex JSONL output.
- [ ] Send follow-up input.
- [ ] Resolve approvals and stop the task.
- [ ] Show task summary and changed files.

Exit criteria: the complete MVP journey works on the primary development platform.

## M3 - Cross-platform host support

- [ ] Windows PTY and lifecycle validation.
- [ ] macOS PTY and lifecycle validation.
- [ ] Linux PTY and lifecycle validation.
- [ ] Platform credential stores.
- [ ] Automated release artifacts for all three hosts.

Exit criteria: the same mobile build completes the MVP journey against every host OS.

## M4 - Product hardening

- [ ] mDNS discovery.
- [x] QR pairing, secure client credentials, and certificate-pinned TLS transport.
- [ ] Durable encrypted task metadata.
- [ ] Device revocation and security audit view.
- [ ] Tray/menu-bar configuration shell.
- [ ] Crash recovery and automatic update design.

Exit criteria: a non-developer can install, pair, use, and revoke LiteCode safely.
