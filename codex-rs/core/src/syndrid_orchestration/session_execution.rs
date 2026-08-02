use super::ExecutionModeSelection;
use super::ExecutionPolicyError;
use super::OrchestrationMode;
use super::OrchestrationStrategyAvailability;
use super::PolicySource;
use super::ResolvedExecutionPolicy;
use super::ResolvedOrchestrationPolicy;
use super::RoutingProfileId;
use super::RoutingRole;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::sync::Mutex;

/// The validation state of the resolved policy and its selected route.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionPolicyValidation {
    Unresolved,
    Valid,
    Invalid(ExecutionPolicyError),
}

/// The lifecycle state of one reusable live-session execution boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionExecutionStatus {
    Idle,
    Preparing,
    Validating,
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

/// The origin of the policy currently selected by a session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionPolicySource {
    Default,
    ExplicitUserSelection,
    SessionOverride,
    ExplicitCustomPolicy,
}

/// A typed failure from a session-state operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionExecutionStateError {
    LockUnavailable,
    InvalidTransition {
        from: SessionExecutionStatus,
        to: SessionExecutionStatus,
    },
    RunAlreadyActive,
    PolicyMutationWhileActive,
    RoutingMutationWhileActive,
    PolicyUnresolved,
    ResetWhileActive,
    ResetWhileCleanupPending,
    StaleRunGeneration,
}

impl fmt::Display for SessionExecutionStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LockUnavailable => formatter.write_str("session state is unavailable"),
            Self::InvalidTransition { from, to } => {
                write!(
                    formatter,
                    "invalid session transition from {from:?} to {to:?}"
                )
            }
            Self::RunAlreadyActive => formatter.write_str("session already has a live run"),
            Self::PolicyMutationWhileActive => {
                formatter.write_str("execution policy cannot change during a live run")
            }
            Self::RoutingMutationWhileActive => {
                formatter.write_str("routing profile cannot change during a live run")
            }
            Self::PolicyUnresolved => formatter.write_str("execution policy is unresolved"),
            Self::ResetWhileActive => formatter.write_str("session cannot reset while active"),
            Self::ResetWhileCleanupPending => {
                formatter.write_str("session reset is blocked while cleanup is pending")
            }
            Self::StaleRunGeneration => formatter.write_str("stale live-run generation"),
        }
    }
}

impl std::error::Error for SessionExecutionStateError {}

#[derive(Clone)]
struct SessionExecutionInner {
    strategy: OrchestrationMode,
    selected_mode: ExecutionModeSelection,
    resolved_policy: ResolvedExecutionPolicy,
    resolved_orchestration_policy: ResolvedOrchestrationPolicy,
    routing_profile_id: Option<RoutingProfileId>,
    source: SessionPolicySource,
    validation: SessionPolicyValidation,
    status: SessionExecutionStatus,
    next_generation: u64,
    active_generation: Option<u64>,
}

/// Read-only inspection of session policy plus a guarded live-run lifecycle.
#[derive(Clone)]
pub struct SessionExecutionPolicyState {
    inner: Arc<Mutex<SessionExecutionInner>>,
}

impl fmt::Debug for SessionExecutionPolicyState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Ok(inner) = self.inner.lock() else {
            return formatter.write_str("SessionExecutionPolicyState(<unavailable>)");
        };
        formatter
            .debug_struct("SessionExecutionPolicyState")
            .field("selected_mode", &inner.selected_mode)
            .field("resolved_mode", &resolved_mode_id(&inner.resolved_policy))
            .field("routing_profile_id", &inner.routing_profile_id)
            .field("source", &inner.source)
            .field("validation", &inner.validation)
            .field("status", &inner.status)
            .finish()
    }
}

impl SessionExecutionPolicyState {
    /// Creates a session state with O6E's Balanced default and no selected route.
    pub fn new() -> Result<Self, ExecutionPolicyError> {
        Self::with_selection(
            ExecutionModeSelection::default(),
            SessionPolicySource::Default,
        )
    }

    pub fn with_selection(
        selection: ExecutionModeSelection,
        source: SessionPolicySource,
    ) -> Result<Self, ExecutionPolicyError> {
        Self::with_strategy_selection(OrchestrationMode::Single, selection, source)
    }

    pub fn with_strategy_selection(
        strategy: OrchestrationMode,
        selection: ExecutionModeSelection,
        source: SessionPolicySource,
    ) -> Result<Self, ExecutionPolicyError> {
        let resolved_orchestration_policy =
            ResolvedOrchestrationPolicy::resolve(strategy, selection.clone())?;
        let resolved_policy = resolved_orchestration_policy.execution().clone();
        Ok(Self {
            inner: Arc::new(Mutex::new(SessionExecutionInner {
                strategy,
                selected_mode: selection,
                resolved_policy,
                resolved_orchestration_policy,
                routing_profile_id: None,
                source,
                validation: SessionPolicyValidation::Unresolved,
                status: SessionExecutionStatus::Idle,
                next_generation: 0,
                active_generation: None,
            })),
        })
    }

