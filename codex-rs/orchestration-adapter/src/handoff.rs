use codex_orchestration::AgentId;
use codex_orchestration::StructuredHandoff;
use codex_orchestration::TaskId;
use codex_orchestration::WorkflowId;
use serde::Deserialize;
use serde::Serialize;

use crate::RuntimeAgentId;

/// Request to deliver an existing bounded handoff to an existing child.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeliverHandoffRequest {
    pub workflow_id: WorkflowId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub runtime_id: RuntimeAgentId,
    pub handoff: StructuredHandoff,
}

/// Closed outcomes for handoff delivery; none claims that delivery was executed by O2A.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffDeliveryOutcome {
    Accepted,
    Rejected,
    TargetUnavailable,
    TargetCompleted,
}

/// Result record for a future handoff delivery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeliverHandoffResult {
    pub workflow_id: WorkflowId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub runtime_id: RuntimeAgentId,
    pub outcome: HandoffDeliveryOutcome,
}
