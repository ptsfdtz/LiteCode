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
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
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
    tasks: TaskSupervisor,
}

#[derive(Clone, Default)]
struct TaskSupervisor {
    inner: Arc<Mutex<HashMap<String, SupervisedTask>>>,
}

enum SupervisedTask {
    Active(mpsc::UnboundedSender<TaskControl>),
    Stopping,
    Finished,
}

enum TaskControl {
    SendInput(String),
    Stop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StopTaskResult {
    Requested,
    AlreadyRequested,
    AlreadyFinished,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SendInputResult {
    Sent,
    NotRunning,
    Unknown,
}

impl TaskSupervisor {
    fn register(&self, task_id: &TaskId) -> Option<mpsc::UnboundedReceiver<TaskControl>> {
        let mut tasks = self.inner.lock().expect("task supervisor mutex poisoned");
        if tasks.contains_key(task_id.as_str()) {
            return None;
        }
        let (sender, receiver) = mpsc::unbounded_channel();
        tasks.insert(task_id.as_str().to_owned(), SupervisedTask::Active(sender));
        Some(receiver)
    }

    fn stop(&self, task_id: &TaskId) -> StopTaskResult {
        let mut tasks = self.inner.lock().expect("task supervisor mutex poisoned");
        let Some(task) = tasks.get_mut(task_id.as_str()) else {
            return StopTaskResult::Unknown;
        };
        match task {
            SupervisedTask::Active(sender) => {
                let _ = sender.send(TaskControl::Stop);
                *task = SupervisedTask::Stopping;
                StopTaskResult::Requested
            }
            SupervisedTask::Stopping => StopTaskResult::AlreadyRequested,
            SupervisedTask::Finished => StopTaskResult::AlreadyFinished,
        }
    }

    fn send_input(&self, task_id: &TaskId, input: String) -> SendInputResult {
        let tasks = self.inner.lock().expect("task supervisor mutex poisoned");
        let Some(task) = tasks.get(task_id.as_str()) else {
            return SendInputResult::Unknown;
        };
        match task {
            SupervisedTask::Active(sender) => {
                if sender.send(TaskControl::SendInput(input)).is_ok() {
                    SendInputResult::Sent
                } else {
                    SendInputResult::NotRunning
                }
            }
            SupervisedTask::Stopping | SupervisedTask::Finished => SendInputResult::NotRunning,
        }
    }

    fn finish(&self, task_id: &TaskId) {
        if let Some(task) = self
            .inner
            .lock()
            .expect("task supervisor mutex poisoned")
            .get_mut(task_id.as_str())
        {
            *task = SupervisedTask::Finished;
        }
    }
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
        | AgentEvent::TaskStopped { sequence, .. }
        | AgentEvent::TaskFailed { sequence, .. } => *sequence,
    }
}

enum AppServerAction {
    Send(Vec<serde_json::Value>),
    Terminal(AppServerTerminal),
    None,
}

enum AppServerTerminal {
    Completed,
    Stopped,
    Failed(String),
}

struct AppServerSession {
    prompt: Option<String>,
    thread_id: Option<String>,
    turn_id: Option<String>,
    queued_inputs: Vec<String>,
    next_request_id: u64,
    interrupt_requested: bool,
}

impl AppServerSession {
    fn new(prompt: String) -> Self {
        Self {
            prompt: Some(prompt),
            thread_id: None,
            turn_id: None,
            queued_inputs: Vec::new(),
            next_request_id: 3,
            interrupt_requested: false,
        }
    }

