use super::ExecutionBudgetSnapshot;
use super::ExecutionModeSelection;
use super::LiveOrchestrationTerminal;
use super::LiveRepairResult;
use super::LiveRoleOutcome;
use super::PolicySource;
use super::ResolvedExecutionPolicy;
use super::RoutingProfileId;
use super::RoutingRole;
use super::SessionExecutionStatus;
use super::execution_budget_accounting::BudgetExhaustion;
use super::execution_budget_accounting::BudgetExhaustionCategory;
use super::live_coordinator_types::LiveEvent;
use super::orchestration_observability::*;
use std::sync::Mutex;

#[derive(Clone)]
pub(crate) struct ObservationIdentity {
    pub generation: u64,
    pub run_id: String,
    pub mode: ExecutionModeSelection,
    pub source: PolicySource,
    pub profile_id: RoutingProfileId,
    pub policy: ResolvedExecutionPolicy,
}

/// Generation-bound collector for one coherent observation snapshot.
pub struct OrchestrationObservationCollector {
    generation: u64,
    state: Mutex<CollectorState>,
}

struct CollectorState {
    stage: OrchestrationObservationStage,
    active_role: ObservedActiveRole,
    terminal: Option<LiveOrchestrationTerminal>,
    events_applied: usize,
    finalized: bool,
}

impl OrchestrationObservationCollector {
    pub(crate) fn new(identity: &ObservationIdentity) -> Self {
        Self {
            generation: identity.generation,
            state: Mutex::new(CollectorState {
                stage: OrchestrationObservationStage::Idle,
                active_role: ObservedActiveRole::None,
                terminal: None,
                events_applied: 0,
                finalized: false,
            }),
        }
    }

    pub(crate) fn apply_events(
        &self,
        generation: u64,
        events: &[LiveEvent],
    ) -> Result<(), ObservationQuality> {
        if generation != self.generation {
            return Err(ObservationQuality::Unavailable);
        }
        let Ok(mut state) = self.state.lock() else {
            return Err(ObservationQuality::Unavailable);
        };
        if state.finalized {
            return Err(ObservationQuality::Unavailable);
        }
        for event in events.iter().skip(state.events_applied) {
            match event {
                LiveEvent::RunPrepared => state.stage = OrchestrationObservationStage::Preparing,
                LiveEvent::PolicyValidated => {
                    state.stage = OrchestrationObservationStage::Validating
                }
                LiveEvent::RoleStarted(RoutingRole::Planner) => {
                    state.stage = OrchestrationObservationStage::Planning;
                    state.active_role = ObservedActiveRole::Planner;
                }
                LiveEvent::RoleStarted(RoutingRole::Main) => {
                    state.stage = OrchestrationObservationStage::Executing;
                    state.active_role = ObservedActiveRole::Main;
                }
                LiveEvent::RoleStarted(RoutingRole::Executor) => {
                    state.stage = OrchestrationObservationStage::Executing;
                    state.active_role = ObservedActiveRole::ExecutorBatch;
                }
                LiveEvent::RoleStarted(RoutingRole::Verifier) => {
                    state.stage = OrchestrationObservationStage::Verifying;
                    state.active_role = ObservedActiveRole::Verifier;
                }
                LiveEvent::RoleStarted(RoutingRole::Repair) | LiveEvent::RepairStarted => {
                    state.stage = OrchestrationObservationStage::Repairing;
                    state.active_role = ObservedActiveRole::Repair;
                }
                LiveEvent::ExecutorBatchStarted => {
                    state.stage = OrchestrationObservationStage::Executing;
                    state.active_role = ObservedActiveRole::ExecutorBatch;
                }
                LiveEvent::RunTerminal(terminal) => {
                    state.stage = OrchestrationObservationStage::Terminal;
                    state.active_role = ObservedActiveRole::None;
                    state.terminal = Some(*terminal);
                }
                LiveEvent::RoleSkipped(_, _)
                | LiveEvent::VerifierDecision
                | LiveEvent::RepairEligibilityEvaluated => {}
            }
        }
        state.events_applied = events.len();
        Ok(())
    }

