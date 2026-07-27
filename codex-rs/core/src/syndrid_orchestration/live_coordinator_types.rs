use super::ExecutionModeSelection;
use super::ResolvedExecutionPolicy;
use super::RoutingProfileId;
use super::RoutingRole;
use super::SubagentFailurePolicy;
use super::SubagentRepairFailureCategory;
use super::SubagentToolPolicy;
use std::fmt;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub const MAX_RUN_ID_BYTES: usize = 128;
pub const MAX_INSTRUCTION_BYTES: usize = 32 * 1024;
pub const MAX_CONTEXT_BYTES: usize = 128 * 1024;
pub const MAX_EVENTS: usize = 64;

#[derive(Clone)]
pub struct PlannerTaskSpecification {
    pub task_id: String,
    pub instruction: String,
    pub context: Option<String>,
    pub tool_policy: SubagentToolPolicy,
    pub timeout: Option<Duration>,
}

impl fmt::Debug for PlannerTaskSpecification {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlannerTaskSpecification")
            .field("task_id_bytes", &self.task_id.len())
            .field("instruction_bytes", &self.instruction.len())
            .field(
                "context_bytes",
                &self.context.as_ref().map_or(0, String::len),
            )
            .field("timeout", &self.timeout)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub enum PlanningContract {
    NotRequested,
    Required { instruction: String },
}

#[derive(Clone, Debug)]
pub enum VerificationContract {
    NotRequested,
    Provider { instruction: String },
    Decision(VerificationDecision),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerificationDecision {
    Accepted,
    Rejected {
        category: SubagentRepairFailureCategory,
        reason: String,
        repair_instruction: String,
    },
}

pub struct LiveOrchestrationRequest {
    pub run_id: String,
    pub policy: Option<ResolvedExecutionPolicy>,
    pub routing_profile_id: Option<RoutingProfileId>,
    pub instruction: String,
    pub context: Option<String>,
    pub tasks: Vec<PlannerTaskSpecification>,
    pub planning: PlanningContract,
    pub verification: VerificationContract,
    pub failure_policy: SubagentFailurePolicy,
    pub repair_instruction: String,
    pub approved_tool_policy: SubagentToolPolicy,
    pub cancellation: CancellationToken,
    pub overall_timeout: Option<Duration>,
}

impl fmt::Debug for LiveOrchestrationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LiveOrchestrationRequest")
            .field("run_id_bytes", &self.run_id.len())
            .field("has_policy", &self.policy.is_some())
            .field("routing_profile_id", &self.routing_profile_id)
            .field("instruction_bytes", &self.instruction.len())
            .field(
                "context_bytes",
                &self.context.as_ref().map_or(0, String::len),
            )
            .field("task_count", &self.tasks.len())
            .field("planning", &self.planning)
            .field("verification", &self.verification)
            .field("failure_policy", &self.failure_policy)
            .field("repair_instruction_bytes", &self.repair_instruction.len())
            .field("cancellation_cancelled", &self.cancellation.is_cancelled())
            .field("overall_timeout", &self.overall_timeout)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveRoleState {
    Succeeded,
    Failed,
    Rejected,
    Skipped,
    Cancelled,
    TimedOut,
    BudgetExhausted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveRoleSkipReason {
    Disabled,
    NotRequested,
    NoEligibleRepair,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveRepairResult {
    RepairSucceeded,
    RepairDisabled,
    NotEligible,
    RepairFailed,
    RepairTimedOut,
    Cancelled,
    BudgetExhausted,
    PolicyInvalid,
    RouteMismatch,
    BatchInvalid,
    InitialValidationFailed,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveRoleOutcome {
    pub role: RoutingRole,
    pub state: LiveRoleState,
    pub skip_reason: Option<LiveRoleSkipReason>,
    pub task_ids: Vec<String>,
    pub task_states: Vec<LiveRoleState>,
    pub provider_invocations: usize,
    pub tool_calls: usize,
    pub repair_result: Option<LiveRepairResult>,
    pub repair_attempts: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveOrchestrationTerminal {
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    BudgetExhausted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveEvent {
    RunPrepared,
    PolicyValidated,
    RoleStarted(RoutingRole),
    RoleSkipped(RoutingRole, LiveRoleSkipReason),
    ExecutorBatchStarted,
    VerifierDecision,
    RepairEligibilityEvaluated,
    RepairStarted,
    RunTerminal(LiveOrchestrationTerminal),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveOrchestrationOutcome {
    pub run_id: String,
    pub selected_mode: ExecutionModeSelection,
    pub resolved_policy: super::ResolvedExecutionPolicyExplanation,
    pub routing_profile_id: RoutingProfileId,
    pub terminal: LiveOrchestrationTerminal,
    pub roles: Vec<LiveRoleOutcome>,
    pub peak_concurrency: usize,
    pub provider_invocations: usize,
    pub tool_calls: usize,
    pub cancelled: bool,
    pub timed_out: bool,
    pub budget_exhausted: bool,
    pub terminal_error: Option<LiveOrchestrationError>,
    pub synthesis_permitted: bool,
    pub events: Vec<LiveEvent>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveOrchestrationError {
    InvalidRequest,
    SessionAlreadyRunning,
    InvalidSessionState,
    UnresolvedExecutionPolicy,
    InvalidExecutionPolicy,
    MissingRoutingProfile,
    MissingRequiredRoleRoute(RoutingRole),
    DisabledRequiredRole(RoutingRole),
    InvalidProviderConnection(RoutingRole),
    PlanningRequiredButDisabled,
    ExecutorTasksExceedPolicyCeiling,
    InvalidTaskIdentifiers,
    ExecutorBatchFailure,
    VerifierRuntimeFailure,
    VerifierRejected,
    RepairUnavailable,
    RepairFailed,
    RepairPolicyInvalid,
    RepairInitialValidationFailed,
    RepairRouteMismatch,
    RepairBatchInvalid,
    Cancellation,
    Timeout,
    BudgetExhaustion,
}
impl fmt::Display for LiveOrchestrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::MissingRequiredRoleRoute(role) => {
                return write!(formatter, "missing {role} route");
            }
            Self::DisabledRequiredRole(role) => return write!(formatter, "disabled {role} route"),
            Self::InvalidProviderConnection(role) => {
                return write!(formatter, "invalid {role} provider connection");
            }
            Self::PlanningRequiredButDisabled => "planning is required but Planner is disabled",
            Self::ExecutorTasksExceedPolicyCeiling => {
                "executor task count exceeds the execution policy"
            }
            Self::InvalidTaskIdentifiers => "executor task identifiers are invalid",
            Self::ExecutorBatchFailure => "executor batch failed",
            Self::VerifierRuntimeFailure => "verifier runtime failed",
            Self::VerifierRejected => "verifier rejected the executor result",
            Self::RepairUnavailable => "repair is unavailable",
            Self::RepairFailed => "repair failed",
            Self::RepairPolicyInvalid => "repair policy is invalid",
            Self::RepairInitialValidationFailed => "repair initial validation failed",
            Self::RepairRouteMismatch => "repair route mismatch",
            Self::RepairBatchInvalid => "repair batch is invalid",
            Self::Cancellation => "live orchestration was cancelled",
            Self::Timeout => "live orchestration timed out",
            Self::BudgetExhaustion => "live orchestration budget is exhausted",
            Self::InvalidRequest => "live orchestration request is invalid",
            Self::SessionAlreadyRunning => "session already has a live run",
            Self::InvalidSessionState => "session lifecycle transition is invalid",
            Self::UnresolvedExecutionPolicy => "execution policy is unresolved",
            Self::InvalidExecutionPolicy => "execution policy is invalid",
            Self::MissingRoutingProfile => "routing profile is missing",
        };
        formatter.write_str(text)
    }
}
impl std::error::Error for LiveOrchestrationError {}
