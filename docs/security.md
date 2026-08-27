# Security model

## Protected assets

- Source code and files on the host.
- AI provider credentials available to local tools.
- Command execution capability.
- Task prompts, output, and audit metadata.
- Device identity and pairing credentials.

## Trust boundaries

- The paired phone is trusted to request actions, but high-risk actions still require
  explicit approval.
- The agent is the enforcement boundary and must not trust paths supplied by a client.
- The local network is untrusted.
- AI tool output is untrusted data and cannot grant permissions.

## MVP requirements

- Pairing requires a short-lived, user-visible secret or QR code.
- Each device receives a revocable identity.
- All post-pairing traffic is authenticated and encrypted.
- Workspace roots are configured and canonicalized on the host.
- Requested paths are canonicalized and checked against an authorized root.
- The client cannot submit an unrestricted shell command through the LiteCode protocol.
- Approval requests show the effective command/action and working directory.
- Sensitive environment variables and credentials are redacted from logs and events.
- Agent logs record security-relevant actions without recording secret values.
- Rate limits apply to pairing and authentication failures.

## Selected identity and pairing model

- An Agent installation has a stable, random, non-secret agent ID.
- A host-initiated QR invitation carries the agent ID, endpoint, TLS certificate
  fingerprint, and a random single-use secret that expires after five minutes.
- The pairing endpoint consumes the secret and issues a random device ID and bearer
  credential. The Agent persists only the credential hash.
- Every WebSocket upgrade requires `Authorization: Bearer <device credential>` and
  rejects missing, invalid, or revoked credentials before accepting commands.
- LAN pairing and sessions require TLS with the QR-provided certificate fingerprint
  pinned by the client. Plain `http` and `ws` are permitted only on loopback.
- Pairing failures are limited to five attempts per source per minute. Secrets and
  credentials must never appear in logs.
- The Agent pairing interface and its invitation status, QR, regeneration, and
  cancellation endpoints accept only loopback source addresses. They are not exposed
  as unauthenticated LAN administration endpoints, even when task transport uses TLS.
- Regenerating or cancelling an invitation immediately invalidates its prior secret.
  QR responses use `Cache-Control: no-store`.
- Authentication failures use the same per-source limit. Device registries and TLS
  private keys are written with owner-only permissions on Unix; Windows relies on the
  user profile directory ACL until the platform credential-store abstraction lands.
- Task replay is available only inside an already bearer-authenticated WebSocket
  session. The Agent retains replayable task prompts and output in process memory; it
  does not write them to disk in this slice, and clears them when the Agent process
  exits.
- `stop_task` is accepted only over the same bearer-authenticated WebSocket as other
  task commands. The Agent resolves the supplied task ID through its in-memory
  supervisor and terminates only that task. Unknown, repeated, and already-terminal
  cancellation requests are no-ops and do not affect other processes.
- Disconnecting a client never implies cancellation. Cancellation state and its
  replayable terminal event remain in memory only and are cleared by an Agent restart.
- Each task starts a Codex app-server stdio child and an ephemeral thread. LiteCode sets
  the authorized workspace as `cwd`, retains `workspace-write` sandboxing, and keeps
  approval policy at `never` until explicit mobile approval handling is implemented.
  Follow-up text is accepted only over an authenticated WebSocket and routed only to
  the matching active task's in-memory control channel. App-server threads are not
  materialized in Codex session storage.

The mechanism and rollout constraints are recorded in ADR-0003.

## Local development exception

The initial M1 development slice was unauthenticated. Plain `http` and `ws` mode now
implements one-time pairing and authenticated WebSocket upgrades, and still enforces
all of the following:

- binds only to an IP loopback address;
- accepts only workspace ID `local` and tool ID `codex`;
- authorizes exactly one canonical workspace supplied when the agent starts;
- starts Codex with `workspace-write` sandboxing and ephemeral session storage;
- is not suitable for LAN or public network exposure.

Non-loopback binding requires the explicit `--tls` flag and an advertised host. TLS
mode generates a per-installation self-signed identity, publishes its SHA-256
fingerprint in the pairing invitation, and is accepted by the client only when that
fingerprint matches. Device revocation rejects subsequent connection attempts.

## Explicit non-goals for the MVP

- Defending against an already-compromised host operating system.
- Sandboxing the AI CLI beyond its native sandbox and approval model.
- Secure public internet relay.

## Open security work

- Certificate rotation and host platform credential-store abstraction.
- Device management UI plus active-session invalidation after revocation. The minimal
  pairing interface does not list, modify, or revoke devices.
- Exact approval risk classification.
- Task event retention and secure deletion policy.

Security-sensitive implementation must update this document and add an ADR when it
chooses one of these mechanisms.