    pub(crate) fn snapshot(
        &self,
        identity: &ObservationIdentity,
        roles: &[LiveRoleOutcome],
        budget: &ExecutionBudgetSnapshot,
        events: &[LiveEvent],
        terminal: LiveOrchestrationTerminal,
        synthesis_permitted: bool,
        peak_concurrency: usize,
    ) -> OrchestrationObservationSnapshot {
        let _ = self.apply_events(identity.generation, events);
        let state = self.state.lock().ok();
        let (stage, active_role) = state
            .as_ref()
            .map(|value| (value.stage, value.active_role))
            .unwrap_or((
                OrchestrationObservationStage::Terminal,
                ObservedActiveRole::None,
            ));
        let terminal_reason = terminal_reason(terminal, budget.last_exhaustion, roles);
        let elapsed = budget.elapsed;
        let remaining = identity
            .policy
            .policy()
            .batch_timeout
            .saturating_sub(elapsed);
        let task_counts = task_counts(roles, budget);
        let provider_started = budget.provider_started;
        let provider_active = provider_started
            .saturating_sub(budget.provider_completed)
            .saturating_sub(budget.provider_cancelled);
        let tool_active = budget.tool_started.saturating_sub(budget.tool_completed);
        OrchestrationObservationSnapshot {
            generation: Observed::exact(identity.generation),
            run_id: Observed::exact(identity.run_id.clone()),
            selected_mode: Observed::exact(identity.mode.clone()),
            policy_source: Observed::exact(identity.source),
            routing_profile_id: Observed::exact(identity.profile_id.clone()),
            lifecycle: Observed::exact(lifecycle_for(terminal)),
            stage: Observed::exact(stage),
            active_role: Observed::exact(active_role),
            terminal: Observed::exact(Some(terminal)),
            terminal_reason: Observed::exact(Some(terminal_reason)),
            synthesis_permitted: Observed::exact(synthesis_permitted),
            tasks: task_counts,
            provider: ObservationProviderUsage {
                reserved: Observed::exact(budget.provider_reserved),
                started: Observed::exact(provider_started),
                completed: Observed::exact(budget.provider_completed),
                cancelled_after_start: Observed::exact(budget.provider_cancelled),
                rejected_before_start: Observed::exact(budget.provider_rejected),
                by_role: role_provider_usage(budget),
                input_tokens: Observed::unavailable(),
                output_tokens: Observed::exact(budget.output_tokens_consumed),
                cached_input_tokens: Observed::unavailable(),
            },
            tools: ObservationToolUsage {
                reserved: Observed::exact(budget.tool_reserved),
                started: Observed::exact(budget.tool_started),
                completed: Observed::exact(budget.tool_completed),
                rejected: Observed::exact(budget.tool_rejected),
                output_bytes: Observed::exact(budget.tool_output_bytes),
            },
            budgets: budget_entries(budget, peak_concurrency),
            current_provider_count: Observed::derived(provider_active),
            current_tool_count: Observed::derived(tool_active),
            current_executor_concurrency: Observed::exact(0),
            peak_executor_concurrency: Observed::exact(peak_concurrency),
            elapsed: Observed::exact(elapsed),
            configured_timeout: Observed::exact(identity.policy.policy().batch_timeout),
            remaining_time: Observed::derived(remaining),
            timed_out: Observed::exact(terminal == LiveOrchestrationTerminal::TimedOut),
            cancelled: Observed::exact(terminal == LiveOrchestrationTerminal::Cancelled),
            cleanup_pending: Observed::exact(false),
            repair: repair_state(roles, &identity.policy),
        }
    }
}

fn lifecycle_for(terminal: LiveOrchestrationTerminal) -> SessionExecutionStatus {
    match terminal {
        LiveOrchestrationTerminal::Completed => SessionExecutionStatus::Completed,
        LiveOrchestrationTerminal::Cancelled => SessionExecutionStatus::Cancelled,
        LiveOrchestrationTerminal::TimedOut => SessionExecutionStatus::TimedOut,
        LiveOrchestrationTerminal::Failed | LiveOrchestrationTerminal::BudgetExhausted => {
            SessionExecutionStatus::Failed
        }
    }
}

