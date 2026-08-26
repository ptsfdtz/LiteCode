# Client-agent protocol

## Status

Draft for protocol version 2. Rust types define task messages and the pairing exchange.
The Agent supports authenticated loopback transport and explicitly enabled TLS network
transport. The Flutter client parses QR invitations, pins the advertised certificate,
and stores paired credentials in the platform secure store.

## Transport

- Local discovery: mDNS with a non-sensitive agent identifier.
- Local development session: bearer-authenticated WebSocket restricted to loopback.
- Networked session target: authenticated WebSocket over TLS after device pairing.
- Encoding: JSON for the MVP.
- Compatibility: every envelope includes a protocol version.

## Envelope

The local slice currently sends tagged command and event objects directly. Versioned
envelopes and message identifiers remain required before protocol compatibility is
considered stable.

Every task event includes `task_id` and a monotonically increasing `sequence`, starting
at 1 independently for each task. Clients discard duplicate sequences and buffer gaps
until replay supplies the missing events.

## Pairing invitation

The QR payload uses the `litecode://pair` URI scheme and contains:

| Field | Purpose |
| --- | --- |
| `agent` | Stable, non-secret Agent installation ID |
| `endpoint` | URL-safe base64 encoded HTTPS origin |
| `fingerprint` | SHA-256 fingerprint of the Agent TLS certificate |
| `secret` | Random, single-use pairing secret valid for five minutes |

Plain loopback invitations omit `fingerprint` and advertise an HTTP origin. TLS
invitations include it. Clients reject plain non-loopback endpoints and malformed
SHA-256 fingerprints.

The host-side pairing interface exposes the current invitation through local control
endpoints. These endpoints are an Agent administration surface and reject non-loopback
sources:

| Endpoint | Purpose |
| --- | --- |
| `GET /pairing` | Render the local pairing interface |
| `GET /v1/pairing-invitation` | Return host, transport, expiry, and lifecycle status |
| `GET /v1/pairing-invitation/qr` | Render the active invitation as an SVG QR code |
| `POST /v1/pairing-invitation/regenerate` | Invalidate the prior invitation and create a five-minute invitation |
| `POST /v1/pairing-invitation/cancel` | Invalidate the active invitation |

The status response does not contain the pairing secret. The QR endpoint returns `404`
unless the invitation is active. Lifecycle status is one of `active`, `used`,
`cancelled`, or `expired`.

## Pairing exchange

`POST /v1/pair` accepts JSON with `protocolVersion`, `pairingSecret`, and `deviceName`.
On success it returns `protocolVersion`, `agentId`, `deviceId`, and
`deviceCredential`. The credential appears only in this response and subsequent
`Authorization: Bearer` headers; it is never included in WebSocket message bodies.

Errors use HTTP status `400` for protocol incompatibility, `401` for an invalid or used
invitation, and `429` for a rate-limited source. Error bodies contain stable codes and
never echo supplied values.

Regeneration and cancellation invalidate the previous secret. Neither operation can
make a consumed, cancelled, or expired invitation usable again.

WebSocket upgrades return `401` for missing, invalid, or revoked credentials and `429`
after repeated authentication failures from the same source.

## Client commands

| Command | Purpose |
| --- | --- |
| `create_task` | Start a task in an authorized workspace with a selected tool |
| `send_input` | Add follow-up input to a running task |
| `resolve_approval` | Approve once or reject a pending operation |
| `stop_task` | Request graceful task termination |
| `resume_events` | Replay task events after a known sequence |

`resume_events` contains `task_id` and `after_sequence`. The Agent returns retained
events for that task whose sequence is greater than `after_sequence`, in ascending
order. Live delivery can overlap replay, so clients must de-duplicate and restore
sequence order. An unknown task or a cursor at the latest event produces no events.

The current Agent retains task events only in memory. Temporary WebSocket disconnects
do not stop the task or lose events, but restarting the Agent clears replay history.

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
