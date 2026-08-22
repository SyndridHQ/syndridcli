use super::ResolvedExecutionPolicy;
use super::RoutingRole;
use std::fmt;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

#[path = "execution_budget_reservations.rs"]
mod execution_budget_reservations;
pub use super::execution_budget_accounting::BudgetExhaustion;
pub use super::execution_budget_accounting::BudgetExhaustionCategory;
pub use super::execution_budget_accounting::ExecutionBudgetSnapshot;
pub(crate) use execution_budget_reservations::ProviderInvocationTerminal;
pub use execution_budget_reservations::ProviderReservation;
pub use execution_budget_reservations::ToolReservation;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionBudgetLimits {
    pub max_provider_invocations: usize,
    pub max_planner_provider_invocations: usize,
    pub max_executor_provider_invocations: usize,
    pub max_verifier_provider_invocations: usize,
    pub max_repair_provider_invocations: usize,
    pub max_tool_calls: usize,
    pub max_tool_output_bytes: usize,
    pub max_context_bytes: usize,
    pub max_output_tokens: u32,
    pub max_executor_tasks: usize,
    pub max_executor_concurrency: usize,
    pub max_repair_attempts: usize,
    pub max_elapsed: Duration,
    pub max_repair_elapsed: Duration,
    pub max_depth: u8,
}

impl ExecutionBudgetLimits {
    pub fn from_resolved(policy: &ResolvedExecutionPolicy) -> Self {
        let value = policy.policy();
        let provider_limit = value.max_provider_invocations;
        let repair_limit = usize::from(value.max_repair_attempts);
        Self {
            max_provider_invocations: provider_limit,
            max_planner_provider_invocations: provider_limit,
            max_executor_provider_invocations: provider_limit,
            max_verifier_provider_invocations: provider_limit,
            max_repair_provider_invocations: provider_limit,
            max_tool_calls: value.max_tool_calls,
            max_tool_output_bytes: value.max_tool_output_bytes,
            max_context_bytes: value.context_budget_bytes,
            max_output_tokens: value.output_budget_tokens,
            max_executor_tasks: value.max_subagents,
            max_executor_concurrency: value.max_concurrency,
            max_repair_attempts: repair_limit,
            max_elapsed: value.batch_timeout,
            max_repair_elapsed: value.repair_timeout,
            max_depth: 1,
        }
    }

    fn provider_limit(&self, role: RoutingRole) -> (usize, BudgetExhaustionCategory) {
        match role {
            RoutingRole::Main => (
                self.max_provider_invocations,
                BudgetExhaustionCategory::TotalProviderInvocations,
            ),
            RoutingRole::Planner => (
                self.max_planner_provider_invocations,
                BudgetExhaustionCategory::PlannerProviderInvocations,
            ),
            RoutingRole::Executor => (
                self.max_executor_provider_invocations,
                BudgetExhaustionCategory::ExecutorProviderInvocations,
            ),
            RoutingRole::Verifier => (
                self.max_verifier_provider_invocations,
                BudgetExhaustionCategory::VerifierProviderInvocations,
            ),
            RoutingRole::Repair => (
                self.max_repair_provider_invocations,
                BudgetExhaustionCategory::RepairProviderInvocations,
            ),
        }
    }
}

#[derive(Clone)]
pub struct ExecutionBudgetLedger {
    state: Arc<Mutex<LedgerState>>,
    started_at: Instant,
    limits: ExecutionBudgetLimits,
    generation: u64,
    last_exhaustion: Arc<Mutex<Option<BudgetExhaustion>>>,
}

struct LedgerState {
    provider_reserved: usize,
    provider_started: usize,
    provider_completed: usize,
    provider_cancelled: usize,
    provider_failed: usize,
    provider_timed_out: usize,
    provider_rejected: usize,
    provider_by_role: std::collections::BTreeMap<RoutingRole, usize>,
    tool_reserved: usize,
    tool_started: usize,
    tool_completed: usize,
    tool_rejected: usize,
    tool_output_bytes: usize,
    context_bytes_consumed: usize,
    output_tokens_consumed: u64,
    executor_tasks_admitted: usize,
    repair_attempts_admitted: usize,
    elapsed_exhausted: bool,
    terminal: bool,
}