fn terminal_reason(
    terminal: LiveOrchestrationTerminal,
    exhaustion: Option<BudgetExhaustion>,
    roles: &[LiveRoleOutcome],
) -> ObservationTerminalReason {
    match terminal {
        LiveOrchestrationTerminal::Completed => ObservationTerminalReason::Completed,
        LiveOrchestrationTerminal::Cancelled => ObservationTerminalReason::Cancelled,
        LiveOrchestrationTerminal::TimedOut => ObservationTerminalReason::TimedOut,
        LiveOrchestrationTerminal::BudgetExhausted => exhaustion
            .map(ObservationTerminalReason::BudgetExhausted)
            .unwrap_or(ObservationTerminalReason::InternalCoordinatorFailure),
        LiveOrchestrationTerminal::Failed => {
            if roles
                .iter()
                .any(|role| role.repair_result == Some(LiveRepairResult::RepairFailed))
            {
                ObservationTerminalReason::RepairFailed
            } else if roles
                .iter()
                .any(|role| role.state == super::LiveRoleState::Rejected)
            {
                ObservationTerminalReason::VerifierRejected
            } else {
                ObservationTerminalReason::ProviderFailed
            }
        }
    }
}

fn task_counts(
    roles: &[LiveRoleOutcome],
    budget: &ExecutionBudgetSnapshot,
) -> ObservationTaskCounts {
    let executor = roles.iter().find(|role| role.role == RoutingRole::Executor);
    let total = executor.map_or(0, |role| role.task_states.len());
    let completed = executor.map_or(0, |role| {
        role.task_states
            .iter()
            .filter(|state| **state == super::LiveRoleState::Succeeded)
            .count()
    });
    let failed = executor.map_or(0, |role| {
        role.task_states
            .iter()
            .filter(|state| **state == super::LiveRoleState::Failed)
            .count()
    });
    let cancelled = executor.map_or(0, |role| {
        role.task_states
            .iter()
            .filter(|state| **state == super::LiveRoleState::Cancelled)
            .count()
    });
    ObservationTaskCounts {
        total: Observed::exact(total.max(budget.executor_tasks_admitted)),
        queued: Observed::exact(0),
        active: Observed::exact(0),
        completed: Observed::exact(completed),
        failed: Observed::exact(failed),
        cancelled: Observed::exact(cancelled),
        outcomes_available: Observed::exact(executor.map_or(0, |role| role.task_states.len())),
    }
}

fn role_provider_usage(budget: &ExecutionBudgetSnapshot) -> Vec<ObservationRoleProviderUsage> {
    budget
        .provider_admitted_by_role
        .iter()
        .map(|(role, count)| ObservationRoleProviderUsage {
            role: *role,
            admitted: Observed::exact(*count),
        })
        .collect()
}