    pub fn select_mode(
        &self,
        selection: ExecutionModeSelection,
        source: SessionPolicySource,
    ) -> Result<(), SessionExecutionStateError> {
        let mut inner = self.lock()?;
        ensure_idle(&inner, true)?;
        let resolved_orchestration_policy =
            ResolvedOrchestrationPolicy::resolve(inner.strategy, selection.clone())
                .map_err(|_| SessionExecutionStateError::PolicyUnresolved)?;
        inner.selected_mode = selection;
        inner.resolved_policy = resolved_orchestration_policy.execution().clone();
        inner.resolved_orchestration_policy = resolved_orchestration_policy;
        inner.source = source;
        inner.validation = SessionPolicyValidation::Unresolved;
        Ok(())
    }

    pub fn select_strategy(
        &self,
        strategy: OrchestrationMode,
    ) -> Result<(), SessionExecutionStateError> {
        let mut inner = self.lock()?;
        ensure_idle(&inner, true)?;
        let resolved_orchestration_policy =
            ResolvedOrchestrationPolicy::resolve(strategy, inner.selected_mode.clone())
                .map_err(|_| SessionExecutionStateError::PolicyUnresolved)?;
        inner.strategy = strategy;
        inner.resolved_orchestration_policy = resolved_orchestration_policy;
        inner.validation = SessionPolicyValidation::Unresolved;
        Ok(())
    }

    pub fn strategy(&self) -> Result<OrchestrationMode, SessionExecutionStateError> {
        Ok(self.lock()?.strategy)
    }

    pub fn strategy_availability(
        &self,
    ) -> Result<OrchestrationStrategyAvailability, SessionExecutionStateError> {
        Ok(self.lock()?.resolved_orchestration_policy.availability())
    }

    pub fn resolved_orchestration_policy(
        &self,
    ) -> Result<ResolvedOrchestrationPolicy, SessionExecutionStateError> {
        Ok(self.lock()?.resolved_orchestration_policy.clone())
    }

    pub fn select_routing_profile(
        &self,
        profile_id: RoutingProfileId,
    ) -> Result<(), SessionExecutionStateError> {
        let mut inner = self.lock()?;
        ensure_idle(&inner, false)?;
        inner.routing_profile_id = Some(profile_id);
        inner.validation = SessionPolicyValidation::Unresolved;
        Ok(())
    }

    pub fn selected_mode(&self) -> Result<ExecutionModeSelection, SessionExecutionStateError> {
        Ok(self.lock()?.selected_mode.clone())
    }

    pub fn resolved_policy(&self) -> Result<ResolvedExecutionPolicy, SessionExecutionStateError> {
        Ok(self.lock()?.resolved_policy.clone())
    }

    pub fn routing_profile_id(
        &self,
    ) -> Result<Option<RoutingProfileId>, SessionExecutionStateError> {
        Ok(self.lock()?.routing_profile_id.clone())
    }

    pub fn policy_source(&self) -> Result<SessionPolicySource, SessionExecutionStateError> {
        Ok(self.lock()?.source)
    }

    pub fn validation(&self) -> Result<SessionPolicyValidation, SessionExecutionStateError> {
        Ok(self.lock()?.validation.clone())
    }

    pub fn status(&self) -> Result<SessionExecutionStatus, SessionExecutionStateError> {
        Ok(self.lock()?.status)
    }

    /// Explicitly prepares a terminal session for a later run.
    pub fn reset_to_idle(&self) -> Result<(), SessionExecutionStateError> {
        let mut inner = self.lock()?;
        if matches!(
            inner.status,
            SessionExecutionStatus::Preparing
                | SessionExecutionStatus::Validating
                | SessionExecutionStatus::Running
                | SessionExecutionStatus::Cancelling
        ) {
            return Err(SessionExecutionStateError::ResetWhileActive);
        }
        if inner.status == SessionExecutionStatus::Idle {
            if inner.active_generation.is_some() {
                return Err(SessionExecutionStateError::ResetWhileCleanupPending);
            }
            return Ok(());
        }
        if !matches!(
            inner.status,
            SessionExecutionStatus::Completed
                | SessionExecutionStatus::Failed
                | SessionExecutionStatus::Cancelled
                | SessionExecutionStatus::TimedOut
        ) {
            return Err(SessionExecutionStateError::ResetWhileCleanupPending);
        }
        inner.status = SessionExecutionStatus::Idle;
        inner.validation = SessionPolicyValidation::Unresolved;
        inner.active_generation = None;
        Ok(())
    }

