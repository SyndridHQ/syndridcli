use super::LiveOrchestrationError;
use super::LiveOrchestrationTerminal;
use super::execution_budget_accounting::BudgetExhaustionCategory;
use super::routing_profiles::RoutingRole;
use super::subagent::SubagentError;
use super::subagent::SubagentStatus;
use super::subagent_tools::SubagentToolKind;
use std::sync::Mutex;

/// Identifies the authoritative class of a failure without retaining failure material.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrchestrationFailureKind {
    InvalidRequest,
    InvalidPolicy,
    InvalidTask,
    InvalidRoute,
    InvalidLifecycleState,
    StaleGeneration,
    PlannerProviderFailure,
    PlannerToolFailure,
    PlannerCancelled,
    PlannerTimedOut,
    PlannerBudgetExhausted,
    PlannerJoinFailure,
    ExecutorProviderFailure,
    ExecutorToolFailure,
    ExecutorCancelled,
    ExecutorTimedOut,
    ExecutorBudgetExhausted,
    ExecutorJoinFailure,
    ExecutorBatchFailure,
    VerifierRejected,
    VerifierProviderFailure,
    VerifierToolFailure,
    VerifierCancelled,
    VerifierTimedOut,
    VerifierBudgetExhausted,
    VerifierJoinFailure,
    RepairNotEligible,
    RepairDisabled,
    RepairProviderFailure,
    RepairToolFailure,
    RepairCancelled,
    RepairTimedOut,
    RepairBudgetExhausted,
    RepairJoinFailure,
    RepairFailed,
    UserCancelled,
    TotalTimedOut,
    BudgetExhausted(BudgetExhaustionCategory),
    LifecycleViolation,
    RoutingFailure,
    ProviderFailure,
    ToolFailure,
    TaskJoinFailure,
    InternalCoordinatorFailure,
}

/// Reports whether a caller may retry after inspecting the failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Retryability {
    NotRetryable,
    RetryableSameRoute,
    RetryableAfterUserAction,
    Unknown,
}

/// Safe, typed failure information attached to a terminal coordinator outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrchestrationFailure {
    pub kind: OrchestrationFailureKind,
    pub retryability: Retryability,
    pub role: Option<RoutingRole>,
    pub tool: Option<SubagentToolKind>,
    pub terminal: LiveOrchestrationTerminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalCauseSubmission {
    Accepted(OrchestrationFailure),
    Retained(OrchestrationFailure),
    Replaced(OrchestrationFailure),
    StaleGeneration,
    Frozen(OrchestrationFailure),
}

/// Serializes terminal-cause selection for one live-run generation.
pub(crate) struct TerminalCauseArbiter {
    generation: u64,
    state: Mutex<TerminalCauseState>,
}

struct TerminalCauseState {
    accepted: Option<OrchestrationFailure>,
    frozen: bool,
}

impl TerminalCauseArbiter {
    pub(crate) fn new(generation: u64) -> Self {
        Self {
            generation,
            state: Mutex::new(TerminalCauseState {
                accepted: None,
                frozen: false,
            }),
        }
    }

    pub(crate) fn submit(
        &self,
        generation: u64,
        candidate: OrchestrationFailure,
    ) -> TerminalCauseSubmission {
        let Ok(mut state) = self.state.lock() else {
            return TerminalCauseSubmission::Frozen(candidate);
        };
        if generation != self.generation {
            return TerminalCauseSubmission::StaleGeneration;
        }
        if state.frozen {
            return state
                .accepted
                .map(TerminalCauseSubmission::Frozen)
                .unwrap_or(TerminalCauseSubmission::Frozen(candidate));
        }
        let Some(existing) = state.accepted else {
            state.accepted = Some(candidate);
            return TerminalCauseSubmission::Accepted(candidate);
        };
        if existing.kind == OrchestrationFailureKind::TotalTimedOut
            && candidate.kind == OrchestrationFailureKind::UserCancelled
        {
            return TerminalCauseSubmission::Retained(existing);
        }
        if cause_priority(candidate.kind) > cause_priority(existing.kind) {
            state.accepted = Some(candidate);
            TerminalCauseSubmission::Replaced(candidate)
        } else {
            TerminalCauseSubmission::Retained(existing)
        }
    }

