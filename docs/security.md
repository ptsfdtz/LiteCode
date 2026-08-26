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

The mechanism and rollout constraints are recorded in ADR-0003.

## Local development exception

The initial M1 development slice was unauthenticated. The Agent now implements
one-time pairing and authenticated WebSocket upgrades, but still enforces all of the
following until TLS and mobile credential storage are complete:

- binds only to an IP loopback address;
- accepts only workspace ID `local` and tool ID `codex`;
- authorizes exactly one canonical workspace supplied when the agent starts;
- starts Codex with `workspace-write` sandboxing and ephemeral session storage;
- is not suitable for LAN or public network exposure.

Removing the loopback restriction requires TLS with certificate pinning, mobile QR
pairing and secure credential storage, working device revocation, and transport-level
integration tests.

## Explicit non-goals for the MVP

- Defending against an already-compromised host operating system.
- Sandboxing the AI CLI beyond its native sandbox and approval model.
- Secure public internet relay.

## Open security work

- Certificate generation, rotation, and platform credential-store abstractions.
- Device listing and revocation UX plus active-session invalidation.
- Exact approval risk classification.
- Task event retention and secure deletion policy.

Security-sensitive implementation must update this document and add an ADR when it
chooses one of these mechanisms.
