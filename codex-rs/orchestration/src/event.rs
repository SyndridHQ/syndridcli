use serde::Deserialize;
use serde::Serialize;

use crate::AgentId;
use crate::DataQuality;
use crate::RunLifecycleState;
use crate::TaskId;
use crate::VerificationState;
use crate::WorkflowId;
use crate::WorkflowStage;

/// Stable numeric reference for an orchestration event; this crate does not generate it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct EventReference(pub u64);

/// Exhaustive data-only kinds for the future append-only workflow log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum WorkflowEventKind {
    WorkflowCreated,
    LifecycleChanged { state: RunLifecycleState },
    StageChanged { stage: WorkflowStage },
    AgentAssigned,
    AgentReleased,
    WorkClaimed,
    BudgetObserved { data_quality: DataQuality },
    RecommendationRecorded,
    HandoffRecorded,
    VerificationRecorded { state: VerificationState },
    CancellationRequested,
    CancellationAcknowledged,
    WorkflowCompleted,
    WorkflowFailed,
}

/// Correlated event envelope; storage, replay, and projections belong to O2.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowEvent {
    pub workflow_id: WorkflowId,
    pub task_id: Option<TaskId>,
    pub agent_id: Option<AgentId>,
    pub sequence: u64,
    pub causation: Option<EventReference>,
    pub correlation: Option<EventReference>,
    pub kind: WorkflowEventKind,
}
