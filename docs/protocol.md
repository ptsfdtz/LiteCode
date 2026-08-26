# Client-agent protocol

## Status

Draft for protocol version 1. Rust types define the domain vocabulary and serialize to
JSON on the implemented local WebSocket slice.

## Transport

- Local discovery: mDNS with a non-sensitive agent identifier.
- Local demo session: unauthenticated WebSocket restricted to the loopback interface.
- Networked session target: authenticated WebSocket over TLS after device pairing.
- Encoding: JSON for the MVP.
- Compatibility: every envelope includes a protocol version.

## Envelope

The local slice currently sends tagged command and event objects directly. Versioned
envelopes, message identifiers, and replay sequences remain required before LAN use.

Events additionally include `taskId` and a monotonically increasing `sequence` when
they belong to a task.

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
- Secrets, environment values, and raw credentials are never protocol payloads.
