use codex_orchestration::AgentId;
use codex_orchestration::BoundedText;
use codex_orchestration::DataQuality;
use codex_orchestration::TaskId;
use codex_orchestration::WorkflowId;
use serde::Deserialize;
use serde::Serialize;

/// Closed categories for failures at the future runtime boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterErrorKind {
    InvalidRequest,
    Unsupported,
    PermissionDenied,
    CapacityUnavailable,
    RuntimeUnavailable,
    ChildNotFound,
    ChildAlreadyTerminal,
    Conflict,
    InternalAdapterFailure,
}

/// Whether a future adapter may consider an error retryable; O2B performs no retry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retryability {
    Retryable,
    NotRetryable,
    Unknown,
}

/// Bounded, safe error information returned by a future adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterError {
    kind: AdapterErrorKind,
    message: BoundedText,
    retryability: Retryability,
    data_quality: DataQuality,
    workflow_id: Option<WorkflowId>,
    task_id: Option<TaskId>,
    agent_id: Option<AgentId>,
}

impl AdapterError {
    pub fn new(
        kind: AdapterErrorKind,
        message: BoundedText,
        retryability: Retryability,
        data_quality: DataQuality,
    ) -> Self {
        Self {
            kind,
            message,
            retryability,
            data_quality,
            workflow_id: None,
            task_id: None,
            agent_id: None,
        }
    }

    pub fn with_attribution(
        mut self,
        workflow_id: Option<WorkflowId>,
        task_id: Option<TaskId>,
        agent_id: Option<AgentId>,
    ) -> Self {
        self.workflow_id = workflow_id;
        self.task_id = task_id;
        self.agent_id = agent_id;
        self
    }

    pub fn kind(&self) -> AdapterErrorKind {
        self.kind
    }
    pub fn message(&self) -> &BoundedText {
        &self.message
    }
    pub fn retryability(&self) -> Retryability {
        self.retryability
    }
    pub fn data_quality(&self) -> DataQuality {
        self.data_quality
    }
    pub fn workflow_id(&self) -> Option<&WorkflowId> {
        self.workflow_id.as_ref()
    }
    pub fn task_id(&self) -> Option<&TaskId> {
        self.task_id.as_ref()
    }
    pub fn agent_id(&self) -> Option<&AgentId> {
        self.agent_id.as_ref()
    }
}
