# ADR-0001: Flutter mobile client and Rust desktop agent

- Status: Accepted
- Date: 2026-08-24

## Context

LiteCode needs one mobile client for Android and iOS and a host agent that runs on
Windows, macOS, and Linux. The host must supervise interactive child processes, enforce
filesystem boundaries, run as a background service, and ship with minimal runtime
requirements.

## Decision

Use Flutter and Dart for the mobile application. Use Rust for the host agent and shared
host-side domain libraries. Communicate through a versioned, implementation-neutral
protocol rather than sharing language-specific runtime objects.

A future Flutter desktop shell may configure the agent, but the Rust agent remains an
independent background process and security boundary.

## Consequences

- Mobile UI and behavior can be shared across Android and iOS.
- Agent core logic and binaries can be shared across the three host platforms.
- PTY, service, and credential-store implementations still require platform adapters.
- The project uses two languages and must validate both toolchains in CI.
- The protocol boundary must remain explicit and versioned.

