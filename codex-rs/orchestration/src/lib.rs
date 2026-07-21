//! Behavior-free domain values for future Syndrid orchestration.
//!
//! This crate describes behavior-free orchestration domain values. It does not execute agents,
//! schedule work, persist state, or own Codex runtime behavior.

mod agent;
mod budget;
mod ids;
mod mode;
mod permissions;
mod routing;
mod state;

pub use agent::AgentAssignment;
pub use agent::AgentProfile;
pub use agent::AgentRole;
pub use agent::WorkAccess;
pub use agent::WorkClaim;
pub use budget::AdaptiveBudgetPolicy;
pub use budget::EfficiencyPosture;
pub use budget::MultiplierError;
pub use budget::UsageBudgetMultiplier;
pub use budget::UsageQuantity;
pub use budget::WorkflowBudget;
pub use ids::AgentId;
pub use ids::IdentifierError;
pub use ids::TaskId;
pub use ids::WorkflowId;
pub use mode::OrchestrationMode;
pub use permissions::PermissionEnvelope;
pub use permissions::PermissionEnvelopeError;
pub use routing::EffortRoute;
pub use routing::ModelRoute;
pub use routing::RouteSource;
pub use routing::RouteStatus;
pub use state::CancellationState;
pub use state::DataQuality;
pub use state::RunLifecycleState;
pub use state::VerificationState;
pub use state::WaitReason;
pub use state::WorkflowRun;
pub use state::WorkflowStage;

#[cfg(test)]
#[path = "ids_tests.rs"]
mod ids_tests;

#[cfg(test)]
#[path = "domain_tests.rs"]
mod domain_tests;
