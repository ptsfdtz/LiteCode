# ADR-0003: Device pairing and authenticated transport

- Status: Accepted
- Date: 2026-08-26

## Context

The loopback development slice proved the Flutter, Agent, and Codex process chain, but
its unauthenticated WebSocket cannot be exposed to an untrusted local network. LiteCode
needs a host identity that a phone can recognize, an explicit user-mediated pairing
ceremony, revocable device identities, and encryption plus authentication for every
network command.

## Decision

Each Agent installation owns a stable random `agentId`. Pairing starts only after a
host-side user action and produces a QR invitation containing the agent ID, endpoint,
TLS certificate fingerprint, and a cryptographically random, single-use secret with a
five-minute lifetime. The QR code is a transport for the invitation, not an
authorization boundary by itself.

The phone connects with TLS and pins the certificate fingerprint from the invitation.
It sends the pairing secret, protocol version, and a user-visible device name to
`POST /v1/pair`. A successful exchange consumes the secret and returns a random device
ID and bearer credential. The Agent stores only a SHA-256 digest of that credential;
the phone stores the credential in the platform credential store. Pairing and
authentication failures are rate limited without logging secret values.

After pairing, the client opens `wss://.../v1/ws` with the device credential in the
`Authorization: Bearer` header. The Agent authenticates the upgrade before accepting
commands and checks that the device has not been revoked. Revocation invalidates new
connections immediately and closes existing sessions as soon as practical.

The implementation delivers the identity, one-time exchange, hashed device registry,
rate limiting, authenticated WebSocket upgrade, TLS identity, certificate pinning,
mobile QR handling, secure client credential storage, and device revocation commands.
Plain transport remains loopback-only. Non-loopback binding is available only with the
explicit `--tls` option and an advertised host.

The Agent exposes a minimal host-side pairing page that shows its computer name, Agent
ID, listening address, TLS state, invitation expiry, and a scannable QR code. Local
controls may cancel the current invitation or atomically invalidate it and generate a
new five-minute invitation. A successful pairing consumes the invitation as before.
The page and its status, QR, and mutation endpoints reject every non-loopback source;
they are not a LAN device-management API.

## Consequences

- Knowing a network endpoint is insufficient to execute commands.
- Capturing a used or expired QR invitation cannot create another device.
- A copied live device credential remains sensitive and must be protected by platform
  credential stores and TLS.
- Self-signed per-installation certificates are acceptable because the QR invitation
  pins their fingerprint; the operating-system public CA store is not the trust root.
- Device registry migrations and certificate rotation require explicit compatibility
  handling.
- ADR-0002 remains in force for plain transport; it does not permit non-loopback
  `http` or `ws`.
- The host pairing interface does not change WebSocket authorization: every upgrade
  still requires a valid, non-revoked bearer credential.