fn budget_entries(
    budget: &ExecutionBudgetSnapshot,
    peak_concurrency: usize,
) -> Vec<ObservationBudget> {
    let mut entries = Vec::new();
    let push = |entries: &mut Vec<ObservationBudget>,
                category: BudgetExhaustionCategory,
                limit: u64,
                consumed: u64,
                role: Option<RoutingRole>| {
        let remaining = limit.saturating_sub(consumed);
        entries.push(ObservationBudget {
            category,
            role,
            limit: Observed::exact(limit),
            consumed_or_reserved: Observed::exact(consumed),
            remaining: Observed::derived(remaining),
            exhausted: Observed::exact(consumed >= limit),
        });
    };
    push(
        &mut entries,
        BudgetExhaustionCategory::TotalProviderInvocations,
        budget.limits.max_provider_invocations as u64,
        (budget.provider_started + budget.provider_reserved) as u64,
        None,
    );
    for (role, category, limit) in [
        (
            RoutingRole::Planner,
            BudgetExhaustionCategory::PlannerProviderInvocations,
            budget.limits.max_planner_provider_invocations,
        ),
        (
            RoutingRole::Executor,
            BudgetExhaustionCategory::ExecutorProviderInvocations,
            budget.limits.max_executor_provider_invocations,
        ),
        (
            RoutingRole::Verifier,
            BudgetExhaustionCategory::VerifierProviderInvocations,
            budget.limits.max_verifier_provider_invocations,
        ),
        (
            RoutingRole::Repair,
            BudgetExhaustionCategory::RepairProviderInvocations,
            budget.limits.max_repair_provider_invocations,
        ),
    ] {
        let consumed = budget
            .provider_admitted_by_role
            .iter()
            .find_map(|(admitted_role, count)| (*admitted_role == role).then_some(*count))
            .unwrap_or(0);
        push(
            &mut entries,
            category,
            limit as u64,
            consumed as u64,
            Some(role),
        );
    }
    push(
        &mut entries,
        BudgetExhaustionCategory::TotalToolCalls,
        budget.limits.max_tool_calls as u64,
        (budget.tool_started + budget.tool_reserved) as u64,
        None,
    );
    push(
        &mut entries,
        BudgetExhaustionCategory::AggregateToolOutput,
        budget.limits.max_tool_output_bytes as u64,
        budget.tool_output_bytes as u64,
        None,
    );
    push(
        &mut entries,
        BudgetExhaustionCategory::InputOrContextLimit,
        budget.limits.max_context_bytes as u64,
        budget.context_bytes_consumed as u64,
        None,
    );
    push(
        &mut entries,
        BudgetExhaustionCategory::OutputTokenLimit,
        u64::from(budget.limits.max_output_tokens),
        budget.output_tokens_consumed,
        None,
    );
    push(
        &mut entries,
        BudgetExhaustionCategory::ExecutorTaskCount,
        budget.limits.max_executor_tasks as u64,
        budget.executor_tasks_admitted as u64,
        Some(RoutingRole::Executor),
    );
    push(
        &mut entries,
        BudgetExhaustionCategory::RepairAttempts,
        budget.limits.max_repair_attempts as u64,
        budget.repair_attempts_admitted as u64,
        Some(RoutingRole::Repair),
    );
    push(
        &mut entries,
        BudgetExhaustionCategory::ExecutorConcurrency,
        budget.limits.max_executor_concurrency as u64,
        peak_concurrency as u64,
        Some(RoutingRole::Executor),
    );
    push(
        &mut entries,
        BudgetExhaustionCategory::TotalElapsedTime,
        budget.limits.max_elapsed.as_millis() as u64,
        budget.elapsed.as_millis() as u64,
        None,
    );
    entries.push(ObservationBudget {
        category: BudgetExhaustionCategory::RepairElapsedTime,
        role: Some(RoutingRole::Repair),
        limit: Observed::exact(budget.limits.max_repair_elapsed.as_millis() as u64),
        consumed_or_reserved: Observed::unavailable(),
        remaining: Observed::unavailable(),
        exhausted: Observed::unavailable(),
    });
    entries.push(ObservationBudget {
        category: BudgetExhaustionCategory::SubagentDepth,
        role: None,
        limit: Observed::exact(u64::from(budget.limits.max_depth)),
        consumed_or_reserved: Observed::unavailable(),
        remaining: Observed::unavailable(),
        exhausted: Observed::unavailable(),
    });
    entries
}

fn repair_state(
    roles: &[LiveRoleOutcome],
    policy: &ResolvedExecutionPolicy,
) -> ObservationRepairState {
    let repair = roles.iter().find(|role| role.role == RoutingRole::Repair);
    let result = repair.and_then(|role| role.repair_result);
    ObservationRepairState {
        enabled: Observed::exact(
            policy.role(RoutingRole::Repair).activation != super::RoleActivation::Disabled,
        ),
        eligible: Observed::exact(result.is_some()),
        attempted: Observed::exact(repair.is_some_and(|role| role.repair_attempts > 0)),
        attempts: Observed::exact(repair.map_or(0, |role| role.repair_attempts)),
        result: result.map_or_else(Observed::unavailable, Observed::exact),
        timed_out: Observed::exact(result == Some(LiveRepairResult::RepairTimedOut)),
    }
}
