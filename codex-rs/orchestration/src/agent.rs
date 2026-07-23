use serde::Deserialize;
use serde::Serialize;

use crate::AgentId;
use crate::EffortRoute;
use crate::ModelRoute;
use crate::TaskId;

/// Stable role label for a future workflow assignment.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Planner,
    Explorer,
    Executor,
    Verifier,
}

/// Capability ceiling for work in a shared worktree.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkAccess {
    ReadOnly,
    Writer,
}

impl WorkAccess {
    pub const fn allows(self, requested: Self) -> bool {
        matches!(self, Self::Writer) || matches!(requested, Self::ReadOnly)
    }
}

/// Data-only configuration for a future assigned role.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProfile {
    pub role: AgentRole,
    pub requested_model: Option<ModelRoute>,
    pub requested_effort: Option<EffortRoute>,
    pub access: WorkAccess,
    pub role_label: Option<String>,
}

/// A bounded claim that associates one agent with one task and access level.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkClaim {
    pub agent_id: AgentId,
    pub task_id: TaskId,
    pub access: WorkAccess,
}

/// Data-only association of an agent, task, profile, and optional work claim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAssignment {
    pub agent_id: AgentId,
    pub task_id: TaskId,
    pub profile: AgentProfile,
    pub claim: Option<WorkClaim>,
}
