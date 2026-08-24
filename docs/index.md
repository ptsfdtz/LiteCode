# LiteCode documentation

This directory is the source of truth for product scope and engineering decisions.

## Documents

| Document | Purpose | Update trigger |
| --- | --- | --- |
| [product.md](product.md) | Users, problems, MVP scope, and acceptance criteria | Product behavior changes |
| [architecture.md](architecture.md) | System boundaries and component responsibilities | Component or data-flow changes |
| [protocol.md](protocol.md) | Client-agent messages and compatibility rules | Protocol changes |
| [security.md](security.md) | Threat model, trust boundaries, and security requirements | New capability or trust boundary |
| [roadmap.md](roadmap.md) | Ordered milestones and exit criteria | Milestone planning or completion |
| [status.md](status.md) | Current focus, completed work, blockers, and next actions | Every meaningful delivery |
| [decisions/](decisions/) | Architecture Decision Records (ADRs) | Durable technical decision |

## Maintenance rules

1. Keep `status.md` factual and current; it is not a changelog.
2. Update roadmap checkboxes only when their exit criteria are met.
3. Record decisions that constrain future implementation as an ADR.
4. Update protocol and security documents in the same change as related code.
5. Keep implementation-specific task lists in issues, not in the product specification.