impl ExecutionBudgetLedger {
    pub fn new(policy: &ResolvedExecutionPolicy) -> Self {
        Self::new_for_generation(policy, 0)
    }

    pub fn new_for_generation(policy: &ResolvedExecutionPolicy, generation: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(LedgerState {
                provider_reserved: 0,
                provider_started: 0,
                provider_completed: 0,
                provider_cancelled: 0,
                provider_failed: 0,
                provider_timed_out: 0,
                provider_rejected: 0,
                provider_by_role: std::collections::BTreeMap::new(),
                tool_reserved: 0,
                tool_started: 0,
                tool_completed: 0,
                tool_rejected: 0,
                tool_output_bytes: 0,
                context_bytes_consumed: 0,
                output_tokens_consumed: 0,
                executor_tasks_admitted: 0,
                repair_attempts_admitted: 0,
                elapsed_exhausted: false,
                terminal: false,
            })),
            started_at: Instant::now(),
            limits: ExecutionBudgetLimits::from_resolved(policy),
            generation,
            last_exhaustion: Arc::new(Mutex::new(None)),
        }
    }

    pub fn limits(&self) -> &ExecutionBudgetLimits {
        &self.limits
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn reserve_provider(
        &self,
        role: RoutingRole,
    ) -> Result<ProviderReservation, BudgetExhaustion> {
        let mut state = self.lock()?;
        if !state.terminal {
            self.check_elapsed(&mut state)?;
        }
        if state.terminal {
            return Err(self.exhaustion(
                BudgetExhaustionCategory::RunTerminal,
                self.limits.max_provider_invocations as u64,
                (state.provider_started + state.provider_reserved) as u64,
                Some(role),
            ));
        }
        let (role_limit, role_category) = self.limits.provider_limit(role);
        let role_used = state.provider_by_role.get(&role).copied().unwrap_or(0);
        if state.provider_started + state.provider_reserved >= self.limits.max_provider_invocations
        {
            state.provider_rejected += 1;
            return Err(self.exhaustion(
                BudgetExhaustionCategory::TotalProviderInvocations,
                self.limits.max_provider_invocations as u64,
                (state.provider_started + state.provider_reserved) as u64,
                Some(role),
            ));
        }
        if role_used >= role_limit {
            state.provider_rejected += 1;
            return Err(self.exhaustion(
                role_category,
                role_limit as u64,
                role_used as u64,
                Some(role),
            ));
        }
        state.provider_reserved += 1;
        *state.provider_by_role.entry(role).or_default() += 1;
        Ok(ProviderReservation {
            ledger: self.clone(),
            role,
            committed: false,
        })
    }

    pub fn reserve_tool(&self, role: RoutingRole) -> Result<ToolReservation, BudgetExhaustion> {
        let mut state = self.lock()?;
        if !state.terminal {
            self.check_elapsed(&mut state)?;
        }
        if state.terminal || state.tool_started + state.tool_reserved >= self.limits.max_tool_calls
        {
            state.tool_rejected += 1;
            return Err(self.exhaustion(
                if state.terminal {
                    BudgetExhaustionCategory::RunTerminal
                } else {
                    BudgetExhaustionCategory::TotalToolCalls
                },
                self.limits.max_tool_calls as u64,
                (state.tool_started + state.tool_reserved) as u64,
                Some(role),
            ));
        }
        state.tool_reserved += 1;
        Ok(ToolReservation {
            ledger: self.clone(),
            committed: false,
        })
    }

    pub fn reserve_context(&self, bytes: usize) -> Result<(), BudgetExhaustion> {
        let mut state = self.lock()?;
        if !state.terminal {
            self.check_elapsed(&mut state)?;
        }
        if state.terminal
            || state.context_bytes_consumed.saturating_add(bytes) > self.limits.max_context_bytes
        {
            return Err(self.exhaustion(
                if state.terminal {
                    BudgetExhaustionCategory::RunTerminal
                } else {
                    BudgetExhaustionCategory::InputOrContextLimit
                },
                self.limits.max_context_bytes as u64,
                state.context_bytes_consumed.saturating_add(bytes) as u64,
                None,
            ));
        }
        state.context_bytes_consumed += bytes;
        Ok(())
    }

    pub fn record_output_tokens(&self, tokens: u64) -> Result<(), BudgetExhaustion> {
        let mut state = self.lock()?;
        if !state.terminal {
            self.check_elapsed(&mut state)?;
        }
        if state.terminal
            || state.output_tokens_consumed.saturating_add(tokens)
                > u64::from(self.limits.max_output_tokens)
        {
            return Err(self.exhaustion(
                if state.terminal {
                    BudgetExhaustionCategory::RunTerminal
                } else {
                    BudgetExhaustionCategory::OutputTokenLimit
                },
                u64::from(self.limits.max_output_tokens),
                state.output_tokens_consumed.saturating_add(tokens),
                None,
            ));
        }
        state.output_tokens_consumed += tokens;
        Ok(())
    }

    pub fn admit_executor_tasks(&self, count: usize) -> Result<(), BudgetExhaustion> {
        let mut state = self.lock()?;
        if !state.terminal {
            self.check_elapsed(&mut state)?;
        }
        if state.terminal
            || state.executor_tasks_admitted.saturating_add(count) > self.limits.max_executor_tasks
        {
            return Err(self.exhaustion(
                if state.terminal {
                    BudgetExhaustionCategory::RunTerminal
                } else {
                    BudgetExhaustionCategory::ExecutorTaskCount
                },
                self.limits.max_executor_tasks as u64,
                state.executor_tasks_admitted as u64 + count as u64,
                Some(RoutingRole::Executor),
            ));
        }
        if self.limits.max_executor_concurrency == 0 {
            return Err(self.exhaustion(
                BudgetExhaustionCategory::ExecutorConcurrency,
                0,
                0,
                Some(RoutingRole::Executor),
            ));
        }
        state.executor_tasks_admitted += count;
        Ok(())
    }

    pub fn admit_repair_attempt(&self) -> Result<(), BudgetExhaustion> {
        let mut state = self.lock()?;
        if !state.terminal {
            self.check_elapsed(&mut state)?;
        }
        if state.terminal || state.repair_attempts_admitted >= self.limits.max_repair_attempts {
            return Err(self.exhaustion(
                if state.terminal {
                    BudgetExhaustionCategory::RunTerminal
                } else {
                    BudgetExhaustionCategory::RepairAttempts
                },
                self.limits.max_repair_attempts as u64,
                state.repair_attempts_admitted as u64,
                Some(RoutingRole::Repair),
            ));
        }
        state.repair_attempts_admitted += 1;
        Ok(())
    }

    pub fn record_tool_output(&self, bytes: usize) -> Result<(), BudgetExhaustion> {
        let mut state = self.lock()?;
        if !state.terminal {
            self.check_elapsed(&mut state)?;
        }
        if state.terminal
            || state.tool_output_bytes.saturating_add(bytes) > self.limits.max_tool_output_bytes
        {
            return Err(self.exhaustion(
                if state.terminal {
                    BudgetExhaustionCategory::RunTerminal
                } else {
                    BudgetExhaustionCategory::AggregateToolOutput
                },
                self.limits.max_tool_output_bytes as u64,
                state.tool_output_bytes as u64 + bytes as u64,
                None,
            ));
        }
        state.tool_output_bytes += bytes;
        Ok(())
    }

    pub fn record_provider_completed(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.provider_completed += 1;
        }
    }

    pub fn record_provider_cancelled(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.provider_cancelled += 1;
        }
    }

    pub fn record_provider_failed(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.provider_failed += 1;
        }
    }

    pub(crate) fn record_provider_timed_out(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.provider_timed_out += 1;
        }
    }

    pub fn record_provider_rejected(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.provider_rejected += 1;
        }
    }

    pub fn record_tool_completed(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.tool_completed += 1;
        }
    }

    pub fn mark_terminal(&self) -> Result<(), BudgetExhaustion> {
        let mut state = self.lock()?;
        state.terminal = true;
        Ok(())
    }

    pub fn snapshot(&self) -> ExecutionBudgetSnapshot {
        let Ok(state) = self.state.lock() else {
            return ExecutionBudgetSnapshot {
                limits: self.limits.clone(),
                provider_reserved: 0,
                provider_started: 0,
                provider_completed: 0,
                provider_cancelled: 0,
                provider_failed: 0,
                provider_timed_out: 0,
                provider_rejected: 0,
                tool_reserved: 0,
                tool_started: 0,
                tool_completed: 0,
                tool_rejected: 0,
                tool_output_bytes: 0,
                context_bytes_consumed: 0,
                output_tokens_consumed: 0,
                executor_tasks_admitted: 0,
                repair_attempts_admitted: 0,
                provider_admitted_by_role: Vec::new(),
                elapsed: Duration::ZERO,
                elapsed_exhausted: false,
                terminal: true,
                last_exhaustion: None,
            };
        };
        ExecutionBudgetSnapshot {
            limits: self.limits.clone(),
            provider_reserved: state.provider_reserved,
            provider_started: state.provider_started,
            provider_completed: state.provider_completed,
            provider_cancelled: state.provider_cancelled,
            provider_failed: state.provider_failed,
            provider_timed_out: state.provider_timed_out,
            provider_rejected: state.provider_rejected,
            tool_reserved: state.tool_reserved,
            tool_started: state.tool_started,
            tool_completed: state.tool_completed,
            tool_rejected: state.tool_rejected,
            tool_output_bytes: state.tool_output_bytes,
            context_bytes_consumed: state.context_bytes_consumed,
            output_tokens_consumed: state.output_tokens_consumed,
            executor_tasks_admitted: state.executor_tasks_admitted,
            repair_attempts_admitted: state.repair_attempts_admitted,
            provider_admitted_by_role: state
                .provider_by_role
                .iter()
                .map(|(role, count)| (*role, *count))
                .collect(),
            elapsed: self.started_at.elapsed(),
            elapsed_exhausted: state.elapsed_exhausted,
            terminal: state.terminal,
            last_exhaustion: self.last_exhaustion.lock().ok().and_then(|value| *value),
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, LedgerState>, BudgetExhaustion> {
        self.state.lock().map_err(|_| BudgetExhaustion {
            category: BudgetExhaustionCategory::LedgerUnavailable,
            limit: 0,
            consumed_or_reserved: 0,
            role: None,
        })
    }

    fn check_elapsed(&self, state: &mut LedgerState) -> Result<(), BudgetExhaustion> {
        if self.started_at.elapsed() >= self.limits.max_elapsed {
            state.elapsed_exhausted = true;
            return Err(self.exhaustion(
                BudgetExhaustionCategory::TotalElapsedTime,
                self.limits.max_elapsed.as_millis() as u64,
                self.started_at.elapsed().as_millis() as u64,
                None,
            ));
        }
        Ok(())
    }

    fn exhaustion(
        &self,
        category: BudgetExhaustionCategory,
        limit: u64,
        consumed_or_reserved: u64,
        role: Option<RoutingRole>,
    ) -> BudgetExhaustion {
        let exhaustion = BudgetExhaustion {
            category,
            limit,
            consumed_or_reserved,
            role,
        };
        if let Ok(mut last_exhaustion) = self.last_exhaustion.lock() {
            *last_exhaustion = Some(exhaustion);
        }
        exhaustion
    }
}

impl fmt::Debug for ExecutionBudgetLedger {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionBudgetLedger")
            .field("snapshot", &self.snapshot())
            .finish()
    }
}
