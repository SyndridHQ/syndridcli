use codex_orchestration::AgentId;
use codex_orchestration::AgentRole;
use codex_orchestration::EffortRoute;
use codex_orchestration::ModelRoute;
use codex_orchestration::PermissionEnvelope;
use codex_orchestration::StructuredHandoff;
use codex_orchestration::TaskId;
use codex_orchestration::WorkAccess;
use codex_orchestration::WorkflowId;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::RuntimeAgentId;

/// Failure returned when a spawn request violates the control-plane boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SpawnRequestError {
    #[error("a child agent cannot be its own parent")]
    SelfParent,
    #[error("read-only access cannot carry writer assignment permission")]
    AccessPermissionConflict,
}

/// Already-selected intent for a future native child-thread adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SpawnChildRequestWire")]
pub struct SpawnChildRequest {
    workflow_id: WorkflowId,
    task_id: TaskId,
    child_agent_id: AgentId,
    parent_agent_id: Option<AgentId>,
    role: AgentRole,
    access: WorkAccess,
    model_route: ModelRoute,
    effort_route: EffortRoute,
    permissions: PermissionEnvelope,
    handoff: StructuredHandoff,
}

#[derive(Clone, Debug, Deserialize)]
struct SpawnChildRequestWire {
    workflow_id: WorkflowId,
    task_id: TaskId,
    child_agent_id: AgentId,
    parent_agent_id: Option<AgentId>,
    role: AgentRole,
    access: WorkAccess,
    model_route: ModelRoute,
    effort_route: EffortRoute,
    permissions: PermissionEnvelope,
    handoff: StructuredHandoff,
}

impl TryFrom<SpawnChildRequestWire> for SpawnChildRequest {
    type Error = SpawnRequestError;

    fn try_from(value: SpawnChildRequestWire) -> Result<Self, Self::Error> {
        Self::new(
            value.workflow_id,
            value.task_id,
            value.child_agent_id,
            value.parent_agent_id,
            value.role,
            value.access,
            value.model_route,
            value.effort_route,
            value.permissions,
            value.handoff,
        )
    }
}

impl SpawnChildRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workflow_id: WorkflowId,
        task_id: TaskId,
        child_agent_id: AgentId,
        parent_agent_id: Option<AgentId>,
        role: AgentRole,
        access: WorkAccess,
        model_route: ModelRoute,
        effort_route: EffortRoute,
        permissions: PermissionEnvelope,
        handoff: StructuredHandoff,
    ) -> Result<Self, SpawnRequestError> {
        if parent_agent_id.as_ref() == Some(&child_agent_id) {
            return Err(SpawnRequestError::SelfParent);
        }
        if matches!(access, WorkAccess::ReadOnly)
            && matches!(permissions.assignment_access(), WorkAccess::Writer)
        {
            return Err(SpawnRequestError::AccessPermissionConflict);
        }
        Ok(Self {
            workflow_id,
            task_id,
            child_agent_id,
            parent_agent_id,
            role,
            access,
            model_route,
            effort_route,
            permissions,
            handoff,
        })
    }

    pub fn workflow_id(&self) -> &WorkflowId {
        &self.workflow_id
    }
    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }
    pub fn child_agent_id(&self) -> &AgentId {
        &self.child_agent_id
    }
    pub fn parent_agent_id(&self) -> Option<&AgentId> {
        self.parent_agent_id.as_ref()
    }
    pub fn role(&self) -> AgentRole {
        self.role
    }
    pub fn access(&self) -> WorkAccess {
        self.access
    }
    pub fn model_route(&self) -> &ModelRoute {
        &self.model_route
    }
    pub fn effort_route(&self) -> &EffortRoute {
        &self.effort_route
    }
    pub fn permissions(&self) -> PermissionEnvelope {
        self.permissions
    }
    pub fn handoff(&self) -> &StructuredHandoff {
        &self.handoff
    }
}

/// Result of a future native child spawn, preserving both identity domains.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpawnChildResult {
    pub workflow_id: WorkflowId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub runtime_id: RuntimeAgentId,
}
