//! Core task state and workspace authorization rules for `LiteCode` agents.

use litecode_protocol::TaskId;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizedWorkspace {
    id: String,
    root: PathBuf,
}

impl AuthorizedWorkspace {
    /// Creates a host-authorized workspace rooted at an absolute path.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier is blank or the path is not absolute.
    pub fn new(id: impl Into<String>, root: impl Into<PathBuf>) -> Result<Self, CoreError> {
        let id = id.into();
        let root = root.into();
        if id.trim().is_empty() {
            return Err(CoreError::InvalidWorkspaceId);
        }
        if !root.is_absolute() {
            return Err(CoreError::WorkspacePathMustBeAbsolute);
        }
        Ok(Self { id, root })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskState {
    Pending,
    Running,
    WaitingForApproval,
    Completed,
    Failed,
    Stopped,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Task {
    id: TaskId,
    state: TaskState,
}

impl Task {
    #[must_use]
    pub fn new(id: TaskId) -> Self {
        Self {
            id,
            state: TaskState::Pending,
        }
    }

    #[must_use]
    pub fn id(&self) -> &TaskId {
        &self.id
    }

    #[must_use]
    pub fn state(&self) -> TaskState {
        self.state
    }

    /// Moves the task to a valid next lifecycle state.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidTaskTransition`] for a disallowed transition.
    pub fn transition_to(&mut self, next: TaskState) -> Result<(), CoreError> {
        let allowed = match self.state {
            TaskState::Pending => matches!(next, TaskState::Running | TaskState::Stopped),
            TaskState::Running => matches!(
                next,
                TaskState::WaitingForApproval
                    | TaskState::Completed
                    | TaskState::Failed
                    | TaskState::Stopped
            ),
            TaskState::WaitingForApproval => {
                matches!(
                    next,
                    TaskState::Running | TaskState::Failed | TaskState::Stopped
                )
            }
            TaskState::Completed | TaskState::Failed | TaskState::Stopped => false,
        };
        if !allowed {
            return Err(CoreError::InvalidTaskTransition {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoreError {
    InvalidWorkspaceId,
    WorkspacePathMustBeAbsolute,
    InvalidTaskTransition { from: TaskState, to: TaskState },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_task_cannot_restart() {
        let mut task = Task::new(TaskId::new("task-1").expect("valid task id"));
        task.transition_to(TaskState::Running).expect("can start");
        task.transition_to(TaskState::Completed)
            .expect("can finish");

        assert!(task.transition_to(TaskState::Running).is_err());
    }
}
