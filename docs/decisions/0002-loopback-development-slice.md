# ADR-0002: Loopback-only development slice

- Status: Accepted
- Date: 2026-08-26

## Context

The project needs to validate Flutter, Agent, and Codex process integration before the
device-pairing and authenticated transport design is complete. Exposing an
unauthenticated command-execution service on a LAN would violate the security model.

## Decision

Implement the first vertical slice as an unauthenticated WebSocket service that rejects
all non-loopback bind addresses. The agent authorizes one canonical workspace at
startup, accepts only the `local` workspace and `codex` tool IDs, and starts Codex with
`workspace-write` sandboxing and ephemeral session storage.

The Flutter Windows target uses the fixed endpoint `ws://127.0.0.1:47831/v1/ws` for
local development. This is a temporary development boundary, not the networked MVP
transport design.

## Consequences

- The complete process and UI chain can be tested safely on one computer.
- The service cannot yet support a phone or another computer.
- Pairing, TLS, device identity, revocation, replay, and approvals remain mandatory
  before removing the loopback restriction.
- The direct JSON command/event objects may evolve into versioned envelopes before LAN
  compatibility is promised.