    pub(crate) fn freeze(&self, generation: u64) -> Result<Option<OrchestrationFailure>, ()> {
        let Ok(mut state) = self.state.lock() else {
            return Err(());
        };
        if generation != self.generation {
            return Err(());
        }
        state.frozen = true;
        Ok(state.accepted)
    }

    pub(crate) fn current(&self, generation: u64) -> Result<Option<OrchestrationFailure>, ()> {
        let Ok(state) = self.state.lock() else {
            return Err(());
        };
        if generation != self.generation {
            return Err(());
        }
        Ok(state.accepted)
    }
}

fn cause_priority(kind: OrchestrationFailureKind) -> u8 {
    match kind {
        OrchestrationFailureKind::UserCancelled => 6,
        OrchestrationFailureKind::TotalTimedOut => 5,
        OrchestrationFailureKind::BudgetExhausted(_) => 4,
        OrchestrationFailureKind::TaskJoinFailure
        | OrchestrationFailureKind::PlannerJoinFailure
        | OrchestrationFailureKind::ExecutorJoinFailure
        | OrchestrationFailureKind::VerifierJoinFailure
        | OrchestrationFailureKind::RepairJoinFailure => 2,
        OrchestrationFailureKind::InternalCoordinatorFailure => 1,
        _ => 3,
    }
}

impl OrchestrationFailure {
    pub(crate) fn from_terminal(
        terminal: LiveOrchestrationTerminal,
        error: Option<LiveOrchestrationError>,
        roles: &[super::LiveRoleOutcome],
        budget: Option<BudgetExhaustionCategory>,
    ) -> Option<Self> {
        let kind = match terminal {
            LiveOrchestrationTerminal::Completed => return None,
            LiveOrchestrationTerminal::Cancelled => OrchestrationFailureKind::UserCancelled,
            LiveOrchestrationTerminal::TimedOut => OrchestrationFailureKind::TotalTimedOut,
            LiveOrchestrationTerminal::BudgetExhausted => {
                OrchestrationFailureKind::BudgetExhausted(
                    budget.unwrap_or(BudgetExhaustionCategory::TotalProviderInvocations),
                )
            }
            LiveOrchestrationTerminal::Failed => {
                let role_kind = Self::kind_from_roles(roles);
                match (error, role_kind) {
                    (Some(LiveOrchestrationError::ExecutorBatchFailure), Some(kind))
                        if kind != OrchestrationFailureKind::ExecutorBatchFailure =>
                    {
                        kind
                    }
                    (Some(LiveOrchestrationError::VerifierRuntimeFailure), Some(kind))
                        if kind != OrchestrationFailureKind::VerifierProviderFailure =>
                    {
                        kind
                    }
                    (Some(error), _) => Self::kind_from_error(error),
                    (None, Some(kind)) => kind,
                    (None, None) => OrchestrationFailureKind::InternalCoordinatorFailure,
                }
            }
        };
        let role = role_for_kind(kind, roles);
        Some(Self {
            kind,
            retryability: retryability(kind),
            role,
            tool: None,
            terminal,
        })
    }

