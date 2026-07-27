use super::ExecutionBudgetLedger;
use super::ResolvedExecutionPolicy;
use super::RoutingRole;
use super::SessionExecutionPolicyState;
use super::SessionExecutionStateError;
use super::SessionExecutionStatus;
use super::SubagentBatchOutcome;
use super::SubagentOutcome;
use super::SubagentStatus;
use super::live_coordinator_types::*;

pub(super) fn finish_outcome(
    state: &SessionExecutionPolicyState,
    request: LiveOrchestrationRequest,
    policy: ResolvedExecutionPolicy,
    profile_id: super::RoutingProfileId,
    budget: &ExecutionBudgetLedger,
    events: &mut Vec<LiveEvent>,
    terminal: LiveOrchestrationTerminal,
    error: Option<LiveOrchestrationError>,
    roles: Vec<LiveRoleOutcome>,
    peak_concurrency: usize,
    provider_invocations: usize,
    tool_calls: usize,
) -> Result<LiveOrchestrationOutcome, LiveOrchestrationError> {
    let _ = budget.mark_terminal();
    let budget_snapshot = budget.snapshot();
    let exact_budget_category = budget_snapshot.last_exhaustion.map(|value| value.category);
    let exact_budget_error =
        exact_budget_category.map(LiveOrchestrationError::BudgetExhaustionCategory);
    let state_terminal = match terminal {
        LiveOrchestrationTerminal::Completed => SessionExecutionStatus::Completed,
        LiveOrchestrationTerminal::Cancelled => SessionExecutionStatus::Cancelling,
        LiveOrchestrationTerminal::TimedOut => SessionExecutionStatus::TimedOut,
        LiveOrchestrationTerminal::Failed | LiveOrchestrationTerminal::BudgetExhausted => {
            SessionExecutionStatus::Failed
        }
    };
    state
        .terminalize_generation(budget.generation(), state_terminal)
        .map_err(map_state_error)?;
    if state_terminal == SessionExecutionStatus::Cancelling {
        state
            .terminalize_generation(budget.generation(), SessionExecutionStatus::Cancelled)
            .map_err(map_state_error)?;
    }
    events.push(LiveEvent::RunTerminal(terminal));
    events.truncate(MAX_EVENTS);
    Ok(LiveOrchestrationOutcome {
        run_id: request.run_id,
        selected_mode: policy.selected_mode().clone(),
        resolved_policy: policy.explain(),
        routing_profile_id: profile_id,
        terminal,
        roles,
        peak_concurrency,
        provider_invocations,
        tool_calls,
        cancelled: terminal == LiveOrchestrationTerminal::Cancelled,
        timed_out: terminal == LiveOrchestrationTerminal::TimedOut,
        budget_exhausted: terminal == LiveOrchestrationTerminal::BudgetExhausted,
        terminal_error: if terminal == LiveOrchestrationTerminal::BudgetExhausted {
            exact_budget_error.or(error)
        } else {
            error
        },
        synthesis_permitted: matches!(terminal, LiveOrchestrationTerminal::Completed),
        events: events.clone(),
        budget: budget_snapshot,
        budget_exhaustion_category: exact_budget_category,
    })
}

pub(super) fn skipped(role: RoutingRole, reason: LiveRoleSkipReason) -> LiveRoleOutcome {
    LiveRoleOutcome {
        role,
        state: LiveRoleState::Skipped,
        skip_reason: Some(reason),
        task_ids: Vec::new(),
        task_states: Vec::new(),
        provider_invocations: 0,
        tool_calls: 0,
        repair_result: None,
        repair_attempts: 0,
    }
}

pub(super) fn role_from_single(
    role: RoutingRole,
    outcome: &Result<SubagentOutcome, super::SubagentError>,
) -> LiveRoleOutcome {
    match outcome {
        Ok(value) => LiveRoleOutcome {
            role,
            state: state_from_status(value.status),
            skip_reason: None,
            task_ids: vec![value.task_id.clone()],
            task_states: vec![state_from_status(value.status)],
            provider_invocations: value.provider_turns,
            tool_calls: value.tool_calls,
            repair_result: None,
            repair_attempts: 0,
        },
        Err(_) => LiveRoleOutcome {
            role,
            state: LiveRoleState::Failed,
            skip_reason: None,
            task_ids: vec!["role".to_string()],
            task_states: vec![LiveRoleState::Failed],
            provider_invocations: 0,
            tool_calls: 0,
            repair_result: None,
            repair_attempts: 0,
        },
    }
}