    pub fn policy_summary(&self) -> Result<SessionPolicySummary, SessionExecutionStateError> {
        let inner = self.lock()?;
        let policy = inner.resolved_policy.policy();
        Ok(SessionPolicySummary {
            strategy: inner.strategy,
            strategy_availability: inner.resolved_orchestration_policy.availability(),
            selected_mode: inner.selected_mode.clone(),
            resolved_mode_id: resolved_mode_id(&inner.resolved_policy),
            built_in: matches!(inner.resolved_policy.source(), PolicySource::BuiltIn(_)),
            source: inner.source,
            routing_profile_id: inner.routing_profile_id.clone(),
            enabled_roles: policy
                .roles
                .iter()
                .filter(|(_, role)| role.activation != super::RoleActivation::Disabled)
                .map(|(role, _)| *role)
                .collect(),
            disabled_roles: policy
                .roles
                .iter()
                .filter(|(_, role)| role.activation == super::RoleActivation::Disabled)
                .map(|(role, _)| *role)
                .collect(),
            optional_roles: policy
                .roles
                .iter()
                .filter(|(_, role)| role.activation == super::RoleActivation::Optional)
                .map(|(role, _)| *role)
                .collect(),
            efforts: policy
                .roles
                .iter()
                .map(|(role, value)| (*role, value.effort.clone()))
                .collect(),
            max_subagents: policy.max_subagents,
            max_concurrency: policy.max_concurrency,
            repair_enabled: inner.resolved_policy.role(RoutingRole::Repair).activation
                != super::RoleActivation::Disabled,
            verifier_enabled: inner.resolved_policy.role(RoutingRole::Verifier).activation
                != super::RoleActivation::Disabled,
            planner_enabled: inner.resolved_policy.role(RoutingRole::Planner).activation
                != super::RoleActivation::Disabled,
            task_timeout: policy.task_timeout,
            batch_timeout: policy.batch_timeout,
            repair_timeout: policy.repair_timeout,
            provider_invocation_budget: policy.max_provider_invocations,
            tool_call_budget: policy.max_tool_calls,
            context_budget_bytes: policy.context_budget_bytes,
            output_budget_tokens: policy.output_budget_tokens,
            validation: inner.validation.clone(),
            status: inner.status,
        })
    }

    pub(crate) fn transition(
        &self,
        next: SessionExecutionStatus,
    ) -> Result<(), SessionExecutionStateError> {
        let mut inner = self.lock()?;
        if !valid_transition(inner.status, next) {
            return Err(SessionExecutionStateError::InvalidTransition {
                from: inner.status,
                to: next,
            });
        }
        inner.status = next;
        Ok(())
    }

    pub(crate) fn begin_run(&self) -> Result<u64, SessionExecutionStateError> {
        let mut inner = self.lock()?;
        if inner.status != SessionExecutionStatus::Idle {
            return Err(SessionExecutionStateError::RunAlreadyActive);
        }
        if !valid_transition(inner.status, SessionExecutionStatus::Preparing) {
            return Err(SessionExecutionStateError::InvalidTransition {
                from: inner.status,
                to: SessionExecutionStatus::Preparing,
            });
        }
        inner.next_generation = inner
            .next_generation
            .checked_add(1)
            .ok_or(SessionExecutionStateError::StaleRunGeneration)?;
        inner.active_generation = Some(inner.next_generation);
        inner.status = SessionExecutionStatus::Preparing;
        Ok(inner.next_generation)
    }

    pub(crate) fn terminalize_generation(
        &self,
        generation: u64,
        next: SessionExecutionStatus,
    ) -> Result<(), SessionExecutionStateError> {
        let mut inner = self.lock()?;
        if inner.active_generation != Some(generation) {
            return Err(SessionExecutionStateError::StaleRunGeneration);
        }
        if !valid_transition(inner.status, next) {
            return Err(SessionExecutionStateError::InvalidTransition {
                from: inner.status,
                to: next,
            });
        }
        inner.status = next;
        if matches!(
            next,
            SessionExecutionStatus::Completed
                | SessionExecutionStatus::Failed
                | SessionExecutionStatus::Cancelled
                | SessionExecutionStatus::TimedOut
        ) {
            inner.active_generation = None;
        }
        Ok(())
    }

    pub(crate) fn mark_policy_valid(&self) -> Result<(), SessionExecutionStateError> {
        let mut inner = self.lock()?;
        inner.validation = SessionPolicyValidation::Valid;
        Ok(())
    }