    fn kind_from_error(error: LiveOrchestrationError) -> OrchestrationFailureKind {
        match error {
            LiveOrchestrationError::InvalidRequest => OrchestrationFailureKind::InvalidRequest,
            LiveOrchestrationError::InvalidExecutionPolicy
            | LiveOrchestrationError::UnresolvedExecutionPolicy => {
                OrchestrationFailureKind::InvalidPolicy
            }
            LiveOrchestrationError::InvalidTaskIdentifiers
            | LiveOrchestrationError::ExecutorTasksExceedPolicyCeiling => {
                OrchestrationFailureKind::InvalidTask
            }
            LiveOrchestrationError::MissingRoutingProfile
            | LiveOrchestrationError::MissingRequiredRoleRoute(_)
            | LiveOrchestrationError::DisabledRequiredRole(_)
            | LiveOrchestrationError::InvalidProviderConnection(_)
            | LiveOrchestrationError::PlanningRequiredButDisabled
            | LiveOrchestrationError::RepairRouteMismatch => {
                OrchestrationFailureKind::RoutingFailure
            }
            LiveOrchestrationError::SessionAlreadyRunning
            | LiveOrchestrationError::InvalidSessionState => {
                OrchestrationFailureKind::InvalidLifecycleState
            }
            LiveOrchestrationError::ExecutorBatchFailure => {
                OrchestrationFailureKind::ExecutorBatchFailure
            }
            LiveOrchestrationError::ExecutorJoinFailure => {
                OrchestrationFailureKind::ExecutorJoinFailure
            }
            LiveOrchestrationError::VerifierRuntimeFailure => {
                OrchestrationFailureKind::VerifierProviderFailure
            }
            LiveOrchestrationError::VerifierRejected => OrchestrationFailureKind::VerifierRejected,
            LiveOrchestrationError::RepairUnavailable => {
                OrchestrationFailureKind::RepairNotEligible
            }
            LiveOrchestrationError::RepairFailed => OrchestrationFailureKind::RepairFailed,
            LiveOrchestrationError::RepairPolicyInvalid => OrchestrationFailureKind::InvalidPolicy,
            LiveOrchestrationError::RepairInitialValidationFailed => {
                OrchestrationFailureKind::InvalidTask
            }
            LiveOrchestrationError::RepairBatchInvalid => OrchestrationFailureKind::InvalidTask,
            LiveOrchestrationError::RepairJoinFailure => {
                OrchestrationFailureKind::RepairJoinFailure
            }
            LiveOrchestrationError::Cancellation => OrchestrationFailureKind::UserCancelled,
            LiveOrchestrationError::Timeout => OrchestrationFailureKind::TotalTimedOut,
            LiveOrchestrationError::BudgetExhaustion => OrchestrationFailureKind::BudgetExhausted(
                BudgetExhaustionCategory::TotalProviderInvocations,
            ),
            LiveOrchestrationError::BudgetExhaustionCategory(category) => {
                OrchestrationFailureKind::BudgetExhausted(category)
            }
            LiveOrchestrationError::InternalCoordinatorFailure => {
                OrchestrationFailureKind::InternalCoordinatorFailure
            }
        }
    }

    fn kind_from_roles(roles: &[super::LiveRoleOutcome]) -> Option<OrchestrationFailureKind> {
        roles
            .iter()
            .find(|role| {
                matches!(
                    role.state,
                    super::LiveRoleState::Failed
                        | super::LiveRoleState::Cancelled
                        | super::LiveRoleState::TimedOut
                        | super::LiveRoleState::BudgetExhausted
                )
            })
            .map(|role| match role.role {
                RoutingRole::Planner => OrchestrationFailureKind::PlannerProviderFailure,
                RoutingRole::Executor => OrchestrationFailureKind::ExecutorBatchFailure,
                RoutingRole::Verifier => OrchestrationFailureKind::VerifierProviderFailure,
                RoutingRole::Repair => OrchestrationFailureKind::RepairFailed,
                RoutingRole::Main => OrchestrationFailureKind::ProviderFailure,
            })
    }
}