pub(super) fn role_from_batch(batch: &SubagentBatchOutcome) -> LiveRoleOutcome {
    LiveRoleOutcome {
        role: RoutingRole::Executor,
        state: if matches!(batch.status, super::SubagentBatchStatus::Completed) {
            LiveRoleState::Succeeded
        } else if batch.budget_exhausted {
            LiveRoleState::BudgetExhausted
        } else if batch.status == super::SubagentBatchStatus::Cancelled {
            LiveRoleState::Cancelled
        } else {
            LiveRoleState::Failed
        },
        skip_reason: None,
        task_ids: batch
            .tasks
            .iter()
            .map(|task| task.task_id.clone())
            .collect(),
        task_states: batch
            .tasks
            .iter()
            .map(|task| match task.state {
                super::SubagentTaskState::Completed => LiveRoleState::Succeeded,
                super::SubagentTaskState::Cancelled => LiveRoleState::Cancelled,
                super::SubagentTaskState::TimedOut => LiveRoleState::TimedOut,
                super::SubagentTaskState::BudgetExhausted => LiveRoleState::BudgetExhausted,
                super::SubagentTaskState::Failed => LiveRoleState::Failed,
                super::SubagentTaskState::Queued
                | super::SubagentTaskState::NotStarted
                | super::SubagentTaskState::Running => LiveRoleState::Skipped,
            })
            .collect(),
        provider_invocations: batch.aggregate_provider_turns,
        tool_calls: batch.aggregate_tool_calls,
        repair_result: None,
        repair_attempts: 0,
    }
}

pub(super) fn role_from_repair(outcome: &super::SubagentRepairOutcome) -> LiveRoleOutcome {
    let attempt = outcome.attempts.last();
    LiveRoleOutcome {
        role: RoutingRole::Repair,
        state: match outcome.terminal {
            super::SubagentRepairTerminal::RepairSucceeded
            | super::SubagentRepairTerminal::InitialSucceeded => LiveRoleState::Succeeded,
            super::SubagentRepairTerminal::Cancelled => LiveRoleState::Cancelled,
            super::SubagentRepairTerminal::RepairTimedOut => LiveRoleState::TimedOut,
            super::SubagentRepairTerminal::BudgetExhausted => LiveRoleState::BudgetExhausted,
            _ => LiveRoleState::Failed,
        },
        skip_reason: None,
        task_ids: vec![outcome.task_id.clone()],
        task_states: vec![state_from_repair_terminal(outcome.terminal)],
        provider_invocations: attempt.map_or(0, |value| value.provider_invocations),
        tool_calls: attempt.map_or(0, |value| value.tool_calls),
        repair_result: Some(match outcome.terminal {
            super::SubagentRepairTerminal::InitialSucceeded
            | super::SubagentRepairTerminal::RepairSucceeded => LiveRepairResult::RepairSucceeded,
            super::SubagentRepairTerminal::RepairDisabled => LiveRepairResult::RepairDisabled,
            super::SubagentRepairTerminal::NotEligible => LiveRepairResult::NotEligible,
            super::SubagentRepairTerminal::RepairTimedOut => LiveRepairResult::RepairTimedOut,
            super::SubagentRepairTerminal::Cancelled => LiveRepairResult::Cancelled,
            super::SubagentRepairTerminal::BudgetExhausted => LiveRepairResult::BudgetExhausted,
            super::SubagentRepairTerminal::InitialFailed
            | super::SubagentRepairTerminal::RepairFailed => LiveRepairResult::RepairFailed,
        }),
        repair_attempts: outcome.attempts.len(),
    }
}