    pub(crate) fn mark_policy_invalid(
        &self,
        error: ExecutionPolicyError,
    ) -> Result<(), SessionExecutionStateError> {
        let mut inner = self.lock()?;
        inner.validation = SessionPolicyValidation::Invalid(error);
        Ok(())
    }

    pub(crate) fn mark_timed_out_after_cleanup(
        &self,
        generation: u64,
    ) -> Result<(), SessionExecutionStateError> {
        let mut inner = self.lock()?;
        if inner.active_generation != Some(generation)
            && !(inner.active_generation.is_none()
                && inner.next_generation == generation
                && inner.status == SessionExecutionStatus::Cancelled)
        {
            return Err(SessionExecutionStateError::StaleRunGeneration);
        }
        if !matches!(
            inner.status,
            SessionExecutionStatus::Running | SessionExecutionStatus::Cancelled
        ) {
            return Err(SessionExecutionStateError::InvalidTransition {
                from: inner.status,
                to: SessionExecutionStatus::TimedOut,
            });
        }
        inner.status = SessionExecutionStatus::TimedOut;
        inner.active_generation = None;
        Ok(())
    }

    fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, SessionExecutionInner>, SessionExecutionStateError> {
        self.inner
            .lock()
            .map_err(|_| SessionExecutionStateError::LockUnavailable)
    }
}

/// Safe, bounded inspection data for the active session policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionPolicySummary {
    pub strategy: OrchestrationMode,
    pub strategy_availability: OrchestrationStrategyAvailability,
    pub selected_mode: ExecutionModeSelection,
    pub resolved_mode_id: String,
    pub built_in: bool,
    pub source: SessionPolicySource,
    pub routing_profile_id: Option<RoutingProfileId>,
    pub enabled_roles: Vec<RoutingRole>,
    pub disabled_roles: Vec<RoutingRole>,
    pub optional_roles: Vec<RoutingRole>,
    pub efforts: BTreeMap<RoutingRole, codex_protocol::openai_models::ReasoningEffort>,
    pub max_subagents: usize,
    pub max_concurrency: usize,
    pub repair_enabled: bool,
    pub verifier_enabled: bool,
    pub planner_enabled: bool,
    pub task_timeout: std::time::Duration,
    pub batch_timeout: std::time::Duration,
    pub repair_timeout: std::time::Duration,
    pub provider_invocation_budget: usize,
    pub tool_call_budget: usize,
    pub context_budget_bytes: usize,
    pub output_budget_tokens: u32,
    pub validation: SessionPolicyValidation,
    pub status: SessionExecutionStatus,
}

fn resolved_mode_id(policy: &ResolvedExecutionPolicy) -> String {
    match policy.selected_mode() {
        ExecutionModeSelection::Fast => "fast".to_string(),
        ExecutionModeSelection::Balanced => "balanced".to_string(),
        ExecutionModeSelection::UsageSaver => "usage_saver".to_string(),
        ExecutionModeSelection::Deep => "deep".to_string(),
        ExecutionModeSelection::Custom(_) => "custom".to_string(),
    }
}

fn ensure_idle(
    inner: &SessionExecutionInner,
    policy: bool,
) -> Result<(), SessionExecutionStateError> {
    if inner.status != SessionExecutionStatus::Idle {
        return Err(if policy {
            SessionExecutionStateError::PolicyMutationWhileActive
        } else {
            SessionExecutionStateError::RoutingMutationWhileActive
        });
    }
    Ok(())
}

fn valid_transition(from: SessionExecutionStatus, to: SessionExecutionStatus) -> bool {
    matches!(
        (from, to),
        (
            SessionExecutionStatus::Idle,
            SessionExecutionStatus::Preparing
        ) | (
            SessionExecutionStatus::Preparing,
            SessionExecutionStatus::Validating
        ) | (
            SessionExecutionStatus::Validating,
            SessionExecutionStatus::Running
        ) | (
            SessionExecutionStatus::Validating,
            SessionExecutionStatus::Failed
        ) | (
            SessionExecutionStatus::Validating,
            SessionExecutionStatus::Cancelling
        ) | (
            SessionExecutionStatus::Running,
            SessionExecutionStatus::Completed
        ) | (
            SessionExecutionStatus::Running,
            SessionExecutionStatus::Failed
        ) | (
            SessionExecutionStatus::Running,
            SessionExecutionStatus::Cancelling
        ) | (
            SessionExecutionStatus::Running,
            SessionExecutionStatus::TimedOut
        ) | (
            SessionExecutionStatus::Cancelling,
            SessionExecutionStatus::Cancelled
        )
    )
}
