use super::execution_budget_accounting::BudgetExhaustionCategory;
use super::live_coordinator_types::LiveOrchestrationError;
use super::live_coordinator_types::LiveOrchestrationTerminal;
use super::live_coordinator_types::LiveRoleOutcome;
use super::live_coordinator_types::LiveRoleState;
use super::orchestration_failure::*;
use super::routing_profiles::RoutingRole;
use super::subagent::SubagentError;
use super::subagent_repair::SubagentRepairError;

#[test]
fn failure_mapping_preserves_exact_budget_category() {
    let failure = OrchestrationFailure::from_terminal(
        LiveOrchestrationTerminal::BudgetExhausted,
        Some(LiveOrchestrationError::BudgetExhaustionCategory(
            BudgetExhaustionCategory::RepairAttempts,
        )),
        &[],
        Some(BudgetExhaustionCategory::RepairAttempts),
    )
    .expect("terminal failure should map");
    assert_eq!(
        failure,
        OrchestrationFailure {
            kind: OrchestrationFailureKind::BudgetExhausted(
                BudgetExhaustionCategory::RepairAttempts,
            ),
            retryability: Retryability::Unknown,
            role: None,
            tool: None,
            terminal: LiveOrchestrationTerminal::BudgetExhausted,
        }
    );
}

#[test]
fn cancellation_and_timeout_have_stable_precedence_types() {
    let cancelled = OrchestrationFailure::from_terminal(
        LiveOrchestrationTerminal::Cancelled,
        Some(LiveOrchestrationError::Cancellation),
        &[],
        None,
    )
    .expect("cancelled outcome should map");
    let timed_out = OrchestrationFailure::from_terminal(
        LiveOrchestrationTerminal::TimedOut,
        Some(LiveOrchestrationError::Timeout),
        &[],
        None,
    )
    .expect("timed out outcome should map");
    assert_eq!(cancelled.kind, OrchestrationFailureKind::UserCancelled);
    assert_eq!(cancelled.retryability, Retryability::NotRetryable);
    assert_eq!(timed_out.kind, OrchestrationFailureKind::TotalTimedOut);
}

#[test]
fn verifier_rejection_is_not_provider_failure() {
    let failure = OrchestrationFailure::from_terminal(
        LiveOrchestrationTerminal::Failed,
        Some(LiveOrchestrationError::VerifierRejected),
        &[],
        None,
    )
    .expect("rejection should map");
    assert_eq!(failure.kind, OrchestrationFailureKind::VerifierRejected);
    assert_eq!(failure.retryability, Retryability::Unknown);
}

#[test]
fn successful_outcome_has_no_failure() {
    assert_eq!(
        OrchestrationFailure::from_terminal(LiveOrchestrationTerminal::Completed, None, &[], None,),
        None
    );
}

#[test]
fn planner_failure_is_not_relabelled_as_executor_failure() {
    let roles = vec![LiveRoleOutcome {
        role: RoutingRole::Planner,
        state: LiveRoleState::Failed,
        skip_reason: None,
        task_ids: vec!["planner".to_string()],
        task_states: vec![LiveRoleState::Failed],
        provider_invocations: 1,
        tool_calls: 0,
        repair_result: None,
        repair_attempts: 0,
    }];
    let failure = OrchestrationFailure::from_terminal(
        LiveOrchestrationTerminal::Failed,
        Some(LiveOrchestrationError::ExecutorBatchFailure),
        &roles,
        None,
    )
    .expect("planner failure should map");
    assert_eq!(
        failure.kind,
        OrchestrationFailureKind::PlannerProviderFailure
    );
    assert_eq!(failure.role, Some(RoutingRole::Planner));
}

fn candidate(
    kind: OrchestrationFailureKind,
    terminal: LiveOrchestrationTerminal,
) -> OrchestrationFailure {
    OrchestrationFailure {
        kind,
        retryability: Retryability::Unknown,
        role: None,
        tool: None,
        terminal,
    }
}

#[test]
fn terminal_cause_arbiter_applies_precedence_and_freezes() {
    let arbiter = TerminalCauseArbiter::new(7);
    let stage = candidate(
        OrchestrationFailureKind::PlannerProviderFailure,
        LiveOrchestrationTerminal::Failed,
    );
    assert_eq!(
        arbiter.submit(7, stage),
        TerminalCauseSubmission::Accepted(stage)
    );

    let budget = candidate(
        OrchestrationFailureKind::BudgetExhausted(BudgetExhaustionCategory::TotalToolCalls),
        LiveOrchestrationTerminal::BudgetExhausted,
    );
    assert_eq!(
        arbiter.submit(7, budget),
        TerminalCauseSubmission::Replaced(budget)
    );

    let cancellation = candidate(
        OrchestrationFailureKind::UserCancelled,
        LiveOrchestrationTerminal::Cancelled,
    );
    assert_eq!(
        arbiter.submit(7, cancellation),
        TerminalCauseSubmission::Replaced(cancellation)
    );
    assert_eq!(arbiter.freeze(7), Ok(Some(cancellation)));
    assert_eq!(
        arbiter.submit(7, stage),
        TerminalCauseSubmission::Frozen(cancellation)
    );
    assert_eq!(
        arbiter.submit(8, stage),
        TerminalCauseSubmission::StaleGeneration
    );
}

#[test]
fn join_failures_keep_their_role_specific_categories() {
    assert_eq!(
        classify_subagent_failure(
            RoutingRole::Executor,
            Some(SubagentError::JoinFailure),
            None,
        ),
        OrchestrationFailureKind::ExecutorJoinFailure
    );
    assert_eq!(
        classify_repair_failure(Some(SubagentRepairError::JoinFailure)),
        OrchestrationFailureKind::RepairJoinFailure
    );
}
