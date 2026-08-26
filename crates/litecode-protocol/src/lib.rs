//! Transport-independent messages exchanged by `LiteCode` clients and agents.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Version of the application-level protocol implemented by this crate.
pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairDeviceRequest {
    pub protocol_version: u16,
    pub pairing_secret: String,
    pub device_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairDeviceResponse {
    pub protocol_version: u16,
    pub agent_id: String,
    pub device_id: String,
    pub device_credential: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct TaskId(String);

impl TaskId {
    /// Creates a non-empty task identifier.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::EmptyIdentifier`] when the supplied value is blank.
    pub fn new(value: impl Into<String>) -> Result<Self, ProtocolError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ProtocolError::EmptyIdentifier);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientCommand {
    CreateTask {
        task_id: TaskId,
        workspace_id: String,
        tool: String,
        prompt: String,
    },
    SendInput {
        task_id: TaskId,
        input: String,
    },
    ResolveApproval {
        task_id: TaskId,
        approval_id: String,
        decision: ApprovalDecision,
    },
    StopTask {
        task_id: TaskId,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    ApproveOnce,
    Reject,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    TaskStarted {
        task_id: TaskId,
    },
    OutputDelta {
        task_id: TaskId,
        text: String,
    },
    ApprovalRequired {
        task_id: TaskId,
        approval_id: String,
        summary: String,
    },
    TaskCompleted {
        task_id: TaskId,
        summary: String,
    },
    TaskFailed {
        task_id: TaskId,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    EmptyIdentifier,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIdentifier => formatter.write_str("identifier cannot be empty"),
        }
    }
}

impl std::error::Error for ProtocolError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_id_rejects_blank_values() {
        assert_eq!(TaskId::new("  "), Err(ProtocolError::EmptyIdentifier));
    }

    #[test]
    fn create_task_uses_the_wire_contract() {
        let command = ClientCommand::CreateTask {
            task_id: TaskId::new("task-1").expect("valid task id"),
            workspace_id: "local".into(),
            tool: "codex".into(),
            prompt: "Check the project".into(),
        };

        let json = serde_json::to_string(&command).expect("serializes command");

        assert!(json.contains(r#""type":"create_task""#));
        assert!(json.contains(r#""task_id":"task-1""#));
    }

    #[test]
    fn pairing_request_uses_camel_case_fields() {
        let request = PairDeviceRequest {
            protocol_version: PROTOCOL_VERSION,
            pairing_secret: "one-time-secret".into(),
            device_name: "Test phone".into(),
        };

        let json = serde_json::to_string(&request).expect("serializes pairing request");

        assert!(json.contains(r#""protocolVersion":1"#));
        assert!(json.contains(r#""pairingSecret":"one-time-secret""#));
        assert!(json.contains(r#""deviceName":"Test phone""#));
    }
}
