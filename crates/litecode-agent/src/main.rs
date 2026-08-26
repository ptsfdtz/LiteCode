mod auth;
mod pairing_ui;
mod tls;

use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    extract::{
        ConnectInfo, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use litecode_protocol::{
    AgentEvent, ClientCommand, PROTOCOL_VERSION, PairDeviceRequest, PairDeviceResponse, TaskId,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::mpsc,
};

const DEFAULT_BIND: &str = "127.0.0.1:47831";

#[derive(Clone)]
struct AppState {
    auth: auth::AuthService,
    workspace: Arc<PathBuf>,
    computer_name: Arc<str>,
    endpoint: Arc<str>,
    tls_enabled: bool,
    fingerprint: Option<Arc<str>>,
    task_events: TaskEventStore,
}

#[derive(Clone, Default)]
struct TaskEventStore {
    inner: Arc<Mutex<TaskEventState>>,
}

#[derive(Default)]
struct TaskEventState {
    histories: HashMap<String, TaskHistory>,
    subscribers: Vec<mpsc::UnboundedSender<AgentEvent>>,
}

#[derive(Default)]
struct TaskHistory {
    next_sequence: u64,
    events: Vec<AgentEvent>,
}

impl TaskEventStore {
    fn subscribe(&self) -> mpsc::UnboundedReceiver<AgentEvent> {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.inner
            .lock()
            .expect("task event mutex poisoned")
            .subscribers
            .push(sender);
        receiver
    }

    fn publish(&self, task_id: &TaskId, build: impl FnOnce(u64) -> AgentEvent) {
        let mut state = self.inner.lock().expect("task event mutex poisoned");
        let history = state
            .histories
            .entry(task_id.as_str().to_owned())
            .or_default();
        history.next_sequence += 1;
        let event = build(history.next_sequence);
        history.events.push(event.clone());
        state
            .subscribers
            .retain(|subscriber| subscriber.send(event.clone()).is_ok());
    }

    fn replay(&self, task_id: &TaskId, after_sequence: u64) -> Vec<AgentEvent> {
        self.inner
            .lock()
            .expect("task event mutex poisoned")
            .histories
            .get(task_id.as_str())
            .map(|history| {
                history
                    .events
                    .iter()
                    .filter(|event| event_sequence(event) > after_sequence)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn event_sequence(event: &AgentEvent) -> u64 {
    match event {
        AgentEvent::TaskStarted { sequence, .. }
        | AgentEvent::OutputDelta { sequence, .. }
        | AgentEvent::ApprovalRequired { sequence, .. }
        | AgentEvent::TaskCompleted { sequence, .. }
        | AgentEvent::TaskFailed { sequence, .. } => *sequence,
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("litecode-agent: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.first().map(String::as_str) {
        None | Some("status") => {
            print_status();
            Ok(())
        }
        Some("serve") => {
            let workspace = required_flag(&arguments, "--workspace")?;
            let bind = optional_flag(&arguments, "--bind").unwrap_or(DEFAULT_BIND);
            let tls = has_flag(&arguments, "--tls");
            let advertise_host = optional_flag(&arguments, "--advertise-host");
            let state_dir = optional_flag(&arguments, "--state-dir")
                .map_or_else(default_state_dir, PathBuf::from);
            serve(
                PathBuf::from(workspace),
                bind,
                state_dir,
                tls,
                advertise_host,
            )
            .await
        }
        Some("devices") => {
            let state_dir = optional_flag(&arguments, "--state-dir")
                .map_or_else(default_state_dir, PathBuf::from);
            print_devices(&state_dir)
        }
        Some("revoke-device") => {
            let device_id = required_flag(&arguments, "--device")?;
            let state_dir = optional_flag(&arguments, "--state-dir")
                .map_or_else(default_state_dir, PathBuf::from);
            revoke_device(&state_dir, device_id)
        }
        Some("version" | "--version" | "-V") => {
            println!("litecode-agent {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        Some(command) => Err(format!("unknown command: {command}")),
    }
}

fn required_flag<'a>(arguments: &'a [String], name: &str) -> Result<&'a str, String> {
    optional_flag(arguments, name).ok_or_else(|| format!("missing required option {name}"))
}

fn optional_flag<'a>(arguments: &'a [String], name: &str) -> Option<&'a str> {
    arguments
        .iter()
        .position(|value| value == name)
        .and_then(|position| arguments.get(position + 1))
        .map(String::as_str)
}

fn has_flag(arguments: &[String], name: &str) -> bool {
    arguments.iter().any(|value| value == name)
}

fn default_state_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_STATE_HOME"))
        .map_or_else(|| PathBuf::from(".litecode"), PathBuf::from)
        .join("LiteCode")
}

fn print_devices(state_dir: &Path) -> Result<(), String> {
    let auth = auth::AuthService::load(state_dir.join("devices.json"))?;
    let devices = auth.devices();
    if devices.is_empty() {
        println!("No paired devices.");
        return Ok(());
    }
    for device in devices {
        let state = if device.revoked { "revoked" } else { "active" };
        println!(
            "{}\t{}\t{}\tcreated {}",
            device.id, device.name, state, device.created_at_unix
        );
    }
    Ok(())
}

fn revoke_device(state_dir: &Path, device_id: &str) -> Result<(), String> {
    let auth = auth::AuthService::load(state_dir.join("devices.json"))?;
    if !auth.revoke(device_id)? {
        return Err(format!("unknown device: {device_id}"));
    }
    println!("Revoked device {device_id}");
    Ok(())
}

async fn serve(
    workspace: PathBuf,
    bind: &str,
    state_dir: PathBuf,
    tls_enabled: bool,
    advertised_host_override: Option<&str>,
) -> Result<(), String> {
    let workspace = workspace
        .canonicalize()
        .map_err(|error| format!("invalid workspace {}: {error}", workspace.display()))?;
    let address: SocketAddr = bind
        .parse()
        .map_err(|error| format!("invalid bind address {bind}: {error}"))?;
    if !address.ip().is_loopback() && !tls_enabled {
        return Err("non-loopback binding requires --tls".into());
    }
    let advertised_host = advertised_host_override.unwrap_or_else(|| {
        if address.ip().is_unspecified() {
            ""
        } else {
            bind.rsplit_once(':').map_or(bind, |(host, _)| host)
        }
    });
    if advertised_host.is_empty() {
        return Err("--advertise-host is required for an unspecified bind address".into());
    }

    let auth = auth::AuthService::load(state_dir.join("devices.json"))?;
    let computer_name: Arc<str> = computer_name().into();
    println!("Authorized workspace: {}", workspace.display());
    if tls_enabled {
        let identity = tls::load_or_create(&state_dir, advertised_host).await?;
        let endpoint = format!("https://{advertised_host}:{}", address.port());
        let app = app_router(AppState {
            auth: auth.clone(),
            workspace: Arc::new(workspace.clone()),
            computer_name: computer_name.clone(),
            endpoint: endpoint.clone().into(),
            tls_enabled: true,
            fingerprint: Some(identity.fingerprint.clone().into()),
            task_events: TaskEventStore::default(),
        });
        println!("LiteCode agent listening on {endpoint}/v1/ws");
        println!("Pairing interface: {endpoint}/pairing (available from this computer only)");
        println!(
            "Pairing invitation (valid once for 5 minutes): {}",
            auth.invitation_uri(&endpoint, Some(&identity.fingerprint))
        );
        axum_server::bind_rustls(address, identity.config)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .map_err(|error| format!("TLS server failed: {error}"))
    } else {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|error| format!("cannot bind {address}: {error}"))?;
        let endpoint = format!("http://{advertised_host}:{}", address.port());
        let app = app_router(AppState {
            auth: auth.clone(),
            workspace: Arc::new(workspace.clone()),
            computer_name,
            endpoint: endpoint.clone().into(),
            tls_enabled: false,
            fingerprint: None,
            task_events: TaskEventStore::default(),
        });
        println!("LiteCode agent listening on {endpoint}/v1/ws");
        println!("Pairing interface: {endpoint}/pairing");
        println!(
            "Pairing invitation (valid once for 5 minutes): {}",
            auth.invitation_uri(&endpoint, None)
        );
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .map_err(|error| format!("server failed: {error}"))
    }
}

fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/pairing", get(pairing_ui::page))
        .route("/v1/pairing-invitation", get(pairing_ui::status))
        .route("/v1/pairing-invitation/qr", get(pairing_ui::qr))
        .route(
            "/v1/pairing-invitation/regenerate",
            axum::routing::post(pairing_ui::regenerate),
        )
        .route(
            "/v1/pairing-invitation/cancel",
            axum::routing::post(pairing_ui::cancel),
        )
        .route("/v1/pair", axum::routing::post(pair_device))
        .route("/v1/ws", get(websocket_upgrade))
        .with_state(state)
}

fn computer_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "This computer".into())
}