    fn handle_message(
        &mut self,
        line: &str,
        task_id: &TaskId,
        events: &TaskEventStore,
    ) -> AppServerAction {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
            return AppServerAction::None;
        };
        if let Some(error) = message.get("error") {
            let detail = error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown app-server error");
            return AppServerAction::Terminal(AppServerTerminal::Failed(format!(
                "Codex protocol error: {detail}"
            )));
        }
        if message.get("id").and_then(serde_json::Value::as_u64) == Some(1) {
            return self.handle_thread_started_response(&message);
        }
        let method = message
            .get("method")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        match method {
            "turn/started" => self.handle_turn_started(&message),
            "turn/completed" => Self::handle_turn_completed(&message),
            "item/agentMessage/delta" => {
                if let Some(delta) = message.pointer("/params/delta").and_then(|v| v.as_str()) {
                    publish_output(task_id, events, delta.to_owned());
                }
                AppServerAction::None
            }
            "item/started" | "item/completed" => {
                if let Some(item) = message.pointer("/params/item") {
                    let output = serde_json::json!({"type": method, "item": item});
                    publish_output(task_id, events, output.to_string());
                }
                AppServerAction::None
            }
            _ => AppServerAction::None,
        }
    }

    fn handle_thread_started_response(&mut self, message: &serde_json::Value) -> AppServerAction {
        let Some(thread_id) = message
            .pointer("/result/thread/id")
            .and_then(serde_json::Value::as_str)
        else {
            return AppServerAction::Terminal(AppServerTerminal::Failed(
                "Codex did not return a thread ID".into(),
            ));
        };
        self.thread_id = Some(thread_id.to_owned());
        let prompt = self.prompt.take().unwrap_or_default();
        AppServerAction::Send(vec![serde_json::json!({
            "method": "turn/start",
            "id": 2,
            "params": {
                "threadId": thread_id,
                "input": [{"type": "text", "text": prompt}]
            }
        })])
    }

    fn handle_turn_started(&mut self, message: &serde_json::Value) -> AppServerAction {
        let Some(turn_id) = message
            .pointer("/params/turn/id")
            .and_then(serde_json::Value::as_str)
        else {
            return AppServerAction::None;
        };
        self.turn_id = Some(turn_id.to_owned());
        if self.interrupt_requested {
            return self
                .interrupt_message()
                .map_or(AppServerAction::None, |message| {
                    AppServerAction::Send(vec![message])
                });
        }
        let queued = std::mem::take(&mut self.queued_inputs);
        AppServerAction::Send(
            queued
                .into_iter()
                .filter_map(|input| self.steer_message(&input))
                .collect(),
        )
    }

    fn handle_turn_completed(message: &serde_json::Value) -> AppServerAction {
        match message
            .pointer("/params/turn/status")
            .and_then(serde_json::Value::as_str)
        {
            Some("completed") => AppServerAction::Terminal(AppServerTerminal::Completed),
            Some("interrupted") => AppServerAction::Terminal(AppServerTerminal::Stopped),
            Some("failed") => {
                AppServerAction::Terminal(AppServerTerminal::Failed("Codex turn failed".into()))
            }
            _ => AppServerAction::None,
        }
    }

    fn steer(&mut self, input: String) -> Option<serde_json::Value> {
        if self.turn_id.is_none() {
            self.queued_inputs.push(input);
            return None;
        }
        self.steer_message(&input)
    }

    fn steer_message(&mut self, input: &str) -> Option<serde_json::Value> {
        let thread_id = self.thread_id.as_ref()?;
        let turn_id = self.turn_id.as_ref()?;
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        Some(serde_json::json!({
            "method": "turn/steer",
            "id": request_id,
            "params": {
                "threadId": thread_id,
                "expectedTurnId": turn_id,
                "input": [{"type": "text", "text": input}]
            }
        }))
    }

    fn interrupt(&mut self) -> Option<serde_json::Value> {
        self.interrupt_requested = true;
        self.interrupt_message()
    }

