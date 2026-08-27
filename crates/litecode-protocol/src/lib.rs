//! Transport-independent messages exchanged by `LiteCode` clients and agents.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Version of the application-level protocol implemented by this crate.
pub const PROTOCOL_VERSION: u16 = 2;

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
    ResumeEvents {
        task_id: TaskId,
        after_sequence: u64,
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
        sequence: u64,
    },
    OutputDelta {
        task_id: TaskId,
        sequence: u64,
        text: String,
    },
    ApprovalRequired {
        task_id: TaskId,
        sequence: u64,
        approval_id: String,
        summary: String,
    },
    TaskCompleted {
        task_id: TaskId,
        sequence: u64,
        summary: String,
    },
    TaskStopped {
        task_id: TaskId,
        sequence: u64,
    },
    TaskFailed {
        task_id: TaskId,
        sequence: u64,
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

        assert!(json.contains(r#""protocolVersion":2"#));
        assert!(json.contains(r#""pairingSecret":"one-time-secret""#));
        assert!(json.contains(r#""deviceName":"Test phone""#));
    }

    #[test]
    fn resume_events_uses_the_wire_contract() {
        let command = ClientCommand::ResumeEvents {
            task_id: TaskId::new("task-1").expect("valid task id"),
            after_sequence: 7,
        };

        let json = serde_json::to_string(&command).expect("serializes resume command");

        assert_eq!(
            json,
            r#"{"type":"resume_events","task_id":"task-1","after_sequence":7}"#
        );
    }

    #[test]
    fn send_input_uses_the_wire_contract() {
        let command = ClientCommand::SendInput {
            task_id: TaskId::new("task-1").expect("valid task id"),
            input: "Focus on tests".into(),
        };

        assert_eq!(
            serde_json::to_string(&command).expect("serializes command"),
            r#"{"type":"send_input","task_id":"task-1","input":"Focus on tests"}"#
        );
    }

    #[test]
    fn task_events_include_their_sequence() {
        let event = AgentEvent::TaskStarted {
            task_id: TaskId::new("task-1").expect("valid task id"),
            sequence: 1,
        };

        let json = serde_json::to_string(&event).expect("serializes event");

        assert!(json.contains(r#""sequence":1"#));
    }

    #[test]
    fn task_stopped_uses_the_wire_contract() {
        let event = AgentEvent::TaskStopped {
            task_id: TaskId::new("task-1").expect("valid task id"),
            sequence: 4,
        };

        assert_eq!(
            serde_json::to_string(&event).expect("serializes event"),
            r#"{"type":"task_stopped","task_id":"task-1","sequence":4}"#
        );
    }
}
