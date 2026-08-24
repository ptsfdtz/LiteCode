# Product specification

## Product statement

LiteCode lets a developer start, monitor, guide, and approve AI coding tasks running on
their own computers from a phone.

## Primary user

A developer who owns multiple Windows, macOS, or Linux computers, already uses an AI
coding CLI, and needs to supervise longer-running work while away from the keyboard.

## Core job

When an AI coding task is running on another computer, help the user understand its
state and safely provide the next decision without operating a full remote desktop.

## MVP journey

1. Start the LiteCode agent on a computer.
2. Pair the Flutter app with that agent over the local network.
3. Select a workspace previously authorized on the computer.
4. Select Codex and submit a prompt.
5. Observe task output and state changes on the phone.
6. Approve or reject an operation that needs confirmation.
7. Send follow-up input or stop the task.
8. Review the completion summary and changed files.

## MVP scope

### Included

- Flutter app for Android and iOS.
- Rust agent for Windows, macOS, and Linux.
- Local-network discovery and connection.
- Explicit device pairing.
- Explicit workspace authorization.
- One AI adapter: Codex CLI.
- Task output, follow-up input, approval, stop, and reconnect.
- Task summary and changed-file list.

### Excluded

- Full remote terminal or remote desktop.
- Mobile code editor.
- Public internet relay.
- Collaborative/multi-user access.
- Automatic commits, pushes, or deployments.
- Additional AI tools before the Codex flow is stable.

## MVP success criteria

- A user completes the entire journey without touching the host after agent startup.
- A temporary phone disconnect does not terminate the task.
- An unpaired device cannot access workspaces or task output.
- The agent never exposes a path outside an explicitly authorized workspace.
- The same agent core passes automated tests on Windows, macOS, and Linux.

## Product principles

- Tasks, not terminals, are the primary user-facing object.
- The host remains authoritative for execution and permissions.
- Remote convenience must not silently expand filesystem or command access.
- Mobile screens prioritize status, decisions, and concise results.

