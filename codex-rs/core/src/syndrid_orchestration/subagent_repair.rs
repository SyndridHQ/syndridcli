use super::routing_profiles::RoutingRole;
use super::subagent::SubagentError;
use super::subagent::SubagentOutcome;
use super::subagent::SubagentProvider;
use super::subagent::SubagentRequest;
use super::subagent::SubagentRuntime;
use super::subagent::SubagentStatus;
use super::subagent_batch::SubagentFailurePolicy;
use std::fmt;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub const SUBAGENT_REPAIR_MAX_ATTEMPTS: u8 = 1;
pub const SUBAGENT_REPAIR_MAX_CONTEXT_BYTES: usize = 16 * 1024;
pub const SUBAGENT_REPAIR_MAX_OUTPUT_TOKENS: u32 = 4_000;
pub const SUBAGENT_REPAIR_MAX_CONCURRENCY: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentRepairFailureCategory {
    VerifierRejected,
    StructurallyInvalid,
    MissingRequiredContent,
    ResultContractViolation,
    ToolResultFailure,
    Cancelled,
    Timeout,
    InvalidRoute,
    AuthenticationFailure,
    ProviderUnavailable,
    ProviderBudgetExhausted,
    ToolBudgetExhausted,
    PolicyInvalid,
    RecursionViolation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentRepairEligibility {
    Eligible(SubagentRepairFailureCategory),
    Ineligible(SubagentRepairFailureCategory),
}

#[derive(Clone, Eq, PartialEq)]
pub struct SubagentRepairRoute {
    pub profile_id: String,
    pub role: RoutingRole,
    pub provider_id: String,
    pub connection_id: String,
    pub model_id: String,
}

impl fmt::Debug for SubagentRepairRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubagentRepairRoute")
            .field("profile_id_bytes", &self.profile_id.len())
            .field("role", &self.role)
            .field("provider_id_bytes", &self.provider_id.len())
            .field("connection_id_bytes", &self.connection_id.len())
            .field("model_id_bytes", &self.model_id.len())
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubagentRepairPolicy {
    pub enabled: bool,
    pub max_repair_attempts: u8,
    pub route: SubagentRepairRoute,
    pub per_repair_timeout: Duration,
    pub total_repair_timeout: Duration,
    pub max_provider_invocations: usize,
    pub max_tool_calls: usize,
    pub max_context_bytes: usize,
    pub max_output_tokens: u32,
}

impl SubagentRepairPolicy {
    pub fn validate(&self) -> Result<(), SubagentRepairError> {
        if self.max_repair_attempts > SUBAGENT_REPAIR_MAX_ATTEMPTS
            || (self.enabled && self.max_repair_attempts == 0)
            || self.route.role != RoutingRole::Repair
            || self.route.profile_id.is_empty()
            || self.route.provider_id.is_empty()
            || self.route.connection_id.is_empty()
            || self.route.model_id.is_empty()
            || self.per_repair_timeout.is_zero()
            || self.total_repair_timeout.is_zero()
            || self.per_repair_timeout > self.total_repair_timeout
            || self.max_provider_invocations == 0
            || self.max_tool_calls == 0
            || self.max_context_bytes == 0
            || self.max_context_bytes > SUBAGENT_REPAIR_MAX_CONTEXT_BYTES
            || self.max_output_tokens == 0
            || self.max_output_tokens > SUBAGENT_REPAIR_MAX_OUTPUT_TOKENS
        {
            return Err(SubagentRepairError::PolicyInvalid);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentAttemptKind {
    Initial,
    Repair,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentAttemptState {
    Started,
    Succeeded,
    Failed,
    Rejected,
    TimedOut,
    Cancelled,
    BudgetExhausted,
}

#[derive(Clone, Eq, PartialEq)]
pub struct SubagentAttemptOutcome {
    pub attempt_number: u8,
    pub kind: SubagentAttemptKind,
    pub state: SubagentAttemptState,
    pub route: SubagentRepairRoute,
    pub outcome: Option<SubagentOutcome>,
    pub error: Option<SubagentError>,
    pub provider_invocations: usize,
    pub tool_calls: usize,
    pub timed_out: bool,
    pub cancelled: bool,
    pub terminal_failure: Option<SubagentRepairFailureCategory>,
    pub repair_permitted: bool,
    pub repair_budget_exhausted: bool,
}

impl fmt::Debug for SubagentAttemptOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubagentAttemptOutcome")
            .field("attempt_number", &self.attempt_number)
            .field("kind", &self.kind)
            .field("state", &self.state)
            .field("route", &self.route)
            .field("has_outcome", &self.outcome.is_some())
            .field("error", &self.error)
            .field("provider_invocations", &self.provider_invocations)
            .field("tool_calls", &self.tool_calls)
            .field("timed_out", &self.timed_out)
            .field("cancelled", &self.cancelled)
            .field("terminal_failure", &self.terminal_failure)
            .field("repair_permitted", &self.repair_permitted)
            .field("repair_budget_exhausted", &self.repair_budget_exhausted)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentRepairTerminal {
    InitialSucceeded,
    RepairSucceeded,
    RepairDisabled,
    NotEligible,
    InitialFailed,
    RepairFailed,
    RepairTimedOut,
    Cancelled,
    BudgetExhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubagentRepairOutcome {
    pub task_id: String,
    pub terminal: SubagentRepairTerminal,
    pub attempts: Vec<SubagentAttemptOutcome>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentRepairError {
    PolicyInvalid,
    InitialValidationFailed(SubagentError),
    RouteMismatch,
    BudgetExhausted,
    CancelledBeforeRepair,
    JoinFailure,
    BatchInvalid,
}

impl fmt::Display for SubagentRepairError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PolicyInvalid => formatter.write_str("subagent repair policy is invalid"),
            Self::InitialValidationFailed(error) => error.fmt(formatter),
            Self::RouteMismatch => {
                formatter.write_str("repair route did not match the selected route")
            }
            Self::BudgetExhausted => formatter.write_str("subagent repair budget is exhausted"),
            Self::CancelledBeforeRepair => {
                formatter.write_str("repair was cancelled before starting")
            }
            Self::JoinFailure => formatter.write_str("repair child task join failed"),
            Self::BatchInvalid => formatter.write_str("subagent repair batch is invalid"),
        }
    }
}

impl std::error::Error for SubagentRepairError {}

#[derive(Clone)]
pub struct SubagentRepairBudget {
    state: Arc<Mutex<BudgetState>>,
}

#[derive(Clone, Copy)]
struct BudgetState {
    provider_invocations: usize,
    tool_calls: usize,
    context_bytes: usize,
    output_tokens: u32,
    limits: (usize, usize, usize, u32),
}

impl SubagentRepairBudget {
    pub fn new(
        max_provider_invocations: usize,
        max_tool_calls: usize,
        max_context_bytes: usize,
        max_output_tokens: u32,
    ) -> Result<Self, SubagentRepairError> {
        if max_provider_invocations == 0
            || max_tool_calls == 0
            || max_context_bytes == 0
            || max_output_tokens == 0
        {
            return Err(SubagentRepairError::BudgetExhausted);
        }
        Ok(Self {
            state: Arc::new(Mutex::new(BudgetState {
                provider_invocations: 0,
                tool_calls: 0,
                context_bytes: 0,
                output_tokens: 0,
                limits: (
                    max_provider_invocations,
                    max_tool_calls,
                    max_context_bytes,
                    max_output_tokens,
                ),
            })),
        })
    }

    fn reserve(&self, policy: &SubagentRepairPolicy) -> Option<BudgetReservation> {
        let mut state = self.state.lock().expect("repair budget mutex poisoned");
        let context = policy.max_context_bytes;
        let output = policy.max_output_tokens;
        let provider = policy.max_provider_invocations;
        let tools = policy.max_tool_calls;
        if state.provider_invocations.saturating_add(provider) > state.limits.0
            || state.tool_calls.saturating_add(tools) > state.limits.1
            || state.context_bytes.saturating_add(context) > state.limits.2
            || state.output_tokens.saturating_add(output) > state.limits.3
        {
            return None;
        }
        state.provider_invocations += provider;
        state.tool_calls += tools;
        state.context_bytes += context;
        state.output_tokens += output;
        Some(BudgetReservation {
            budget: self.clone(),
            provider,
            tools,
            context,
            output,
        })
    }
}

struct BudgetReservation {
    budget: SubagentRepairBudget,
    provider: usize,
    tools: usize,
    context: usize,
    output: u32,
}

impl Drop for BudgetReservation {
    fn drop(&mut self) {
        let mut state = self
            .budget
            .state
            .lock()
            .expect("repair budget mutex poisoned");
        state.provider_invocations -= self.provider;
        state.tool_calls -= self.tools;
        state.context_bytes -= self.context;
        state.output_tokens -= self.output;
    }
}

pub struct SubagentRepairRuntime<P> {
    runtime: Arc<SubagentRuntime<P>>,
    budget: SubagentRepairBudget,
}

impl<P> SubagentRepairRuntime<P> {
    pub fn new(runtime: SubagentRuntime<P>, budget: SubagentRepairBudget) -> Self {
        Self {
            runtime: Arc::new(runtime),
            budget,
        }
    }
}

impl<P: SubagentProvider + 'static> SubagentRepairRuntime<P> {
    pub async fn run(
        &self,
        initial: SubagentRequest,
        policy: SubagentRepairPolicy,
        eligibility: SubagentRepairEligibility,
        rejection_reason: String,
        repair_instruction: String,
    ) -> Result<SubagentRepairOutcome, SubagentRepairError> {
        policy.validate()?;
        let task_id = initial.task_id.clone();
        let initial_route = SubagentRepairRoute {
            profile_id: policy.route.profile_id.clone(),
            role: initial.role,
            provider_id: policy.route.provider_id.clone(),
            connection_id: policy.route.connection_id.clone(),
            model_id: policy.route.model_id.clone(),
        };
        let initial_result = self.runtime.run_subagent(initial.clone()).await;
        let initial_attempt = attempt(
            1,
            SubagentAttemptKind::Initial,
            initial_route,
            initial_result,
        );
        if initial_attempt.state == SubagentAttemptState::Succeeded {
            return Ok(SubagentRepairOutcome {
                task_id,
                terminal: SubagentRepairTerminal::InitialSucceeded,
                attempts: vec![initial_attempt],
            });
        }
        let mut attempts = vec![initial_attempt];
        let eligible = matches!(eligibility, SubagentRepairEligibility::Eligible(_));
        if !policy.enabled {
            return Ok(terminal(
                task_id,
                SubagentRepairTerminal::RepairDisabled,
                attempts,
            ));
        }
        if !eligible {
            return Ok(terminal(
                task_id,
                SubagentRepairTerminal::NotEligible,
                attempts,
            ));
        }
        if initial.cancellation.is_cancelled() {
            return Ok(terminal(
                task_id,
                SubagentRepairTerminal::Cancelled,
                attempts,
            ));
        }
        let context = repair_context(
            &initial.instruction,
            attempts[0]
                .outcome
                .as_ref()
                .and_then(|outcome| outcome.output.as_deref()),
            &rejection_reason,
            &repair_instruction,
            policy.max_context_bytes,
        );
        let repair_cancellation = initial.cancellation.child_token();
        let mut repair = initial;
        repair.role = policy.route.role;
        repair.instruction = repair_instruction;
        repair.context = Some(context);
        repair.timeout = policy.per_repair_timeout;
        repair.max_output_tokens = policy.max_output_tokens;
        repair.cancellation = repair_cancellation.clone();
        repair.tool_policy = repair
            .tool_policy
            .with_repair_limits(policy.max_provider_invocations, policy.max_tool_calls);

        let Ok(resolved_route) = self.runtime.resolved_route(&repair) else {
            let mut failed = attempt(
                2,
                SubagentAttemptKind::Repair,
                policy.route.clone(),
                Err(SubagentError::InvalidModel),
            );
            failed.terminal_failure = Some(SubagentRepairFailureCategory::InvalidRoute);
            attempts.push(failed);
            return Ok(terminal(
                task_id,
                SubagentRepairTerminal::RepairFailed,
                attempts,
            ));
        };
        let actual_route = route_from_parts(resolved_route);
        if actual_route != policy.route {
            let mut failed = attempt(
                2,
                SubagentAttemptKind::Repair,
                actual_route,
                Err(SubagentError::InternalFailure),
            );
            failed.terminal_failure = Some(SubagentRepairFailureCategory::InvalidRoute);
            attempts.push(failed);
            return Ok(terminal(
                task_id,
                SubagentRepairTerminal::RepairFailed,
                attempts,
            ));
        }
        if repair_cancellation.is_cancelled() {
            attempts[0].cancelled = true;
            attempts[0].repair_permitted = false;
            return Ok(terminal(
                task_id,
                SubagentRepairTerminal::Cancelled,
                attempts,
            ));
        }
        let Some(_reservation) = self.budget.reserve(&policy) else {
            attempts[0].repair_budget_exhausted = true;
            return Ok(terminal(
                task_id,
                SubagentRepairTerminal::BudgetExhausted,
                attempts,
            ));
        };
        attempts[0].repair_permitted = true;
        let repair_future = self.runtime.run_subagent(repair);
        tokio::pin!(repair_future);
        let result = tokio::time::timeout(policy.total_repair_timeout, &mut repair_future).await;
        let repair_result = match result {
            Ok(result) => result,
            Err(_) => {
                repair_cancellation.cancel();
                let _ = repair_future.await;
                Err(SubagentError::InvocationTimedOut)
            }
        };
        let repair_attempt = attempt(
            2,
            SubagentAttemptKind::Repair,
            policy.route.clone(),
            repair_result,
        );
        let terminal_state = match repair_attempt.state {
            SubagentAttemptState::Succeeded => SubagentRepairTerminal::RepairSucceeded,
            SubagentAttemptState::TimedOut => SubagentRepairTerminal::RepairTimedOut,
            SubagentAttemptState::Cancelled => SubagentRepairTerminal::Cancelled,
            SubagentAttemptState::BudgetExhausted => SubagentRepairTerminal::BudgetExhausted,
            _ => SubagentRepairTerminal::RepairFailed,
        };
        attempts.push(repair_attempt);
        Ok(terminal(task_id, terminal_state, attempts))
    }
}

fn terminal(
    task_id: String,
    terminal: SubagentRepairTerminal,
    attempts: Vec<SubagentAttemptOutcome>,
) -> SubagentRepairOutcome {
    SubagentRepairOutcome {
        task_id,
        terminal,
        attempts,
    }
}

fn attempt(
    attempt_number: u8,
    kind: SubagentAttemptKind,
    route: SubagentRepairRoute,
    result: Result<SubagentOutcome, SubagentError>,
) -> SubagentAttemptOutcome {
    let (outcome, error) = match result {
        Ok(outcome) => (Some(outcome), None),
        Err(error) => (None, Some(error)),
    };
    let status = outcome.as_ref().map(|outcome| outcome.status);
    let cancelled = matches!(status, Some(SubagentStatus::Cancelled))
        || matches!(
            error,
            Some(SubagentError::CancelledBeforeStart | SubagentError::InvocationCancelled)
        );
    let timed_out = matches!(status, Some(SubagentStatus::TimedOut))
        || matches!(error, Some(SubagentError::InvocationTimedOut));
    let state = if cancelled {
        SubagentAttemptState::Cancelled
    } else if timed_out {
        SubagentAttemptState::TimedOut
    } else if matches!(
        status,
        Some(SubagentStatus::Completed | SubagentStatus::CompletedWithTruncation)
    ) {
        SubagentAttemptState::Succeeded
    } else if matches!(status, Some(SubagentStatus::ProviderRejected)) {
        SubagentAttemptState::Rejected
    } else if matches!(status, Some(SubagentStatus::BudgetExhausted)) {
        SubagentAttemptState::BudgetExhausted
    } else {
        SubagentAttemptState::Failed
    };
    let provider_invocations = outcome.as_ref().map_or(0, |outcome| outcome.provider_turns);
    let tool_calls = outcome.as_ref().map_or(0, |outcome| outcome.tool_calls);
    let budget_exhausted = outcome
        .as_ref()
        .is_some_and(|outcome| outcome.budget_exhausted);
    SubagentAttemptOutcome {
        attempt_number,
        kind,
        state,
        route,
        outcome,
        error,
        provider_invocations,
        tool_calls,
        timed_out,
        cancelled,
        terminal_failure: None,
        repair_permitted: false,
        repair_budget_exhausted: budget_exhausted,
    }
}

fn route_from_parts(
    (profile_id, provider_id, connection_id, model_id, role): (
        String,
        String,
        String,
        String,
        RoutingRole,
    ),
) -> SubagentRepairRoute {
    SubagentRepairRoute {
        profile_id,
        role,
        provider_id,
        connection_id,
        model_id,
    }
}

fn repair_context(
    instruction: &str,
    output: Option<&str>,
    reason: &str,
    repair_instruction: &str,
    limit: usize,
) -> String {
    let mut context = format!(
        "Original task:\n{instruction}\n\nInitial result:\n{}\n\nRejection reason:\n{reason}\n\nRepair instruction:\n{repair_instruction}",
        output.unwrap_or("<no result>")
    );
    if context.len() > limit {
        let mut end = limit;
        while !context.is_char_boundary(end) {
            end -= 1;
        }
        context.truncate(end);
    }
    context
}

pub struct SubagentRepairBatchRequest<P> {
    pub tasks: Vec<(
        SubagentRequest,
        SubagentRepairPolicy,
        SubagentRepairEligibility,
        String,
        String,
    )>,
    pub max_concurrency: usize,
    pub failure_policy: SubagentFailurePolicy,
    pub cancellation: CancellationToken,
    pub runtime: Arc<SubagentRepairRuntime<P>>,
}

impl<P> fmt::Debug for SubagentRepairBatchRequest<P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubagentRepairBatchRequest")
            .field("task_count", &self.tasks.len())
            .field("max_concurrency", &self.max_concurrency)
            .field("failure_policy", &self.failure_policy)
            .field("cancelled", &self.cancellation.is_cancelled())
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubagentRepairBatchOutcome {
    pub outcomes: Vec<Result<SubagentRepairOutcome, SubagentRepairError>>,
    pub peak_observed_concurrency: usize,
}

pub struct SubagentRepairBatchRuntime;

struct ActiveRepairTaskGuard {
    active: Arc<AtomicUsize>,
}

impl Drop for ActiveRepairTaskGuard {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::Relaxed);
    }
}

impl SubagentRepairBatchRuntime {
    pub async fn run<P: SubagentProvider + 'static>(
        request: SubagentRepairBatchRequest<P>,
    ) -> Result<SubagentRepairBatchOutcome, SubagentRepairError> {
        if request.tasks.is_empty()
            || request.max_concurrency == 0
            || request.max_concurrency > SUBAGENT_REPAIR_MAX_CONCURRENCY
        {
            return Err(SubagentRepairError::BatchInvalid);
        }
        let total = request.tasks.len();
        let mut outcomes: Vec<Option<Result<SubagentRepairOutcome, SubagentRepairError>>> =
            (0..total).map(|_| None).collect();
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut join_set = JoinSet::new();
        let mut next = 0;
        let mut cancelled = false;
        while next < total && !cancelled && !request.cancellation.is_cancelled() {
            while next < total
                && join_set.len() < request.max_concurrency
                && !request.cancellation.is_cancelled()
            {
                let index = next;
                let task = request.tasks[index].clone();
                let runtime = request.runtime.clone();
                let active_count = active.clone();
                let peak_count = peak.clone();
                let cancellation = request.cancellation.child_token();
                join_set.spawn(async move {
                    let _active = ActiveRepairTaskGuard {
                        active: active_count.clone(),
                    };
                    let current = active_count.fetch_add(1, Ordering::Relaxed) + 1;
                    peak_count.fetch_max(current, Ordering::Relaxed);
                    let mut initial = task.0;
                    initial.cancellation = cancellation;
                    let result = runtime.run(initial, task.1, task.2, task.3, task.4).await;
                    (index, result)
                });
                next += 1;
            }
            let Some(joined) = join_set.join_next().await else {
                break;
            };
            let (index, result) = joined.map_err(|_| SubagentRepairError::JoinFailure)?;
            let failed = result
                .as_ref()
                .map(|outcome| {
                    !matches!(
                        outcome.terminal,
                        SubagentRepairTerminal::InitialSucceeded
                            | SubagentRepairTerminal::RepairSucceeded
                    )
                })
                .unwrap_or(true);
            outcomes[index] = Some(result);
            if failed && request.failure_policy == SubagentFailurePolicy::CancelRemaining {
                cancelled = true;
                request.cancellation.cancel();
            }
        }
        request.cancellation.cancel();
        while let Some(joined) = join_set.join_next().await {
            let (index, result) = joined.map_err(|_| SubagentRepairError::JoinFailure)?;
            outcomes[index] = Some(result);
        }
        for outcome in outcomes.iter_mut().skip(next) {
            *outcome = Some(Err(SubagentRepairError::CancelledBeforeRepair));
        }
        Ok(SubagentRepairBatchOutcome {
            outcomes: outcomes.into_iter().map(Option::unwrap).collect(),
            peak_observed_concurrency: peak.load(Ordering::Relaxed),
        })
    }
}