    fn interrupt_message(&mut self) -> Option<serde_json::Value> {
        let thread_id = self.thread_id.as_ref()?;
        let turn_id = self.turn_id.as_ref()?;
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        Some(serde_json::json!({
            "method": "turn/interrupt",
            "id": request_id,
            "params": {"threadId": thread_id, "turnId": turn_id}
        }))
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
            tasks: TaskSupervisor::default(),
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
            tasks: TaskSupervisor::default(),
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
                if let Some(controls) = state.tasks.register(&task_id) {
                    let workspace = Arc::clone(&state.workspace);
                    let events = state.task_events.clone();
                    let tasks = state.tasks.clone();
                    tokio::spawn(async move {
                        run_codex(task_id, prompt, workspace.as_ref(), events, tasks, controls)
                            .await;
                    });
                } else {
                    let event_task_id = task_id.clone();
                    state
                        .task_events
                        .publish(&task_id, |sequence| AgentEvent::TaskFailed {
                            task_id: event_task_id,
                            sequence,
                            message: "task ID has already been used".into(),
                        });
                }
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
            Ok(ClientCommand::StopTask { task_id }) => {
                let _ = state.tasks.stop(&task_id);
            }
            Ok(ClientCommand::SendInput { task_id, input }) if !input.trim().is_empty() => {
                let _ = state.tasks.send_input(&task_id, input);
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

async fn run_codex(
    task_id: TaskId,
    prompt: String,
    workspace: &Path,
    events: TaskEventStore,
    tasks: TaskSupervisor,
    controls: mpsc::UnboundedReceiver<TaskControl>,
) {
    let event_task_id = task_id.clone();
    events.publish(&task_id, |sequence| AgentEvent::TaskStarted {
        task_id: event_task_id,
        sequence,
    });

    let mut command = codex_command();
    let child = command
        .args(["app-server", "--stdio"])
        .stdin(std::process::Stdio::piped())
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
        tasks.finish(&task_id);
        return;
    };

    let Some(mut stdin) = child.stdin.take() else {
        publish_task_failure(&task_id, &events, "Codex stdin is unavailable".into());
        tasks.finish(&task_id);
        return;
    };
    let Some(stdout) = child.stdout.take() else {
        publish_task_failure(&task_id, &events, "Codex stdout is unavailable".into());
        tasks.finish(&task_id);
        return;
    };
    let Some(stderr) = child.stderr.take() else {
        publish_task_failure(&task_id, &events, "Codex stderr is unavailable".into());
        tasks.finish(&task_id);
        return;
    };
    let workspace = workspace.to_string_lossy();
    let initialize = serde_json::json!({
        "method": "initialize",
        "id": 0,
        "params": {"clientInfo": {
            "name": "litecode",
            "title": "LiteCode",
            "version": env!("CARGO_PKG_VERSION")
        }}
    });
    let initialized = serde_json::json!({"method": "initialized", "params": {}});
    let start_thread = serde_json::json!({
        "method": "thread/start",
        "id": 1,
        "params": {
            "cwd": workspace,
            "approvalPolicy": "never",
            "sandbox": "workspace-write",
            "ephemeral": true,
            "serviceName": "litecode"
        }
    });
    if send_rpc(&mut stdin, &initialize).await.is_err()
        || send_rpc(&mut stdin, &initialized).await.is_err()
        || send_rpc(&mut stdin, &start_thread).await.is_err()
    {
        publish_task_failure(&task_id, &events, "could not initialize Codex".into());
        let _ = child.kill().await;
        tasks.finish(&task_id);
        return;
    }

    let terminal = drive_app_server(
        child,
        stdin,
        stdout,
        stderr,
        controls,
        AppServerSession::new(prompt),
        &task_id,
        &events,
    )
    .await;
    publish_app_server_terminal(&task_id, &events, terminal);
    tasks.finish(&task_id);
}

#[allow(clippy::too_many_arguments)]
async fn drive_app_server(
    mut child: tokio::process::Child,
    mut stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    mut controls: mpsc::UnboundedReceiver<TaskControl>,
    mut session: AppServerSession,
    task_id: &TaskId,
    events: &TaskEventStore,
) -> AppServerTerminal {
    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();
    let terminal = loop {
        tokio::select! {
            line = stdout_lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        match session.handle_message(&line, task_id, events) {
                            AppServerAction::Send(messages) => {
                                if send_rpc_messages(&mut stdin, messages).await.is_err() {
                                    break AppServerTerminal::Failed("could not send input to Codex".into());
                                }
                            }
                            AppServerAction::Terminal(terminal) => break terminal,
                            AppServerAction::None => {}
                        }
                    }
                    Ok(None) => break AppServerTerminal::Failed("Codex closed unexpectedly".into()),
                    Err(error) => break AppServerTerminal::Failed(format!("could not read Codex output: {error}")),
                }
            }
            line = stderr_lines.next_line() => {
                if let Ok(Some(line)) = line {
                    publish_output(task_id, events, format!("stderr: {line}"));
                }
            }
            control = controls.recv() => {
                match control {
                    Some(TaskControl::SendInput(input)) => {
                        if let Some(message) = session.steer(input) {
                            if send_rpc(&mut stdin, &message).await.is_err() {
                                break AppServerTerminal::Failed("could not send follow-up input".into());
                            }
                        }
                    }
                    Some(TaskControl::Stop) => {
                        if let Some(message) = session.interrupt() {
                            if send_rpc(&mut stdin, &message).await.is_err() {
                                break AppServerTerminal::Stopped;
                            }
                        } else {
                            break AppServerTerminal::Stopped;
                        }
                    }
                    None => break AppServerTerminal::Failed("task control channel closed".into()),
                }
            }
            status = child.wait() => {
                break match status {
                    Ok(status) => AppServerTerminal::Failed(format!("Codex exited with {status}")),
                    Err(error) => AppServerTerminal::Failed(format!("failed to wait for Codex: {error}")),
                };
            }
        }
    };
    let _ = child.kill().await;
    terminal
}