pub(crate) fn classify_subagent_failure(
    role: RoutingRole,
    error: Option<SubagentError>,
    status: Option<SubagentStatus>,
) -> OrchestrationFailureKind {
    let status_kind = match status {
        Some(SubagentStatus::Cancelled) => Some(OrchestrationFailureKind::UserCancelled),
        Some(SubagentStatus::TimedOut) => Some(OrchestrationFailureKind::TotalTimedOut),
        Some(SubagentStatus::BudgetExhausted) => Some(OrchestrationFailureKind::BudgetExhausted(
            BudgetExhaustionCategory::TotalProviderInvocations,
        )),
        Some(SubagentStatus::ToolExecutionFailed) => Some(tool_failure_kind(role)),
        Some(SubagentStatus::RoutingFailed) => Some(OrchestrationFailureKind::RoutingFailure),
        Some(SubagentStatus::InternalFailure) => {
            Some(OrchestrationFailureKind::InternalCoordinatorFailure)
        }
        _ => None,
    };
    status_kind.unwrap_or_else(|| match error {
        Some(SubagentError::CancelledBeforeStart | SubagentError::InvocationCancelled) => {
            OrchestrationFailureKind::UserCancelled
        }
        Some(SubagentError::InvocationTimedOut) => OrchestrationFailureKind::TotalTimedOut,
        Some(SubagentError::JoinFailure) => join_failure_kind(role),
        Some(SubagentError::InvalidTaskId | SubagentError::EmptyInstruction) => {
            OrchestrationFailureKind::InvalidTask
        }
        Some(
            SubagentError::NoActiveProfile
            | SubagentError::UnknownActiveProfile
            | SubagentError::MissingRoleAssignment
            | SubagentError::UnknownProvider
            | SubagentError::UnknownConnection
            | SubagentError::ConnectionDisabled
            | SubagentError::ConnectionUnconfigured
            | SubagentError::ReauthenticationRequired
            | SubagentError::InvalidModel
            | SubagentError::ModelUnverified,
        ) => OrchestrationFailureKind::RoutingFailure,
        Some(SubagentError::InvalidToolPolicy) => tool_failure_kind(role),
        _ => provider_failure_kind(role),
    })
}

pub(crate) fn classify_repair_failure(
    error: Option<super::SubagentRepairError>,
) -> OrchestrationFailureKind {
    match error {
        Some(super::SubagentRepairError::JoinFailure) => {
            OrchestrationFailureKind::RepairJoinFailure
        }
        Some(super::SubagentRepairError::CancelledBeforeRepair) => {
            OrchestrationFailureKind::RepairCancelled
        }
        Some(super::SubagentRepairError::BudgetExhausted) => {
            OrchestrationFailureKind::RepairBudgetExhausted
        }
        Some(super::SubagentRepairError::PolicyInvalid) => OrchestrationFailureKind::InvalidPolicy,
        Some(super::SubagentRepairError::RouteMismatch) => OrchestrationFailureKind::InvalidRoute,
        Some(super::SubagentRepairError::BatchInvalid) => OrchestrationFailureKind::RepairFailed,
        Some(super::SubagentRepairError::InitialValidationFailed(_)) => {
            OrchestrationFailureKind::InvalidTask
        }
        None => OrchestrationFailureKind::RepairFailed,
    }
}

fn join_failure_kind(role: RoutingRole) -> OrchestrationFailureKind {
    match role {
        RoutingRole::Planner => OrchestrationFailureKind::PlannerJoinFailure,
        RoutingRole::Executor => OrchestrationFailureKind::ExecutorJoinFailure,
        RoutingRole::Verifier => OrchestrationFailureKind::VerifierJoinFailure,
        RoutingRole::Repair => OrchestrationFailureKind::RepairJoinFailure,
        RoutingRole::Main => OrchestrationFailureKind::TaskJoinFailure,
    }
}

fn provider_failure_kind(role: RoutingRole) -> OrchestrationFailureKind {
    match role {
        RoutingRole::Planner => OrchestrationFailureKind::PlannerProviderFailure,
        RoutingRole::Executor => OrchestrationFailureKind::ExecutorProviderFailure,
        RoutingRole::Verifier => OrchestrationFailureKind::VerifierProviderFailure,
        RoutingRole::Repair => OrchestrationFailureKind::RepairProviderFailure,
        RoutingRole::Main => OrchestrationFailureKind::ProviderFailure,
    }
}

