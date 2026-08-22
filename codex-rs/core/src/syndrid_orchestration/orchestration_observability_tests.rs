use super::ResolvedExecutionPolicy;
use super::execution_budget::ExecutionBudgetLedger;
use super::execution_budget_accounting::BudgetExhaustionCategory;
use super::execution_modes::ExecutionModeSelection;
use super::live_coordinator_types::LiveEvent;
use super::live_coordinator_types::LiveOrchestrationTerminal;
use super::orchestration_observability::ObservationQuality;
use super::orchestration_observability::OrchestrationObservationStage;
use super::orchestration_observability_runtime::ObservationIdentity;
use super::orchestration_observability_runtime::OrchestrationObservationCollector;
use super::routing_profiles::RoutingProfileId;
use super::routing_profiles::RoutingRole;
use pretty_assertions::assert_eq;

fn identity(policy: ResolvedExecutionPolicy, generation: u64) -> ObservationIdentity {
    let mode = policy.selected_mode().clone();
    let source = policy.explain().source;
    ObservationIdentity {
        generation,
        run_id: "run-observation".to_string(),
        mode,
        source,
        profile_id: RoutingProfileId::new("profile-observation").expect("profile id"),
        policy,
    }
}

#[test]
fn idle_quality_and_unavailable_tokens_are_explicit() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("policy");
    let identity = identity(policy, 11);
    let ledger = ExecutionBudgetLedger::new_for_generation(&identity.policy, 11);
    let collector = OrchestrationObservationCollector::new(&identity);
    let snapshot = collector.snapshot(
        &identity,
        &[],
        &ledger.snapshot(),
        &[],
        LiveOrchestrationTerminal::Completed,
        true,
        0,
    );
    assert_eq!(snapshot.generation.quality, ObservationQuality::Exact);
    assert_eq!(snapshot.generation.value, Some(11));
    assert_eq!(
        snapshot.provider.cached_input_tokens.quality,
        ObservationQuality::Unavailable
    );
    assert_eq!(snapshot.provider.cached_input_tokens.value, None);
}

#[test]
fn ordered_events_produce_monotonic_terminal_stage() {
    let policy = ExecutionModeSelection::Balanced.resolve().expect("policy");
    let identity = identity(policy, 12);
    let ledger = ExecutionBudgetLedger::new_for_generation(&identity.policy, 12);
    let collector = OrchestrationObservationCollector::new(&identity);
    let events = [
        LiveEvent::RunPrepared,
        LiveEvent::PolicyValidated,
        LiveEvent::RoleStarted(RoutingRole::Executor),
        LiveEvent::ExecutorBatchStarted,
        LiveEvent::RunTerminal(LiveOrchestrationTerminal::Completed),
    ];
    let snapshot = collector.apply_events(12, &events).expect("events apply");
    assert_eq!(snapshot, ());
    let snapshot = collector.snapshot(
        &identity,
        &[],
        &ledger.snapshot(),
        &events,
        LiveOrchestrationTerminal::Completed,
        true,
        0,
    );
    assert_eq!(
        snapshot.stage,
        super::orchestration_observability::Observed {
            value: Some(OrchestrationObservationStage::Terminal),
            quality: ObservationQuality::Exact,
        }
    );
    assert_eq!(
        snapshot.active_role.value,
        Some(super::orchestration_observability::ObservedActiveRole::None)
    );
}

#[test]
fn stale_generation_cannot_mutate_collector() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("policy");
    let identity = identity(policy, 13);
    let collector = OrchestrationObservationCollector::new(&identity);
    assert_eq!(
        collector.apply_events(14, &[LiveEvent::RunPrepared]),
        Err(ObservationQuality::Unavailable)
    );
}

#[test]
fn budget_remaining_is_derived_and_exact_category_is_preserved() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("policy");
    let identity = identity(policy, 14);
    let ledger = ExecutionBudgetLedger::new_for_generation(&identity.policy, 14);
    let exhaustion = ledger.admit_executor_tasks(2).expect_err("task exhaustion");
    assert_eq!(
        exhaustion.category,
        BudgetExhaustionCategory::ExecutorTaskCount
    );
    let collector = OrchestrationObservationCollector::new(&identity);
    let snapshot = collector.snapshot(
        &identity,
        &[],
        &ledger.snapshot(),
        &[LiveEvent::RunTerminal(
            LiveOrchestrationTerminal::BudgetExhausted,
        )],
        LiveOrchestrationTerminal::BudgetExhausted,
        false,
        0,
    );
    let task_budget = snapshot
        .budgets
        .iter()
        .find(|entry| entry.category == BudgetExhaustionCategory::ExecutorTaskCount)
        .expect("task budget");
    assert_eq!(task_budget.remaining.quality, ObservationQuality::Derived);
    assert_eq!(task_budget.exhausted.value, Some(false));
    assert_eq!(
        snapshot.terminal_reason.value,
        Some(Some(
            super::orchestration_observability::ObservationTerminalReason::BudgetExhausted(
                exhaustion,
            )
        ))
    );
}

#[test]
fn failed_provider_is_not_reported_as_active_or_rejected_before_start() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("policy");
    let identity = identity(policy, 16);
    let ledger = ExecutionBudgetLedger::new_for_generation(&identity.policy, 16);
    ledger
        .reserve_provider(RoutingRole::Executor)
        .expect("provider reservation")
        .commit()
        .expect("provider starts");
    ledger.record_provider_failed();
    let collector = OrchestrationObservationCollector::new(&identity);
    let snapshot = collector.snapshot(
        &identity,
        &[],
        &ledger.snapshot(),
        &[LiveEvent::RunTerminal(LiveOrchestrationTerminal::Failed)],
        LiveOrchestrationTerminal::Failed,
        false,
        0,
    );

    assert_eq!(snapshot.current_provider_count.value, Some(0));
    assert_eq!(snapshot.provider.failed_after_start.value, Some(1));
    assert_eq!(snapshot.provider.rejected_before_start.value, Some(0));
}

#[test]
fn snapshot_debug_contains_no_execution_material() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("policy");
    let identity = identity(policy, 15);
    let ledger = ExecutionBudgetLedger::new_for_generation(&identity.policy, 15);
    let snapshot = OrchestrationObservationCollector::new(&identity).snapshot(
        &identity,
        &[],
        &ledger.snapshot(),
        &[],
        LiveOrchestrationTerminal::Completed,
        true,
        0,
    );
    let debug = format!("{snapshot:?}");
    for sentinel in [
        "PROMPT_SENTINEL",
        "CONTEXT_SENTINEL",
        "CREDENTIAL_SENTINEL",
        "TOKEN_SENTINEL",
        "PROVIDER_RESPONSE_SENTINEL",
        "TOOL_OUTPUT_SENTINEL",
        "REASONING_SENTINEL",
    ] {
        assert!(!debug.contains(sentinel), "debug leaked {sentinel}");
    }
}
