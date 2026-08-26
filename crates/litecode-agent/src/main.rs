mod auth;

use std::{net::SocketAddr, path::PathBuf, process::ExitCode, sync::Arc};

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
            let state_dir = optional_flag(&arguments, "--state-dir")
                .map_or_else(default_state_dir, PathBuf::from);
            serve(PathBuf::from(workspace), bind, state_dir).await
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

fn default_state_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_STATE_HOME"))
        .map_or_else(|| PathBuf::from(".litecode"), PathBuf::from)
        .join("LiteCode")
}

async fn serve(workspace: PathBuf, bind: &str, state_dir: PathBuf) -> Result<(), String> {
    let workspace = workspace
        .canonicalize()
        .map_err(|error| format!("invalid workspace {}: {error}", workspace.display()))?;
    let address: SocketAddr = bind
        .parse()
        .map_err(|error| format!("invalid bind address {bind}: {error}"))?;
    if !address.ip().is_loopback() {
        return Err(
            "LAN binding requires TLS and is not implemented; use a loopback address".into(),
        );
    }

    let auth = auth::AuthService::load(state_dir.join("devices.json"))?;

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/pair", axum::routing::post(pair_device))
        .route("/v1/ws", get(websocket_upgrade))
        .with_state(AppState {
            auth: auth.clone(),
            workspace: Arc::new(workspace.clone()),
        });
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| format!("cannot bind {address}: {error}"))?;
    println!("LiteCode agent listening on ws://{address}/v1/ws");
    println!("Authorized workspace: {}", workspace.display());
    println!(
        "Pairing invitation (valid once for 5 minutes): {}",
        auth.invitation_uri(&format!("http://{address}"))
    );
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(|error| format!("server failed: {error}"))
}

async fn health() -> &'static str {
    "ok"
}

async fn websocket_upgrade(
    upgrade: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    let credential = bearer_credential(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !state.auth.authenticate(credential) {
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
    let (event_sender, mut event_receiver) = mpsc::unbounded_channel::<AgentEvent>();

    let writer = tokio::spawn(async move {
        while let Some(event) = event_receiver.recv().await {
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
                let events = event_sender.clone();
                tokio::spawn(async move {
                    run_codex(task_id, prompt, workspace.as_ref(), &events).await;
                });
            }
            Ok(ClientCommand::CreateTask { task_id, .. }) => send_event(
                &event_sender,
                AgentEvent::TaskFailed {
                    task_id,
                    message: "unsupported workspace, tool, or empty prompt".into(),
                },
            ),
            Ok(_) => {}
            Err(error) => {
                let Ok(task_id) = TaskId::new("invalid-command") else {
                    continue;
                };
                send_event(
                    &event_sender,
                    AgentEvent::TaskFailed {
                        task_id,
                        message: format!("invalid command: {error}"),
                    },
                );
            }
        }
    }

    drop(event_sender);
    let _ = writer.await;
}

async fn run_codex(
    task_id: TaskId,
    prompt: String,
    workspace: &PathBuf,
    events: &mpsc::UnboundedSender<AgentEvent>,
) {
    send_event(
        events,
        AgentEvent::TaskStarted {
            task_id: task_id.clone(),
        },
    );

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
        send_event(
            events,
            AgentEvent::TaskFailed {
                task_id,
                message: format!("could not start Codex: {error}"),
            },
        );
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
            send_event(
                &stdout_events,
                AgentEvent::OutputDelta {
                    task_id: stdout_task_id.clone(),
                    text: line,
                },
            );
        }
    });
    let stderr_task_id = task_id.clone();
    let stderr_events = events.clone();
    let stderr_task = tokio::spawn(async move {
        while let Ok(Some(line)) = stderr_lines.next_line().await {
            send_event(
                &stderr_events,
                AgentEvent::OutputDelta {
                    task_id: stderr_task_id.clone(),
                    text: format!("stderr: {line}"),
                },
            );
        }
    });

    let status = child.wait().await;
    let _ = tokio::join!(stdout_task, stderr_task);
    match status {
        Ok(status) if status.success() => send_event(
            events,
            AgentEvent::TaskCompleted {
                task_id,
                summary: "Codex completed successfully".into(),
            },
        ),
        Ok(status) => send_event(
            events,
            AgentEvent::TaskFailed {
                task_id,
                message: format!("Codex exited with {status}"),
            },
        ),
        Err(error) => send_event(
            events,
            AgentEvent::TaskFailed {
                task_id,
                message: format!("failed to wait for Codex: {error}"),
            },
        ),
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

fn send_event(sender: &mpsc::UnboundedSender<AgentEvent>, event: AgentEvent) {
    let _ = sender.send(event);
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
    println!(
        "  litecode-agent serve --workspace <PATH> [--bind 127.0.0.1:47831] [--state-dir <PATH>]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
