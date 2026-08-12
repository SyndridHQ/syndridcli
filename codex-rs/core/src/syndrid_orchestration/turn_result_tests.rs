use super::super::execution_budget::ExecutionBudgetLimits;
use super::super::execution_budget_accounting::BudgetExhaustion;
use super::super::execution_budget_accounting::BudgetExhaustionCategory;
use super::super::execution_budget_accounting::ExecutionBudgetSnapshot;
use super::super::execution_modes::ExecutionModeSelection;
use super::super::execution_modes::PolicySource;
use super::super::execution_modes::ResolvedExecutionPolicyExplanation;
use super::super::live_coordinator_types::LiveOrchestrationError;
use super::super::live_coordinator_types::LiveOrchestrationOutcome;
use super::super::live_coordinator_types::LiveOrchestrationTerminal;
use super::super::live_coordinator_types::LiveRoleOutcome;
use super::super::live_coordinator_types::LiveRoleState;
use super::super::orchestration_observability::ObservationCleanupState;
use super::super::orchestration_observability::ObservationFailureState;
use super::super::orchestration_observability::ObservationProviderUsage;
use super::super::orchestration_observability::ObservationQuality;
use super::super::orchestration_observability::ObservationRepairState;
use super::super::orchestration_observability::ObservationTaskCounts;
use super::super::orchestration_observability::ObservationTerminalReason;
use super::super::orchestration_observability::ObservationToolUsage;
use super::super::orchestration_observability::Observed;
use super::super::orchestration_observability::OrchestrationObservationSnapshot;
use super::super::orchestration_observability::OrchestrationObservationStage;
use super::super::routing_profiles::RoutingProfileId;
use super::super::routing_profiles::RoutingRole;
use super::super::session_execution::SessionExecutionStatus;
use super::*;
use std::time::Duration;

fn exact<T>(value: T) -> Observed<T> {
    Observed {
        value: Some(value),
        quality: ObservationQuality::Exact,
    }
}

fn unavailable<T>() -> Observed<T> {
    Observed {
        value: None,
        quality: ObservationQuality::Unavailable,
    }
}

fn snapshot(
    terminal: LiveOrchestrationTerminal,
    cleanup_complete: bool,
) -> OrchestrationObservationSnapshot {
    OrchestrationObservationSnapshot {
        generation: exact(7),
        run_id: exact("run-7".to_string()),
        selected_mode: exact(ExecutionModeSelection::Balanced),
        policy_source: exact(PolicySource::BuiltIn(
            super::super::execution_modes::BuiltInExecutionMode::Balanced,
        )),
        routing_profile_id: exact(RoutingProfileId::new("default").unwrap()),
        lifecycle: exact(match terminal {
            LiveOrchestrationTerminal::Completed => SessionExecutionStatus::Completed,
            LiveOrchestrationTerminal::Cancelled => SessionExecutionStatus::Cancelled,
            LiveOrchestrationTerminal::TimedOut => SessionExecutionStatus::TimedOut,
            LiveOrchestrationTerminal::Failed | LiveOrchestrationTerminal::BudgetExhausted => {
                SessionExecutionStatus::Failed
            }
        }),
        stage: exact(OrchestrationObservationStage::Terminal),
        active_role: exact(super::super::orchestration_observability::ObservedActiveRole::None),
        terminal: exact(Some(terminal)),
        terminal_reason: exact(Some(match terminal {
            LiveOrchestrationTerminal::Completed => ObservationTerminalReason::Completed,
            LiveOrchestrationTerminal::Cancelled => ObservationTerminalReason::Cancelled,
            LiveOrchestrationTerminal::TimedOut => ObservationTerminalReason::TimedOut,
            LiveOrchestrationTerminal::BudgetExhausted => {
                ObservationTerminalReason::BudgetExhausted(BudgetExhaustion {
                    category: BudgetExhaustionCategory::TotalProviderInvocations,
                    limit: 1,
                    consumed_or_reserved: 1,
                    role: None,
                })
            }
            LiveOrchestrationTerminal::Failed => {
                ObservationTerminalReason::InternalCoordinatorFailure
            }
        })),
        synthesis_permitted: exact(matches!(terminal, LiveOrchestrationTerminal::Completed)),
        tasks: ObservationTaskCounts {
            total: exact(0),
            queued: exact(0),
            active: exact(0),
            completed: exact(0),
            failed: exact(0),
            cancelled: exact(0),
            outcomes_available: exact(0),
        },
        provider: ObservationProviderUsage {
            reserved: exact(0),
            started: exact(0),
            completed: exact(0),
            cancelled_after_start: exact(0),
            failed_after_start: exact(0),
            rejected_before_start: exact(0),
            by_role: Vec::new(),
            input_tokens: unavailable(),
            output_tokens: unavailable(),
            cached_input_tokens: unavailable(),
        },
        tools: ObservationToolUsage {
            reserved: exact(0),
            started: exact(0),
            completed: exact(0),
            rejected: exact(0),
            output_bytes: unavailable(),
        },
        budgets: Vec::new(),
        current_provider_count: exact(0),
        current_tool_count: exact(0),
        current_executor_concurrency: exact(0),
        peak_executor_concurrency: exact(0),
        elapsed: exact(Duration::ZERO),
        configured_timeout: unavailable(),
        remaining_time: unavailable(),
        timed_out: exact(matches!(terminal, LiveOrchestrationTerminal::TimedOut)),
        cancelled: exact(matches!(terminal, LiveOrchestrationTerminal::Cancelled)),
        cleanup_pending: exact(!cleanup_complete),
        repair: ObservationRepairState {
            enabled: unavailable(),
            eligible: unavailable(),
            attempted: unavailable(),
            attempts: unavailable(),
            result: unavailable(),
            timed_out: unavailable(),
        },
        failure: ObservationFailureState {
            accepted_cause: unavailable(),
            affected_role: unavailable(),
            retryability: unavailable(),
            join_failure: unavailable(),
        },
        cleanup: ObservationCleanupState {
            requested: exact(true),
            in_progress: exact(!cleanup_complete),
            complete: exact(cleanup_complete),
            active_planner_children: exact(0),
            active_executor_children: exact(0),
            active_verifier_children: exact(0),
            active_repair_children: exact(0),
            active_provider_children: exact(0),
            active_tool_children: exact(0),
            unresolved_provider_reservations: exact(0),
            unresolved_tool_reservations: exact(0),
        },
    }
}