fn tool_failure_kind(role: RoutingRole) -> OrchestrationFailureKind {
    match role {
        RoutingRole::Planner => OrchestrationFailureKind::PlannerToolFailure,
        RoutingRole::Executor => OrchestrationFailureKind::ExecutorToolFailure,
        RoutingRole::Verifier => OrchestrationFailureKind::VerifierToolFailure,
        RoutingRole::Repair => OrchestrationFailureKind::RepairToolFailure,
        RoutingRole::Main => OrchestrationFailureKind::ToolFailure,
    }
}

fn role_for_kind(
    kind: OrchestrationFailureKind,
    roles: &[super::LiveRoleOutcome],
) -> Option<RoutingRole> {
    roles.iter().find_map(|role| {
        let matches = match kind {
            OrchestrationFailureKind::PlannerProviderFailure
            | OrchestrationFailureKind::PlannerToolFailure
            | OrchestrationFailureKind::PlannerCancelled
            | OrchestrationFailureKind::PlannerTimedOut
            | OrchestrationFailureKind::PlannerBudgetExhausted
            | OrchestrationFailureKind::PlannerJoinFailure => role.role == RoutingRole::Planner,
            OrchestrationFailureKind::ExecutorProviderFailure
            | OrchestrationFailureKind::ExecutorToolFailure
            | OrchestrationFailureKind::ExecutorCancelled
            | OrchestrationFailureKind::ExecutorTimedOut
            | OrchestrationFailureKind::ExecutorBudgetExhausted
            | OrchestrationFailureKind::ExecutorJoinFailure
            | OrchestrationFailureKind::ExecutorBatchFailure => role.role == RoutingRole::Executor,
            OrchestrationFailureKind::VerifierRejected
            | OrchestrationFailureKind::VerifierProviderFailure
            | OrchestrationFailureKind::VerifierToolFailure
            | OrchestrationFailureKind::VerifierCancelled
            | OrchestrationFailureKind::VerifierTimedOut
            | OrchestrationFailureKind::VerifierBudgetExhausted
            | OrchestrationFailureKind::VerifierJoinFailure => role.role == RoutingRole::Verifier,
            OrchestrationFailureKind::RepairNotEligible
            | OrchestrationFailureKind::RepairDisabled
            | OrchestrationFailureKind::RepairProviderFailure
            | OrchestrationFailureKind::RepairToolFailure
            | OrchestrationFailureKind::RepairCancelled
            | OrchestrationFailureKind::RepairTimedOut
            | OrchestrationFailureKind::RepairBudgetExhausted
            | OrchestrationFailureKind::RepairJoinFailure
            | OrchestrationFailureKind::RepairFailed => role.role == RoutingRole::Repair,
            _ => false,
        };
        matches.then_some(role.role)
    })
}

fn retryability(kind: OrchestrationFailureKind) -> Retryability {
    match kind {
        OrchestrationFailureKind::InvalidRequest
        | OrchestrationFailureKind::InvalidPolicy
        | OrchestrationFailureKind::InvalidTask
        | OrchestrationFailureKind::InvalidRoute
        | OrchestrationFailureKind::RoutingFailure
        | OrchestrationFailureKind::InvalidLifecycleState
        | OrchestrationFailureKind::StaleGeneration => Retryability::RetryableAfterUserAction,
        OrchestrationFailureKind::UserCancelled => Retryability::NotRetryable,
        OrchestrationFailureKind::PlannerProviderFailure
        | OrchestrationFailureKind::ExecutorProviderFailure
        | OrchestrationFailureKind::VerifierProviderFailure
        | OrchestrationFailureKind::RepairProviderFailure
        | OrchestrationFailureKind::ProviderFailure => Retryability::RetryableSameRoute,
        _ => Retryability::Unknown,
    }
}
