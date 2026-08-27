# Architecture

## System context

```text
Flutter mobile app
  |  discovery, pairing, commands, events
  |  authenticated WebSocket over the local network
  v
Rust desktop agent
  |-- pairing and device authorization
  |-- workspace authorization
  |-- durable task/session manager
  |-- AI tool adapter boundary
  |-- platform services boundary
  v
Codex CLI + authorized project files
```

## Components

### Mobile app

Presents computers, tasks, approvals, output, and results. It never accesses project
files directly and does not need to understand Codex-specific terminal formatting.

### Agent executable

Owns network sessions, authentication, task lifecycle, persistence, and process
supervision. It continues running tasks when the phone disconnects.

The current M1 slice provides `GET /health`, a one-time pairing exchange at
`POST /v1/pair`, and a bearer-authenticated WebSocket endpoint at `/v1/ws`. It owns a
stable Agent ID, TLS identity, and hashed device registry. Plain transport is restricted
to loopback; explicitly enabled TLS transport may bind to the LAN and advertises a
certificate fingerprint for client pinning.

The Agent also serves a minimal host pairing interface at `/pairing`. It renders the
current invitation locally as a QR code and supports regeneration and cancellation.
The page and all of its control/status endpoints require a loopback source address, so
the LAN listener does not become an unauthenticated administration interface. Pairing
state remains owned by the same authorization service used by `POST /v1/pair`; there is
no second invitation store or alternate authentication path.

Task processes publish sequenced events to an Agent-owned in-memory event store rather
than to one WebSocket connection. Authenticated sessions subscribe to live events and
can request events after their last accepted sequence. This keeps task execution and
event capture alive through a temporary client disconnect. Flutter reconnects with
bounded backoff, requests the missing sequence range, buffers out-of-order arrivals,
and suppresses duplicates. Restart persistence is not part of this slice.

An Agent-owned in-memory task supervisor registers each spawned Codex process by task
ID. Authenticated `stop_task` commands signal only the matching process. Registration
also records stopping and terminal states, making unknown, repeated, and late Stop
requests safe no-ops. Process termination publishes one `task_stopped` terminal event
through the same live and retained event store. Supervisor and cancellation state are
not persisted across Agent restarts.

The Codex adapter now supervises one stdio `codex app-server` process per LiteCode task.
It initializes an ephemeral thread rooted at the startup-authorized workspace, starts
one turn, normalizes streamed app-server items into retained task output, and maps
`send_input` to `turn/steer` and Stop to `turn/interrupt`. The supervisor control
channel is task-ID-scoped and survives Flutter disconnects while the Agent remains
running. Ephemeral threads are not written to Codex session storage. The same
bidirectional adapter boundary will carry explicit approval requests and decisions in
the next slice.

### Core library

Contains platform-independent workspace and task rules. It does not depend on network,
storage, PTY, or UI implementations.

### Protocol library

Defines versioned commands, events, identifiers, and error semantics. Transport
serialization will be added with the first WebSocket slice.

### AI adapter

Converts the common task model to tool-specific process input and converts tool output
and approval requests into structured agent events.

### Platform services

Encapsulates ConPTY/PTY, credential storage, service installation, process discovery,
and platform-specific paths behind narrow interfaces.

## Data ownership

- The host owns workspace authorization and task execution.
- The agent owns durable task state and audit metadata.
- The mobile app may cache display state but is never authoritative.
- Source code remains on the host in the MVP.

## Reliability rules

- A network session and a task lifetime are independent.
- Commands use stable identifiers so retries can be made idempotent.
- Events have a monotonically increasing sequence per task for reconnect replay.
- Stopping the agent is explicit; closing the mobile app does not stop tasks.

## Implemented local slice

```text
Flutter Windows client
  -> one-time loopback pairing at http://127.0.0.1:47831/v1/pair
  -> authenticated ws://127.0.0.1:47831/v1/ws
  -> Rust agent with one startup-authorized workspace
  -> codex app-server --stdio with an ephemeral workspace-write thread
  -> structured task events returned to Flutter
```

This slice validates component boundaries, process streaming, host identity, one-time
pairing, upgrade authentication, encrypted LAN transport, mobile QR handling, secure
client credential storage, a local host pairing interface, and revocation of new
sessions. It now provides reconnect and replay guarantees plus explicit task
cancellation while the Agent process remains running. It does not yet provide device
management UI, mDNS, active-session invalidation, restart persistence, or approval
handling.

## Planned dependency direction

```text
litecode-agent -> litecode-core -> litecode-protocol
litecode-agent -----------------> litecode-protocol
```

The core and protocol crates must not depend on the executable or platform adapters.
