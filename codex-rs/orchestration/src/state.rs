use serde::Deserialize;
use serde::Serialize;

use crate::AgentAssignment;
use crate::OrchestrationMode;
use crate::WorkflowBudget;
use crate::WorkflowId;

/// The kind of work a workflow stage describes, without its outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStage {
    Planning,
    Exploring,
    Executing,
    Verifying,
    Repairing,
}

/// Lifecycle of a workflow, kept independent from stage and verification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunLifecycleState {
    Created,
    Ready,
    Running,
    Waiting,
    Succeeded,
    Failed,
}

/// Why a workflow is waiting for an external or bounded internal condition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitReason {
    UserConfirmation,
    Approval,
    AgentCompletion,
    ToolCompletion,
    BudgetDecision,
    RetryDelay,
    ExternalDependency,
}

/// Cancellation progress, independent from lifecycle state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationState {
    NotRequested,
    Requested,
    Cancelling,
    Cancelled,
}

/// Verification result state based on observed evidence, not an agent claim.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    NotRequired,
    Pending,
    Running,
    Passed,
    Failed,
    Blocked,
    Inconclusive,
}

/// Quality of a value or observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataQuality {
    Exact,
    Derived,
    Estimated,
    Unavailable,
}

/// Data-only workflow identity and independent state dimensions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub workflow_id: WorkflowId,
    pub mode: OrchestrationMode,
    pub lifecycle: RunLifecycleState,
    pub stage: WorkflowStage,
    pub wait_reason: Option<WaitReason>,
    pub cancellation: CancellationState,
    pub verification: VerificationState,
    pub max_concurrency: u16,
    pub max_writers: u16,
    pub assignments: Vec<AgentAssignment>,
    pub budget: Option<WorkflowBudget>,
}

impl WorkflowRun {
    /// The initial policy ceilings required by Syndrid's orchestration design.
    pub const INITIAL_MAX_CONCURRENCY: u16 = 2;
    /// The initial worktree writer ceiling.
    pub const INITIAL_MAX_WRITERS: u16 = 1;
}
