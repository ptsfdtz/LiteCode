# LiteCode

LiteCode is a mobile control plane for AI coding tasks running on your own computers.
The phone coordinates tasks, approvals, and results; the actual development work stays
on an authorized Windows, macOS, or Linux machine.

## Current status

The repository is in the foundation phase. It contains:

- a cross-platform Rust agent workspace;
- a Flutter mobile application shell;
- shared task and protocol domain models;
- product, architecture, security, protocol, roadmap, and decision records.

The first vertical slice will connect the Flutter app to a local agent, start a Codex
task in an authorized workspace, stream its output, and handle user approval.

## Repository layout

```text
apps/mobile/                 Flutter mobile application
crates/litecode-agent/      Desktop agent executable
crates/litecode-core/       Task and workspace domain logic
crates/litecode-protocol/   Messages shared across the connection boundary
docs/                       Product and engineering documentation
```

## Development

### Rust agent

```bash
cargo run -p litecode-agent -- status
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

### Local end-to-end demo

Create an isolated workspace, then start the loopback-only agent:

```powershell
New-Item -ItemType Directory -Force test-workspaces/e2e
cargo run -p litecode-agent -- serve --workspace test-workspaces/e2e
```

In a second terminal, start the Windows Flutter client:

```powershell
cd apps/mobile
flutter run -d windows
```

The current vertical slice deliberately listens only on `127.0.0.1`. It is suitable
for local development, not for LAN or internet exposure.

### Flutter app

Flutter 3.47 or newer is recommended. The Android and iOS platform shells are checked
in; run the standard validation commands from the mobile app directory:

```bash
cd apps/mobile
flutter pub get
flutter analyze
flutter test
flutter run
```

## Documentation workflow

Start with [docs/index.md](docs/index.md). Product behavior belongs in the product
specification, durable technical choices get an ADR, and delivery progress is updated
in the roadmap and current status document.
