use codex_orchestration::AgentId;
use codex_orchestration::BoundedText;
use codex_orchestration::TaskId;
use codex_orchestration::WorkflowId;
use serde::Deserialize;
use serde::Serialize;

use crate::RuntimeAgentId;

/// Provenance of a cancellation request, without executing it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationProvenance {
    User,
    WorkflowPolicy,
    ParentAgent,
    Recovery,
}

/// Request record for a future child cancellation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CancelChildRequest {
    workflow_id: WorkflowId,
    task_id: TaskId,
    agent_id: AgentId,
    runtime_id: Option<RuntimeAgentId>,
    reason: BoundedText,
    provenance: CancellationProvenance,
}

impl CancelChildRequest {
    pub fn new(
        workflow_id: WorkflowId,
        task_id: TaskId,
        agent_id: AgentId,
        runtime_id: Option<RuntimeAgentId>,
        reason: BoundedText,
        provenance: CancellationProvenance,
    ) -> Self {
        Self {
            workflow_id,
            task_id,
            agent_id,
            runtime_id,
            reason,
            provenance,
        }
    }

    pub fn workflow_id(&self) -> &WorkflowId {
        &self.workflow_id
    }
    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }
    pub fn runtime_id(&self) -> Option<&RuntimeAgentId> {
        self.runtime_id.as_ref()
    }
    pub fn reason(&self) -> &BoundedText {
        &self.reason
    }
    pub fn provenance(&self) -> CancellationProvenance {
        self.provenance
    }
}

/// Closed outcomes for a cancellation request, distinct from cancellation completion.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelChildOutcome {
    Requested,
    AlreadyTerminal,
    NotFound,
    Rejected,
    Unsupported,
}

/// Result record for a future cancellation request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CancelChildResult {
    workflow_id: WorkflowId,
    task_id: TaskId,
    agent_id: AgentId,
    outcome: CancelChildOutcome,
}

impl CancelChildResult {
    pub fn new(
        workflow_id: WorkflowId,
        task_id: TaskId,
        agent_id: AgentId,
        outcome: CancelChildOutcome,
    ) -> Self {
        Self {
            workflow_id,
            task_id,
            agent_id,
            outcome,
        }
    }

    pub fn workflow_id(&self) -> &WorkflowId {
        &self.workflow_id
    }
    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }
    pub fn outcome(&self) -> CancelChildOutcome {
        self.outcome
    }
}