fn budget() -> ExecutionBudgetSnapshot {
    ExecutionBudgetSnapshot {
        limits: ExecutionBudgetLimits {
            max_provider_invocations: 1,
            max_planner_provider_invocations: 1,
            max_executor_provider_invocations: 1,
            max_verifier_provider_invocations: 1,
            max_repair_provider_invocations: 1,
            max_tool_calls: 1,
            max_tool_output_bytes: 1,
            max_context_bytes: 1,
            max_output_tokens: 1,
            max_executor_tasks: 1,
            max_executor_concurrency: 1,
            max_repair_attempts: 1,
            max_elapsed: Duration::from_secs(1),
            max_repair_elapsed: Duration::from_secs(1),
            max_depth: 1,
        },
        provider_reserved: 0,
        provider_started: 0,
        provider_completed: 0,
        provider_cancelled: 0,
        provider_failed: 0,
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
    }
}

fn outcome(
    terminal: LiveOrchestrationTerminal,
    cleanup_complete: bool,
) -> LiveOrchestrationOutcome {
    LiveOrchestrationOutcome {
        run_id: "run-7".to_string(),
        selected_mode: ExecutionModeSelection::Balanced,
        resolved_policy: panic_policy(),
        routing_profile_id: RoutingProfileId::new("default").unwrap(),
        terminal,
        roles: vec![LiveRoleOutcome {
            role: RoutingRole::Verifier,
            state: LiveRoleState::Succeeded,
            skip_reason: None,
            task_ids: Vec::new(),
            task_states: Vec::new(),
            provider_invocations: 1,
            tool_calls: 0,
            repair_result: None,
            repair_attempts: 0,
        }],
        peak_concurrency: 1,
        provider_invocations: 1,
        tool_calls: 0,
        cancelled: terminal == LiveOrchestrationTerminal::Cancelled,
        timed_out: terminal == LiveOrchestrationTerminal::TimedOut,
        budget_exhausted: terminal == LiveOrchestrationTerminal::BudgetExhausted,
        terminal_error: None,
        synthesis_permitted: terminal == LiveOrchestrationTerminal::Completed,
        events: Vec::new(),
        budget: budget(),
        budget_exhaustion_category: None,
        observation: snapshot(terminal, cleanup_complete),
        failure: None,
    }
}

fn panic_policy() -> ResolvedExecutionPolicyExplanation {
    ResolvedExecutionPolicyExplanation {
        selected_mode: ExecutionModeSelection::Balanced,
        source: PolicySource::BuiltIn(
            super::super::execution_modes::BuiltInExecutionMode::Balanced,
        ),
        roles: Vec::new(),
        max_subagents: 1,
        max_concurrency: 1,
        max_provider_invocations: 1,
        max_tool_calls: 1,
        max_tool_output_bytes: 1,
        max_repair_attempts: 1,
        task_timeout: Duration::from_secs(1),
        batch_timeout: Duration::from_secs(1),
        repair_timeout: Duration::from_secs(1),
        context_budget_bytes: 1,
        output_budget_tokens: 1,
        max_final_response_tokens: 1,
        optional_roles_may_skip: true,
        shape: super::super::execution_modes::ExecutionShape::SinglePass,
    }
}