async fn send_rpc(
    stdin: &mut tokio::process::ChildStdin,
    message: &serde_json::Value,
) -> std::io::Result<()> {
    stdin.write_all(message.to_string().as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await
}

async fn send_rpc_messages(
    stdin: &mut tokio::process::ChildStdin,
    messages: Vec<serde_json::Value>,
) -> std::io::Result<()> {
    for message in messages {
        send_rpc(stdin, &message).await?;
    }
    Ok(())
}

fn publish_app_server_terminal(
    task_id: &TaskId,
    events: &TaskEventStore,
    terminal: AppServerTerminal,
) {
    let event_task_id = task_id.clone();
    match terminal {
        AppServerTerminal::Completed => {
            events.publish(task_id, |sequence| AgentEvent::TaskCompleted {
                task_id: event_task_id,
                sequence,
                summary: "Codex completed successfully".into(),
            });
        }
        AppServerTerminal::Stopped => {
            events.publish(task_id, |sequence| AgentEvent::TaskStopped {
                task_id: event_task_id,
                sequence,
            });
        }
        AppServerTerminal::Failed(message) => {
            events.publish(task_id, |sequence| AgentEvent::TaskFailed {
                task_id: event_task_id,
                sequence,
                message,
            });
        }
    }
}

fn publish_task_failure(task_id: &TaskId, events: &TaskEventStore, message: String) {
    let event_task_id = task_id.clone();
    events.publish(task_id, |sequence| AgentEvent::TaskFailed {
        task_id: event_task_id,
        sequence,
        message,
    });
}

fn publish_output(task_id: &TaskId, events: &TaskEventStore, text: String) {
    let event_task_id = task_id.clone();
    events.publish(task_id, |sequence| AgentEvent::OutputDelta {
        task_id: event_task_id,
        sequence,
        text,
    });
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
            tasks: TaskSupervisor::default(),
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
    async fn cancelling_a_running_task_signals_only_that_task() {
        let tasks = TaskSupervisor::default();
        let first = TaskId::new("task-first").expect("valid task id");
        let second = TaskId::new("task-second").expect("valid task id");
        let mut first_controls = tasks.register(&first).expect("registers first task");
        let mut second_controls = tasks.register(&second).expect("registers second task");

        assert_eq!(tasks.stop(&first), StopTaskResult::Requested);
        assert!(matches!(
            first_controls.recv().await,
            Some(TaskControl::Stop)
        ));
        assert!(matches!(
            second_controls.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn cancellation_is_safe_for_unknown_repeated_and_finished_tasks() {
        let tasks = TaskSupervisor::default();
        let unknown = TaskId::new("task-unknown").expect("valid task id");
        assert_eq!(tasks.stop(&unknown), StopTaskResult::Unknown);

        let running = TaskId::new("task-running").expect("valid task id");
        let _cancellation = tasks.register(&running).expect("registers task");
        assert_eq!(tasks.stop(&running), StopTaskResult::Requested);
        assert_eq!(tasks.stop(&running), StopTaskResult::AlreadyRequested);

        let completed = TaskId::new("task-completed").expect("valid task id");
        let _cancellation = tasks.register(&completed).expect("registers task");
        tasks.finish(&completed);
        assert_eq!(tasks.stop(&completed), StopTaskResult::AlreadyFinished);
    }

    #[test]
    fn cancellation_terminal_event_is_sequenced_and_replayable() {
        let store = TaskEventStore::default();
        let task_id = TaskId::new("task-stopped").expect("valid task id");
        let started_task_id = task_id.clone();
        store.publish(&task_id, |sequence| AgentEvent::TaskStarted {
            task_id: started_task_id,
            sequence,
        });
        let stopped_task_id = task_id.clone();
        store.publish(&task_id, |sequence| AgentEvent::TaskStopped {
            task_id: stopped_task_id,
            sequence,
        });

        let replay = store.replay(&task_id, 1);
        assert_eq!(replay.len(), 1);
        assert!(matches!(
            replay[0],
            AgentEvent::TaskStopped { sequence: 2, .. }
        ));
    }

    #[test]
    fn follow_up_input_is_routed_only_to_the_running_task() {
        let tasks = TaskSupervisor::default();
        let first = TaskId::new("task-first").expect("valid task id");
        let second = TaskId::new("task-second").expect("valid task id");
        let mut first_controls = tasks.register(&first).expect("registers first task");
        let mut second_controls = tasks.register(&second).expect("registers second task");

        assert_eq!(
            tasks.send_input(&second, "focus on tests".into()),
            SendInputResult::Sent
        );
        assert!(matches!(
            second_controls.try_recv(),
            Ok(TaskControl::SendInput(input)) if input == "focus on tests"
        ));
        assert!(matches!(
            first_controls.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn follow_up_input_is_rejected_for_unknown_stopping_and_finished_tasks() {
        let tasks = TaskSupervisor::default();
        let unknown = TaskId::new("task-unknown").expect("valid task id");
        assert_eq!(
            tasks.send_input(&unknown, "input".into()),
            SendInputResult::Unknown
        );

        let task_id = TaskId::new("task-known").expect("valid task id");
        let _controls = tasks.register(&task_id).expect("registers task");
        assert_eq!(tasks.stop(&task_id), StopTaskResult::Requested);
        assert_eq!(
            tasks.send_input(&task_id, "too late".into()),
            SendInputResult::NotRunning
        );
        tasks.finish(&task_id);
        assert_eq!(
            tasks.send_input(&task_id, "still too late".into()),
            SendInputResult::NotRunning
        );
    }

    #[test]
    fn app_server_session_maps_thread_start_and_follow_up_input() {
        let store = TaskEventStore::default();
        let task_id = TaskId::new("task-app-server").expect("valid task id");
        let mut session = AppServerSession::new("initial prompt".into());
        let thread_response = r#"{"id":1,"result":{"thread":{"id":"thread-1"}}}"#;
        let AppServerAction::Send(messages) =
            session.handle_message(thread_response, &task_id, &store)
        else {
            panic!("thread response should start a turn");
        };
        assert_eq!(messages[0]["method"], "turn/start");
        assert_eq!(messages[0]["params"]["input"][0]["text"], "initial prompt");

        let turn_started = r#"{"method":"turn/started","params":{"turn":{"id":"turn-1"}}}"#;
        let _ = session.handle_message(turn_started, &task_id, &store);
        let steer = session.steer("follow up".into()).expect("steer request");
        assert_eq!(steer["method"], "turn/steer");
        assert_eq!(steer["params"]["expectedTurnId"], "turn-1");
        assert_eq!(steer["params"]["input"][0]["text"], "follow up");
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
