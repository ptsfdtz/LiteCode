# Client-agent protocol

## Status

Draft for protocol version 1. Rust types define task messages and the pairing exchange.
The implemented loopback slice authenticates WebSocket upgrades; TLS and mobile QR
handling are not yet implemented, so non-loopback binding remains prohibited.

## Transport

- Local discovery: mDNS with a non-sensitive agent identifier.
- Local development session: bearer-authenticated WebSocket restricted to loopback.
- Networked session target: authenticated WebSocket over TLS after device pairing.
- Encoding: JSON for the MVP.
- Compatibility: every envelope includes a protocol version.

## Envelope

The local slice currently sends tagged command and event objects directly. Versioned
envelopes, message identifiers, and replay sequences remain required before LAN use.

Events additionally include `taskId` and a monotonically increasing `sequence` when
they belong to a task.

## Pairing invitation

The QR payload uses the `litecode://pair` URI scheme and contains:

| Field | Purpose |
| --- | --- |
| `agent` | Stable, non-secret Agent installation ID |
| `endpoint` | URL-safe base64 encoded HTTPS origin |
| `fingerprint` | SHA-256 fingerprint of the Agent TLS certificate |
| `secret` | Random, single-use pairing secret valid for five minutes |

The current loopback implementation omits `fingerprint` and advertises an HTTP origin.
Clients must reject that form for a non-loopback endpoint.

## Pairing exchange

`POST /v1/pair` accepts JSON with `protocolVersion`, `pairingSecret`, and `deviceName`.
On success it returns `protocolVersion`, `agentId`, `deviceId`, and
`deviceCredential`. The credential appears only in this response and subsequent
`Authorization: Bearer` headers; it is never included in WebSocket message bodies.

Errors use HTTP status `400` for protocol incompatibility, `401` for an invalid or used
invitation, and `429` for a rate-limited source. Error bodies contain stable codes and
never echo supplied values.

## Client commands

| Command | Purpose |
| --- | --- |
| `create_task` | Start a task in an authorized workspace with a selected tool |
| `send_input` | Add follow-up input to a running task |
| `resolve_approval` | Approve once or reject a pending operation |
| `stop_task` | Request graceful task termination |
| `resume_events` | Replay task events after a known sequence |

## Agent events

| Event | Purpose |
| --- | --- |
| `task_started` | Confirms process startup |
| `output_delta` | Streams displayable task output |
| `approval_required` | Requests an explicit mobile decision |
| `task_completed` | Reports a successful terminal state and summary |
| `task_failed` | Reports a failure and safe diagnostic message |

## Compatibility rules

- Unknown optional fields are ignored.
- Unknown message types receive an `unsupported_message` error.
- Breaking schema or semantic changes increment `protocolVersion`.
- Pairing secrets occur only in the one-time pairing request. Device credentials,
  environment values, and other secrets are never WebSocket message payloads.
