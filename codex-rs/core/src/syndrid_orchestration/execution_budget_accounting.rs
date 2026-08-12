use super::ExecutionBudgetLimits;
use super::RoutingRole;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionBudgetSnapshot {
    pub limits: ExecutionBudgetLimits,
    pub provider_reserved: usize,
    pub provider_started: usize,
    pub provider_completed: usize,
    pub provider_cancelled: usize,
    pub provider_failed: usize,
    pub provider_rejected: usize,
    pub tool_reserved: usize,
    pub tool_started: usize,
    pub tool_completed: usize,
    pub tool_rejected: usize,
    pub tool_output_bytes: usize,
    pub context_bytes_consumed: usize,
    pub output_tokens_consumed: u64,
    pub executor_tasks_admitted: usize,
    pub repair_attempts_admitted: usize,
    pub provider_admitted_by_role: Vec<(RoutingRole, usize)>,
    pub elapsed: std::time::Duration,
    pub elapsed_exhausted: bool,
    pub terminal: bool,
    pub last_exhaustion: Option<BudgetExhaustion>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BudgetExhaustion {
    pub category: BudgetExhaustionCategory,
    pub limit: u64,
    pub consumed_or_reserved: u64,
    pub role: Option<RoutingRole>,
}

impl fmt::Display for BudgetExhaustion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "execution budget exhausted: {:?} ({}/{})",
            self.category, self.consumed_or_reserved, self.limit
        )
    }
}

impl std::error::Error for BudgetExhaustion {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetExhaustionCategory {
    TotalProviderInvocations,
    PlannerProviderInvocations,
    ExecutorProviderInvocations,
    VerifierProviderInvocations,
    RepairProviderInvocations,
    TotalToolCalls,
    AggregateToolOutput,
    InputOrContextLimit,
    OutputTokenLimit,
    ExecutorTaskCount,
    ExecutorConcurrency,
    RepairAttempts,
    TotalElapsedTime,
    RepairElapsedTime,
    SubagentDepth,
    RunTerminal,
    LedgerUnavailable,
}