async fn health() -> &'static str {
    "ok"
}

async fn websocket_upgrade(
    upgrade: WebSocketUpgrade,
    State(state): State<AppState>,
    ConnectInfo(source): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    let credential = bearer_credential(&headers).unwrap_or_default();
    let authenticated = state
        .auth
        .authenticate(&source.ip().to_string(), credential)
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    if !authenticated {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(upgrade.on_upgrade(move |socket| handle_socket(socket, state)))
}

async fn pair_device(
    State(state): State<AppState>,
    ConnectInfo(source): ConnectInfo<SocketAddr>,
    Json(request): Json<PairDeviceRequest>,
) -> Result<Json<PairDeviceResponse>, (StatusCode, &'static str)> {
    if request.protocol_version != PROTOCOL_VERSION {
        return Err((StatusCode::BAD_REQUEST, "unsupported_protocol_version"));
    }
    let paired = state
        .auth
        .pair(
            &source.ip().to_string(),
            &request.pairing_secret,
            &request.device_name,
        )
        .map_err(|code| {
            let status = if code == "pairing_rate_limited" {
                StatusCode::TOO_MANY_REQUESTS
            } else {
                StatusCode::UNAUTHORIZED
            };
            (status, code)
        })?;
    Ok(Json(PairDeviceResponse {
        protocol_version: PROTOCOL_VERSION,
        agent_id: paired.agent_id,
        device_id: paired.device_id,
        device_credential: paired.credential,
    }))
}

fn bearer_credential(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .filter(|value| !value.is_empty())
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut event_receiver = state.task_events.subscribe();
    let (direct_sender, mut direct_receiver) = mpsc::unbounded_channel::<AgentEvent>();

    let writer = tokio::spawn(async move {
        loop {
            let event = tokio::select! {
                event = event_receiver.recv() => event,
                event = direct_receiver.recv() => event,
            };
            let Some(event) = event else {
                break;
            };
            let Ok(payload) = serde_json::to_string(&event) else {
                continue;
            };
            if sender.send(Message::Text(payload.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(message)) = receiver.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        match serde_json::from_str::<ClientCommand>(&text) {
            Ok(ClientCommand::CreateTask {
                task_id,
                workspace_id,
                tool,
                prompt,
            }) if workspace_id == "local" && tool == "codex" && !prompt.trim().is_empty() => {
                let workspace = Arc::clone(&state.workspace);
                let events = state.task_events.clone();
                tokio::spawn(async move {
                    run_codex(task_id, prompt, workspace.as_ref(), events).await;
                });
            }
            Ok(ClientCommand::CreateTask { task_id, .. }) => {
                let event_task_id = task_id.clone();
                state
                    .task_events
                    .publish(&task_id, |sequence| AgentEvent::TaskFailed {
                        task_id: event_task_id,
                        sequence,
                        message: "unsupported workspace, tool, or empty prompt".into(),
                    });
            }
            Ok(ClientCommand::ResumeEvents {
                task_id,
                after_sequence,
            }) => {
                for event in state.task_events.replay(&task_id, after_sequence) {
                    if direct_sender.send(event).is_err() {
                        break;
                    }
                }
            }
            Ok(_) => {}
            Err(error) => {
                let Ok(task_id) = TaskId::new("invalid-command") else {
                    continue;
                };
                let event_task_id = task_id.clone();
                state
                    .task_events
                    .publish(&task_id, |sequence| AgentEvent::TaskFailed {
                        task_id: event_task_id,
                        sequence,
                        message: format!("invalid command: {error}"),
                    });
            }
        }
    }
    writer.abort();
    let _ = writer.await;
}

async fn run_codex(task_id: TaskId, prompt: String, workspace: &PathBuf, events: TaskEventStore) {
    let event_task_id = task_id.clone();
    events.publish(&task_id, |sequence| AgentEvent::TaskStarted {
        task_id: event_task_id,
        sequence,
    });

    let mut command = codex_command();
    let child = command
        .args([
            "exec",
            "--json",
            "--ephemeral",
            "--sandbox",
            "workspace-write",
            "--skip-git-repo-check",
            "-C",
        ])
        .arg(workspace)
        .arg(prompt)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn();

    let Ok(mut child) = child else {
        let error = child.expect_err("matched the failed process spawn");
        let event_task_id = task_id.clone();
        events.publish(&task_id, |sequence| AgentEvent::TaskFailed {
            task_id: event_task_id,
            sequence,
            message: format!("could not start Codex: {error}"),
        });
        return;
    };

    let Some(stdout) = child.stdout.take() else {
        return;
    };
    let Some(stderr) = child.stderr.take() else {
        return;
    };
    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();
    let stdout_task_id = task_id.clone();
    let stdout_events = events.clone();
    let stdout_task = tokio::spawn(async move {
        while let Ok(Some(line)) = stdout_lines.next_line().await {
            let event_task_id = stdout_task_id.clone();
            stdout_events.publish(&stdout_task_id, |sequence| AgentEvent::OutputDelta {
                task_id: event_task_id,
                sequence,
                text: line,
            });
        }
    });
    let stderr_task_id = task_id.clone();
    let stderr_events = events.clone();
    let stderr_task = tokio::spawn(async move {
        while let Ok(Some(line)) = stderr_lines.next_line().await {
            let event_task_id = stderr_task_id.clone();
            stderr_events.publish(&stderr_task_id, |sequence| AgentEvent::OutputDelta {
                task_id: event_task_id,
                sequence,
                text: format!("stderr: {line}"),
            });
        }
    });

    let status = child.wait().await;
    let _ = tokio::join!(stdout_task, stderr_task);
    match status {
        Ok(status) if status.success() => {
            let event_task_id = task_id.clone();
            events.publish(&task_id, |sequence| AgentEvent::TaskCompleted {
                task_id: event_task_id,
                sequence,
                summary: "Codex completed successfully".into(),
            });
        }
        Ok(status) => {
            let event_task_id = task_id.clone();
            events.publish(&task_id, |sequence| AgentEvent::TaskFailed {
                task_id: event_task_id,
                sequence,
                message: format!("Codex exited with {status}"),
            });
        }
        Err(error) => {
            let event_task_id = task_id.clone();
            events.publish(&task_id, |sequence| AgentEvent::TaskFailed {
                task_id: event_task_id,
                sequence,
                message: format!("failed to wait for Codex: {error}"),
            });
        }
    }
}

fn codex_command() -> Command {
    #[cfg(windows)]
    {
        let script = std::env::var_os("PATH")
            .into_iter()
            .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
            .map(|directory| directory.join("node_modules/@openai/codex/bin/codex.js"))
            .find(|candidate| candidate.is_file());
        if let Some(script) = script {
            let mut command = Command::new("node.exe");
            command.arg(script);
            return command;
        }
        Command::new("codex.exe")
    }

    #[cfg(not(windows))]
    Command::new("codex")
}

fn print_status() {
    println!("LiteCode agent");
    println!("version: {}", env!("CARGO_PKG_VERSION"));
    println!("protocol: {PROTOCOL_VERSION}");
    println!("platform: {}", std::env::consts::OS);
    println!("state: authenticated-loopback-ready");
}

fn print_help() {
    println!("Usage:");
    println!("  litecode-agent status");
    println!("  litecode-agent devices [--state-dir <PATH>]");
    println!("  litecode-agent revoke-device --device <ID> [--state-dir <PATH>]");
    println!(
        "  litecode-agent serve --workspace <PATH> [--bind <IP:PORT>] [--tls] [--advertise-host <HOST>] [--state-dir <PATH>]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, header},
    };
    use tower::ServiceExt;

    fn test_app() -> (Router, PathBuf) {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "litecode-endpoint-test-{}-{nonce}.json",
            std::process::id(),
        ));
        let _ = std::fs::remove_file(&path);
        let auth = auth::AuthService::load(path.clone()).expect("loads auth");
        let state = AppState {
            auth,
            workspace: Arc::new(std::env::current_dir().expect("current directory")),
            computer_name: Arc::from("Test computer"),
            endpoint: Arc::from("http://127.0.0.1:47831"),
            tls_enabled: false,
            fingerprint: None,
            task_events: TaskEventStore::default(),
        };
        (app_router(state), path)
    }

    fn request(method: &str, uri: &str, source: SocketAddr) -> Request<Body> {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .version(axum::http::Version::HTTP_11)
            .body(Body::empty())
            .expect("request");
        request.extensions_mut().insert(ConnectInfo(source));
        request
    }

    #[test]
    fn bearer_header_requires_the_expected_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer credential".parse().expect("valid header"),
        );
        assert_eq!(bearer_credential(&headers), Some("credential"));

        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Basic credential".parse().expect("valid header"),
        );
        assert_eq!(bearer_credential(&headers), None);
    }

    #[test]
    fn task_events_are_sequenced_and_replayed_after_a_cursor() {
        let store = TaskEventStore::default();
        let task_id = TaskId::new("task-replay").expect("valid task id");
        let first_task_id = task_id.clone();
        store.publish(&task_id, |sequence| AgentEvent::TaskStarted {
            task_id: first_task_id,
            sequence,
        });
        let second_task_id = task_id.clone();
        store.publish(&task_id, |sequence| AgentEvent::OutputDelta {
            task_id: second_task_id,
            sequence,
            text: "second".into(),
        });

        let replay = store.replay(&task_id, 1);

        assert_eq!(replay.len(), 1);
        assert_eq!(event_sequence(&replay[0]), 2);
    }

    #[tokio::test]
    async fn task_events_continue_without_a_connected_subscriber() {
        let store = TaskEventStore::default();
        let task_id = TaskId::new("task-disconnected").expect("valid task id");
        let event_task_id = task_id.clone();
        store.publish(&task_id, |sequence| AgentEvent::TaskCompleted {
            task_id: event_task_id,
            sequence,
            summary: "finished while disconnected".into(),
        });

        let replay = store.replay(&task_id, 0);

        assert_eq!(replay.len(), 1);
        assert_eq!(event_sequence(&replay[0]), 1);
    }

    #[tokio::test]
    async fn pairing_endpoints_cover_the_invitation_lifecycle() {
        let (app, store) = test_app();
        let source: SocketAddr = "127.0.0.1:40000".parse().expect("socket address");

        let response = app
            .clone()
            .oneshot(request("GET", "/v1/pairing-invitation/qr", source))
            .await
            .expect("QR response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("image/svg+xml")
        );

        let response = app
            .clone()
            .oneshot(request("POST", "/v1/pairing-invitation/cancel", source))
            .await
            .expect("cancel response");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let response = app
            .clone()
            .oneshot(request("GET", "/v1/pairing-invitation", source))
            .await
            .expect("status response");
        let body = to_bytes(response.into_body(), 4096)
            .await
            .expect("status body");
        let status: serde_json::Value = serde_json::from_slice(&body).expect("status JSON");
        assert_eq!(status["invitationStatus"], "cancelled");

        let response = app
            .clone()
            .oneshot(request("POST", "/v1/pairing-invitation/regenerate", source))
            .await
            .expect("regenerate response");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let response = app
            .oneshot(request("GET", "/v1/pairing-invitation", source))
            .await
            .expect("status response");
        let body = to_bytes(response.into_body(), 4096)
            .await
            .expect("status body");
        let status: serde_json::Value = serde_json::from_slice(&body).expect("status JSON");
        assert_eq!(status["invitationStatus"], "active");
        let _ = std::fs::remove_file(store);
    }

    #[tokio::test]
    async fn pairing_interface_is_rejected_for_non_loopback_sources() {
        let (app, store) = test_app();
        let source: SocketAddr = "192.0.2.10:40000".parse().expect("socket address");
        let response = app
            .oneshot(request("GET", "/pairing", source))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let _ = std::fs::remove_file(store);
    }
}
