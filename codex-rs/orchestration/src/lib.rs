//! Behavior-free domain values for future Syndrid orchestration.
//!
//! This crate describes behavior-free orchestration domain values. It does not execute agents,
//! schedule work, persist state, or own Codex runtime behavior.

mod agent;
mod budget;
mod event;
mod handoff;
mod ids;
mod mode;
mod permissions;
mod recommendation;
mod routing;
mod sequential;
mod state;
mod verification;

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
pub use event::EventReference;
pub use event::WorkflowEvent;
pub use event::WorkflowEventKind;
pub use handoff::BoundedText;
pub use handoff::BoundedTextError;
pub use handoff::MAX_HANDOFF_TEXT_BYTES;
pub use handoff::StructuredHandoff;
pub use ids::AgentId;
pub use ids::IdentifierError;
pub use ids::TaskId;
pub use ids::WorkflowId;
pub use mode::OrchestrationMode;
pub use permissions::PermissionEnvelope;
pub use permissions::PermissionEnvelopeError;
pub use recommendation::Forecast;
pub use recommendation::ForecastConfidence;
pub use recommendation::MAX_RECOMMENDATION_NOTES;
pub use recommendation::Recommendation;
pub use recommendation::RecommendationError;
pub use routing::EffortRoute;
pub use routing::ModelRoute;
pub use routing::RouteSource;
pub use routing::RouteStatus;
pub use sequential::SequentialStage;
pub use sequential::SequentialWorkflow;
pub use sequential::SequentialWorkflowError;
pub use sequential::SequentialWorkflowState;
pub use sequential::StageCorrelation;
pub use sequential::StageExecutor;
pub use sequential::StageFailureCode;
pub use sequential::StageId;
pub use sequential::StageIdError;
pub use sequential::StageInput;
pub use sequential::StageOutput;
pub use sequential::StageResult;
pub use sequential::StageState;
pub use state::CancellationState;
pub use state::DataQuality;
pub use state::RunLifecycleState;
pub use state::VerificationState;
pub use state::WaitReason;
pub use state::WorkflowRun;
pub use state::WorkflowStage;
pub use verification::EvidenceKind;
pub use verification::EvidenceResult;
pub use verification::VerificationEvidence;
pub use verification::VerificationRequirement;

#[cfg(test)]
#[path = "ids_tests.rs"]
mod ids_tests;

#[cfg(test)]
#[path = "domain_tests.rs"]
mod domain_tests;

#[cfg(test)]
#[path = "o1b_tests.rs"]
mod o1b_tests;

#[cfg(test)]
#[path = "sequential_tests.rs"]
mod sequential_tests;