#[test]
fn bounded_response_rejects_oversized_text() {
    let result = UserFacingResponse::new("x".repeat(MAX_USER_FACING_RESPONSE_BYTES + 1));
    assert_eq!(
        result,
        Err(UserFacingResponseError {
            actual_bytes: MAX_USER_FACING_RESPONSE_BYTES + 1,
            max_bytes: MAX_USER_FACING_RESPONSE_BYTES,
        })
    );
}

#[test]
fn bounded_response_preserves_multibyte_utf8_boundaries() {
    let response = UserFacingResponse::new("é".repeat(MAX_USER_FACING_RESPONSE_BYTES / 2))
        .expect("multibyte response at the byte limit should be accepted");
    assert_eq!(response.as_str().len(), MAX_USER_FACING_RESPONSE_BYTES);
    assert!(response.as_str().is_char_boundary(response.as_str().len()));
    assert!(UserFacingResponse::new(format!("{}é", response.as_str())).is_err());
}

#[test]
fn successful_outcome_maps_to_completed_and_preserves_evidence_separately() {
    let outcome = outcome(LiveOrchestrationTerminal::Completed, true);
    let observation = outcome.observation.clone();
    let result = OrchestrationTurnResultBuilder::build(
        &outcome,
        Some(UserFacingResponse::new("done").unwrap()),
    );
    assert!(matches!(result, OrchestrationTurnResult::Completed { .. }));
    assert_eq!(outcome.observation, observation);
}

#[test]
fn no_response_completed_outcome_is_intentional_noop() {
    let result = OrchestrationTurnResultBuilder::build(
        &outcome(LiveOrchestrationTerminal::Completed, true),
        None,
    );
    match result {
        OrchestrationTurnResult::Completed { response, .. } => {
            assert_eq!(
                response.as_str(),
                "The orchestration completed without an additional response."
            );
        }
        _ => panic!("expected completed result"),
    }
}

#[test]
fn terminal_outcomes_remain_distinct_and_cleanup_failure_does_not_succeed() {
    assert!(matches!(
        OrchestrationTurnResultBuilder::build(
            &outcome(LiveOrchestrationTerminal::Cancelled, true),
            None
        ),
        OrchestrationTurnResult::Cancelled { .. }
    ));
    assert!(matches!(
        OrchestrationTurnResultBuilder::build(
            &outcome(LiveOrchestrationTerminal::TimedOut, true),
            None
        ),
        OrchestrationTurnResult::TimedOut { .. }
    ));
    assert!(matches!(
        OrchestrationTurnResultBuilder::build(
            &outcome(LiveOrchestrationTerminal::BudgetExhausted, true),
            None
        ),
        OrchestrationTurnResult::BudgetExhausted { .. }
    ));
    assert!(matches!(
        OrchestrationTurnResultBuilder::build(
            &outcome(LiveOrchestrationTerminal::Completed, false),
            None
        ),
        OrchestrationTurnResult::CleanupIncomplete { .. }
    ));
}

#[test]
fn verification_failure_is_failed_and_does_not_expose_raw_error_material() {
    let mut outcome = outcome(LiveOrchestrationTerminal::Failed, true);
    outcome.terminal_error = Some(LiveOrchestrationError::VerifierRejected);
    let result = OrchestrationTurnResultBuilder::build(&outcome, None);
    match result {
        OrchestrationTurnResult::Failed {
            failure,
            user_message,
            ..
        } => {
            assert_eq!(failure.kind, OrchestrationFailureKind::VerifierRejected);
            assert!(!user_message.as_str().contains("provider"));
            assert!(!user_message.as_str().contains("tool"));
        }
        _ => panic!("expected failed result"),
    }
}

#[test]
fn repair_exhaustion_remains_partial() {
    let mut outcome = outcome(LiveOrchestrationTerminal::Failed, true);
    outcome.failure = Some(OrchestrationFailure {
        kind: OrchestrationFailureKind::RepairFailed,
        retryability: super::super::orchestration_failure::Retryability::NotRetryable,
        role: Some(RoutingRole::Repair),
        tool: None,
        terminal: LiveOrchestrationTerminal::Failed,
    });
    let result = OrchestrationTurnResultBuilder::build(&outcome, None);
    assert!(matches!(
        result,
        OrchestrationTurnResult::Partial {
            cause: OrchestrationPartialCause::RepairExhausted,
            ..
        }
    ));
}

#[test]
fn completed_without_synthesis_permission_is_partial() {
    let mut outcome = outcome(LiveOrchestrationTerminal::Completed, true);
    outcome.synthesis_permitted = false;
    let result = OrchestrationTurnResultBuilder::build(&outcome, None);
    assert!(matches!(
        result,
        OrchestrationTurnResult::Partial {
            cause: OrchestrationPartialCause::ResponseUnavailable,
            ..
        }
    ));
}