pub(super) fn role_from_repair_error(error: super::SubagentRepairError) -> LiveRoleOutcome {
    let (state, repair_result) = match error {
        super::SubagentRepairError::CancelledBeforeRepair => {
            (LiveRoleState::Cancelled, LiveRepairResult::Cancelled)
        }
        super::SubagentRepairError::BudgetExhausted => (
            LiveRoleState::BudgetExhausted,
            LiveRepairResult::BudgetExhausted,
        ),
        super::SubagentRepairError::PolicyInvalid => {
            (LiveRoleState::Failed, LiveRepairResult::PolicyInvalid)
        }
        super::SubagentRepairError::RouteMismatch => {
            (LiveRoleState::Failed, LiveRepairResult::RouteMismatch)
        }
        super::SubagentRepairError::BatchInvalid => {
            (LiveRoleState::Failed, LiveRepairResult::BatchInvalid)
        }
        super::SubagentRepairError::InitialValidationFailed(_) => (
            LiveRoleState::Failed,
            LiveRepairResult::InitialValidationFailed,
        ),
    };
    LiveRoleOutcome {
        role: RoutingRole::Repair,
        state,
        skip_reason: None,
        task_ids: vec!["repair".to_string()],
        task_states: vec![state],
        provider_invocations: 0,
        tool_calls: 0,
        repair_result: Some(repair_result),
        repair_attempts: 0,
    }
}

pub(super) fn repair_error_terminal(
    error: super::SubagentRepairError,
) -> (LiveOrchestrationTerminal, LiveOrchestrationError) {
    match error {
        super::SubagentRepairError::PolicyInvalid => (
            LiveOrchestrationTerminal::Failed,
            LiveOrchestrationError::RepairPolicyInvalid,
        ),
        super::SubagentRepairError::InitialValidationFailed(_) => (
            LiveOrchestrationTerminal::Failed,
            LiveOrchestrationError::RepairInitialValidationFailed,
        ),
        super::SubagentRepairError::RouteMismatch => (
            LiveOrchestrationTerminal::Failed,
            LiveOrchestrationError::RepairRouteMismatch,
        ),
        super::SubagentRepairError::BudgetExhausted => (
            LiveOrchestrationTerminal::BudgetExhausted,
            LiveOrchestrationError::BudgetExhaustion,
        ),
        super::SubagentRepairError::CancelledBeforeRepair => (
            LiveOrchestrationTerminal::Cancelled,
            LiveOrchestrationError::Cancellation,
        ),
        super::SubagentRepairError::BatchInvalid => (
            LiveOrchestrationTerminal::Failed,
            LiveOrchestrationError::RepairBatchInvalid,
        ),
    }
}

fn state_from_repair_terminal(terminal: super::SubagentRepairTerminal) -> LiveRoleState {
    match terminal {
        super::SubagentRepairTerminal::InitialSucceeded
        | super::SubagentRepairTerminal::RepairSucceeded => LiveRoleState::Succeeded,
        super::SubagentRepairTerminal::RepairDisabled
        | super::SubagentRepairTerminal::NotEligible
        | super::SubagentRepairTerminal::InitialFailed
        | super::SubagentRepairTerminal::RepairFailed => LiveRoleState::Failed,
        super::SubagentRepairTerminal::RepairTimedOut => LiveRoleState::TimedOut,
        super::SubagentRepairTerminal::Cancelled => LiveRoleState::Cancelled,
        super::SubagentRepairTerminal::BudgetExhausted => LiveRoleState::BudgetExhausted,
    }
}

pub(super) fn state_from_status(status: SubagentStatus) -> LiveRoleState {
    match status {
        SubagentStatus::Completed | SubagentStatus::CompletedWithTruncation => {
            LiveRoleState::Succeeded
        }
        SubagentStatus::Cancelled => LiveRoleState::Cancelled,
        SubagentStatus::TimedOut => LiveRoleState::TimedOut,
        SubagentStatus::BudgetExhausted => LiveRoleState::BudgetExhausted,
        _ => LiveRoleState::Failed,
    }
}

pub(super) fn map_state_error(error: super::SessionExecutionStateError) -> LiveOrchestrationError {
    match error {
        SessionExecutionStateError::RunAlreadyActive
        | SessionExecutionStateError::PolicyMutationWhileActive
        | SessionExecutionStateError::RoutingMutationWhileActive => {
            LiveOrchestrationError::SessionAlreadyRunning
        }
        _ => LiveOrchestrationError::InvalidSessionState,
    }
}
