use super::ExecutionModeSelection;
use super::LiveOrchestrationTerminal;
use super::PolicySource;
use super::RoutingProfileId;
use super::RoutingRole;
use super::SessionExecutionStatus;
use super::execution_budget_accounting::BudgetExhaustion;
use super::execution_budget_accounting::BudgetExhaustionCategory;
use std::time::Duration;

/// Describes how directly an observation is supported by runtime data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationQuality {
    Exact,
    Derived,
    Estimated,
    Unavailable,
}

/// A value paired with an explicit data-quality classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observed<T> {
    pub value: Option<T>,
    pub quality: ObservationQuality,
}

impl<T> Observed<T> {
    pub(crate) fn exact(value: T) -> Self {
        Self {
            value: Some(value),
            quality: ObservationQuality::Exact,
        }
    }

    pub(crate) fn derived(value: T) -> Self {
        Self {
            value: Some(value),
            quality: ObservationQuality::Derived,
        }
    }

    pub(crate) fn unavailable() -> Self {
        Self {
            value: None,
            quality: ObservationQuality::Unavailable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrchestrationObservationStage {
    Idle,
    Preparing,
    Validating,
    Planning,
    Executing,
    Verifying,
    Repairing,
    Cancelling,
    CleaningUp,
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservedActiveRole {
    Main,
    Planner,
    ExecutorBatch,
    Verifier,
    Repair,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationTerminalReason {
    Completed,
    ValidationFailed,
    ProviderFailed,
    ToolFailed,
    VerifierRejected,
    RepairFailed,
    Cancelled,
    TimedOut,
    BudgetExhausted(BudgetExhaustion),
    LifecycleViolation,
    RoutingFailure,
    InternalCoordinatorFailure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationTaskCounts {
    pub total: Observed<usize>,
    pub queued: Observed<usize>,
    pub active: Observed<usize>,
    pub completed: Observed<usize>,
    pub failed: Observed<usize>,
    pub cancelled: Observed<usize>,
    pub outcomes_available: Observed<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationRoleProviderUsage {
    pub role: RoutingRole,
    pub admitted: Observed<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationProviderUsage {
    pub reserved: Observed<usize>,
    pub started: Observed<usize>,
    pub completed: Observed<usize>,
    pub cancelled_after_start: Observed<usize>,
    pub rejected_before_start: Observed<usize>,
    pub by_role: Vec<ObservationRoleProviderUsage>,
    pub input_tokens: Observed<u64>,
    pub output_tokens: Observed<u64>,
    pub cached_input_tokens: Observed<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationToolUsage {
    pub reserved: Observed<usize>,
    pub started: Observed<usize>,
    pub completed: Observed<usize>,
    pub rejected: Observed<usize>,
    pub output_bytes: Observed<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationBudget {
    pub category: BudgetExhaustionCategory,
    pub role: Option<RoutingRole>,
    pub limit: Observed<u64>,
    pub consumed_or_reserved: Observed<u64>,
    pub remaining: Observed<u64>,
    pub exhausted: Observed<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationRepairState {
    pub enabled: Observed<bool>,
    pub eligible: Observed<bool>,
    pub attempted: Observed<bool>,
    pub attempts: Observed<usize>,
    pub result: Observed<super::LiveRepairResult>,
    pub timed_out: Observed<bool>,
}

/// Immutable, privacy-safe state for one orchestration generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestrationObservationSnapshot {
    pub generation: Observed<u64>,
    pub run_id: Observed<String>,
    pub selected_mode: Observed<ExecutionModeSelection>,
    pub policy_source: Observed<PolicySource>,
    pub routing_profile_id: Observed<RoutingProfileId>,
    pub lifecycle: Observed<SessionExecutionStatus>,
    pub stage: Observed<OrchestrationObservationStage>,
    pub active_role: Observed<ObservedActiveRole>,
    pub terminal: Observed<Option<LiveOrchestrationTerminal>>,
    pub terminal_reason: Observed<Option<ObservationTerminalReason>>,
    pub synthesis_permitted: Observed<bool>,
    pub tasks: ObservationTaskCounts,
    pub provider: ObservationProviderUsage,
    pub tools: ObservationToolUsage,
    pub budgets: Vec<ObservationBudget>,
    pub current_provider_count: Observed<usize>,
    pub current_tool_count: Observed<usize>,
    pub current_executor_concurrency: Observed<usize>,
    pub peak_executor_concurrency: Observed<usize>,
    pub elapsed: Observed<Duration>,
    pub configured_timeout: Observed<Duration>,
    pub remaining_time: Observed<Duration>,
    pub timed_out: Observed<bool>,
    pub cancelled: Observed<bool>,
    pub cleanup_pending: Observed<bool>,
    pub repair: ObservationRepairState,
}

impl OrchestrationObservationSnapshot {
    pub(crate) fn with_terminal(
        mut self,
        terminal: LiveOrchestrationTerminal,
        reason: ObservationTerminalReason,
        timed_out: bool,
        cancelled: bool,
    ) -> Self {
        self.stage = Observed::exact(OrchestrationObservationStage::Terminal);
        self.terminal = Observed::exact(Some(terminal));
        self.terminal_reason = Observed::exact(Some(reason));
        self.synthesis_permitted = Observed::exact(false);
        self.timed_out = Observed::exact(timed_out);
        self.cancelled = Observed::exact(cancelled);
        self.cleanup_pending = Observed::exact(false);
        self.current_provider_count = Observed::exact(0);
        self.current_tool_count = Observed::exact(0);
        self.current_executor_concurrency = Observed::exact(0);
        self
    }
}
