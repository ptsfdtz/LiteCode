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

## Explicit non-goals for the MVP

- Defending against an already-compromised host operating system.
- Sandboxing the AI CLI beyond its native sandbox and approval model.
- Secure public internet relay.

## Open security decisions

- Device identity and certificate lifecycle.
- Host credential-store abstraction.
- Exact approval risk classification.
- Task event retention and secure deletion policy.

Security-sensitive implementation must update this document and add an ADR when it
chooses one of these mechanisms.

