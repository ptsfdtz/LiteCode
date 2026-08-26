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
  -> codex exec --json --ephemeral --sandbox workspace-write
  -> structured task events returned to Flutter
```

This slice validates component boundaries, process streaming, host identity, one-time
pairing, upgrade authentication, encrypted LAN transport, mobile QR handling, secure
client credential storage, and revocation of new sessions. It does not yet provide
mDNS, active-session invalidation, task persistence, approval, or reconnect guarantees.

## Planned dependency direction

```text
litecode-agent -> litecode-core -> litecode-protocol
litecode-agent -----------------> litecode-protocol
```

The core and protocol crates must not depend on the executable or platform adapters.
