use super::BudgetExhaustionCategory;
use super::ExecutionBudgetLedger;
use super::ExecutionModeSelection;
use super::RoutingRole;
use super::execution_budget_accounting::BudgetExhaustion;

#[test]
fn resolved_policy_limits_are_copied_without_widening() {
    for selection in [
        ExecutionModeSelection::Fast,
        ExecutionModeSelection::Balanced,
        ExecutionModeSelection::UsageSaver,
        ExecutionModeSelection::Deep,
    ] {
        let policy = selection.resolve().expect("built-in policy");
        let limits = ExecutionBudgetLedger::new(&policy).limits().clone();
        assert_eq!(
            limits.max_provider_invocations,
            policy.policy().max_provider_invocations
        );
        assert_eq!(limits.max_tool_calls, policy.policy().max_tool_calls);
        assert_eq!(
            limits.max_tool_output_bytes,
            policy.policy().max_tool_output_bytes
        );
        assert_eq!(
            limits.max_context_bytes,
            policy.policy().context_budget_bytes
        );
        assert_eq!(
            limits.max_output_tokens,
            policy.policy().output_budget_tokens
        );
        assert_eq!(limits.max_executor_tasks, policy.policy().max_subagents);
        assert_eq!(
            limits.max_executor_concurrency,
            policy.policy().max_concurrency
        );
        assert_eq!(
            limits.max_repair_attempts,
            usize::from(policy.policy().max_repair_attempts)
        );
        assert_eq!(limits.max_elapsed, policy.policy().batch_timeout);
        assert_eq!(limits.max_repair_elapsed, policy.policy().repair_timeout);
    }
}

#[test]
fn provider_limit_one_is_inclusive_and_second_invocation_is_rejected() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("fast policy");
    let ledger = ExecutionBudgetLedger::new(&policy);
    let first = ledger
        .reserve_provider(RoutingRole::Executor)
        .expect("first reservation");
    first.commit().expect("first reservation commits");
    let second = ledger
        .reserve_provider(RoutingRole::Executor)
        .expect_err("second reservation must be rejected");
    assert_eq!(
        second.category,
        BudgetExhaustionCategory::TotalProviderInvocations
    );
}

#[test]
fn task_and_repair_limits_are_inclusive() {
    let policy = ExecutionModeSelection::UsageSaver
        .resolve()
        .expect("usage saver policy");
    let ledger = ExecutionBudgetLedger::new(&policy);
    ledger.admit_executor_tasks(1).expect("one task");
    assert_eq!(
        ledger.admit_executor_tasks(1).expect_err("second task"),
        BudgetExhaustion {
            category: BudgetExhaustionCategory::ExecutorTaskCount,
            limit: 1,
            consumed_or_reserved: 2,
            role: Some(RoutingRole::Executor),
        }
    );
    assert_eq!(
        ledger.admit_repair_attempt(),
        Err(BudgetExhaustion {
            category: BudgetExhaustionCategory::RepairAttempts,
            limit: 0,
            consumed_or_reserved: 0,
            role: Some(RoutingRole::Repair),
        })
    );
}

#[test]
fn terminal_ledger_rejects_new_provider_and_tool_work() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("fast policy");
    let ledger = ExecutionBudgetLedger::new(&policy);
    ledger.mark_terminal().expect("terminalize ledger");
    assert_eq!(
        ledger
            .reserve_provider(RoutingRole::Executor)
            .expect_err("provider after terminal"),
        BudgetExhaustion {
            category: BudgetExhaustionCategory::RunTerminal,
            limit: 1,
            consumed_or_reserved: 0,
            role: Some(RoutingRole::Executor),
        }
    );
    assert_eq!(
        ledger
            .reserve_tool(RoutingRole::Executor)
            .expect_err("tool after terminal")
            .category,
        BudgetExhaustionCategory::RunTerminal
    );
}

#[test]
fn reservation_release_and_started_usage_are_distinct() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("fast policy");
    let ledger = ExecutionBudgetLedger::new(&policy);
    let reservation = ledger
        .reserve_provider(RoutingRole::Planner)
        .expect("unstarted reservation");
    drop(reservation);
    let started = ledger
        .reserve_provider(RoutingRole::Executor)
        .expect("released slot");
    started.commit().expect("started reservation commits");
    assert_eq!(ledger.snapshot().provider_started, 1);
    assert_eq!(ledger.snapshot().provider_reserved, 0);
}

#[test]
fn tool_context_output_and_snapshot_categories_are_exact() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("fast policy");
    let ledger = ExecutionBudgetLedger::new(&policy);
    let limits = ledger.limits().clone();
    ledger
        .reserve_context(limits.max_context_bytes)
        .expect("context limit");
    assert_eq!(
        ledger
            .reserve_context(1)
            .expect_err("context overflow")
            .category,
        BudgetExhaustionCategory::InputOrContextLimit
    );
    for _ in 0..limits.max_tool_calls {
        ledger
            .reserve_tool(RoutingRole::Executor)
            .expect("tool slot")
            .commit()
            .expect("tool commit");
    }
    assert_eq!(
        ledger
            .reserve_tool(RoutingRole::Executor)
            .expect_err("tool overflow")
            .category,
        BudgetExhaustionCategory::TotalToolCalls
    );
    ledger
        .record_output_tokens(u64::from(limits.max_output_tokens))
        .expect("output limit");
    assert_eq!(
        ledger
            .record_output_tokens(1)
            .expect_err("output overflow")
            .category,
        BudgetExhaustionCategory::OutputTokenLimit
    );
    assert_eq!(
        ledger
            .snapshot()
            .last_exhaustion
            .map(|error| error.category),
        Some(BudgetExhaustionCategory::OutputTokenLimit)
    );
}

#[test]
fn generation_is_preserved_and_stale_terminalization_is_rejected() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("fast policy");
    let first = ExecutionBudgetLedger::new_for_generation(&policy, 7);
    assert_eq!(first.generation(), 7);
    first.mark_terminal().expect("terminalize first");
    assert_eq!(
        first
            .reserve_provider(RoutingRole::Executor)
            .unwrap_err()
            .category,
        BudgetExhaustionCategory::RunTerminal
    );
}
