use super::super::account_pools::AccountPoolTarget;
use super::super::codex_accounts::CodexAccountProfileId;
use super::super::cooldown_state::ProviderCooldownStatus;
use super::super::provider_failure::ProviderFailureClass;
use super::*;
use pretty_assertions::assert_eq;
use std::time::Duration;

fn profile_id() -> RoutingProfileId {
    RoutingProfileId::new("manual").expect("profile id")
}

fn direct_candidate(role: RoutingRole, connection_id: &str) -> RoutingStrategyCandidate {
    RoutingStrategyCandidate::new(RoutingStrategyCandidateId::new(
        profile_id(),
        role,
        RoutingStrategyCandidateTarget::direct(
            AccountPoolTarget::omniroute(connection_id).expect("target"),
            "codex",
            "gpt-5",
        )
        .expect("candidate target"),
    ))
}

fn snapshot(
    candidate: RoutingStrategyCandidate,
    eligibility: RoutingStrategyEligibility,
) -> RoutingStrategyCandidateSnapshot {
    RoutingStrategyCandidateSnapshot::new(
        candidate,
        vec![RoutingStrategyEvidence::Informational(
            RoutingStrategyInformationalEvidence::Configured,
        )],
        eligibility,
    )
    .expect("candidate snapshot")
}

#[test]
fn configured_order_is_preserved_without_selection() {
    let first = snapshot(
        direct_candidate(RoutingRole::Main, "account-a"),
        RoutingStrategyEligibility::Eligible,
    );
    let second = snapshot(
        direct_candidate(RoutingRole::Main, "account-b"),
        RoutingStrategyEligibility::Eligible,
    );
    let input = RoutingStrategyEvaluationInput::configured(7, vec![first, second])
        .expect("configured candidates");

    let result = evaluate_routing_strategy_candidates(input, 7).expect("evaluation");

    assert_eq!(
        result
            .candidates()
            .iter()
            .map(|candidate| candidate.candidate().id().target().clone())
            .collect::<Vec<_>>(),
        vec![
            RoutingStrategyCandidateTarget::direct(
                AccountPoolTarget::omniroute("account-a").expect("target"),
                "codex",
                "gpt-5",
            )
            .expect("candidate target"),
            RoutingStrategyCandidateTarget::direct(
                AccountPoolTarget::omniroute("account-b").expect("target"),
                "codex",
                "gpt-5",
            )
            .expect("candidate target"),
        ]
    );
    assert_eq!(
        result.outcome(),
        &RoutingStrategyEvaluationOutcome::CandidatesAvailable { eligible_count: 2 }
    );
    assert_eq!(
        result.candidates()[0].evidence().last(),
        Some(&RoutingStrategyEvidence::Ordering(
            RoutingStrategyConfiguredOrder { position: 0 }
        ))
    );
}

#[test]
fn duplicate_candidate_identity_is_rejected() {
    let candidate = snapshot(
        direct_candidate(RoutingRole::Main, "account-a"),
        RoutingStrategyEligibility::Eligible,
    );
    let duplicate = candidate.clone();

    assert_eq!(
        RoutingStrategyEvaluationInput::configured(1, vec![candidate, duplicate]),
        Err(RoutingStrategyCandidateError::DuplicateCandidate)
    );
}

#[test]
fn direct_candidate_preserves_native_account_identity() {
    let account = CodexAccountProfileId::new("account-a").expect("account id");
    let target = RoutingStrategyCandidateTarget::direct(
        AccountPoolTarget::native_codex(account.clone()),
        "codex",
        "gpt-5",
    )
    .expect("candidate target");

    assert_eq!(
        target.direct_target(),
        Some(&AccountPoolTarget::native_codex(account))
    );
    assert_eq!(target.pool_id(), None);
}

#[test]
fn canonical_target_identity_distinguishes_targets_and_candidate_kinds() {
    let account_a = AccountPoolTarget::native_codex(
        CodexAccountProfileId::new("account-a").expect("account id"),
    );
    let account_b = AccountPoolTarget::native_codex(
        CodexAccountProfileId::new("account-b").expect("account id"),
    );
    assert_ne!(account_a, account_b);

    let connection_a = AccountPoolTarget::omniroute("connection-a").expect("connection");
    let connection_b = AccountPoolTarget::omniroute("connection-b").expect("connection");
    assert_ne!(connection_a, connection_b);

    let direct_a = RoutingStrategyCandidateTarget::direct(account_a.clone(), "codex", "gpt-5")
        .expect("direct target");
    let direct_a_again =
        RoutingStrategyCandidateTarget::direct(account_a, "codex", "gpt-5").expect("direct");
    let direct_b = RoutingStrategyCandidateTarget::direct(connection_a, "codex", "gpt-5")
        .expect("direct target");
    let pool = RoutingStrategyCandidateTarget::pool(
        PoolId::new("account-a").expect("pool id"),
        "codex",
        "gpt-5",
    )
    .expect("pool target");

    assert_eq!(direct_a, direct_a_again);
    assert_ne!(direct_a, direct_b);
    assert_ne!(direct_a, pool);
}

#[test]
fn ambiguous_ordering_is_typed_without_guessing() {
    let result =
        evaluate_routing_strategy_candidates(RoutingStrategyEvaluationInput::ambiguous(4), 4)
            .expect("evaluation");

    assert_eq!(
        result.outcome(),
        &RoutingStrategyEvaluationOutcome::NoSelection(
            RoutingStrategyNoSelectionReason::CandidateSetAmbiguous
        )
    );
    assert!(result.candidates().is_empty());
}

#[test]
fn cooldown_evidence_is_read_only_and_exact() {
    let cooling = ProviderCooldownStatus::CoolingDown {
        remaining: Duration::from_secs(12),
        failure_class: ProviderFailureClass::RateLimited,
    };
    let eligibility = RoutingStrategyEligibility::from_cooldown_status(&cooling);
    let candidate = direct_candidate(RoutingRole::Executor, "account-a");
    let snapshot = RoutingStrategyCandidateSnapshot::new(
        candidate,
        vec![
            RoutingStrategyEvidence::Informational(
                RoutingStrategyInformationalEvidence::ExactTarget(
                    AccountPoolTarget::omniroute("connection-a").expect("target"),
                ),
            ),
            RoutingStrategyEvidence::Eligibility(RoutingStrategyEligibilityEvidence::CoolingDown {
                remaining: Duration::from_secs(12),
                failure_class: ProviderFailureClass::RateLimited,
            }),
        ],
        eligibility.clone(),
    )
    .expect("snapshot");

    assert_eq!(eligibility, snapshot.eligibility().clone());
    assert!(!snapshot.eligibility().is_eligible());
}

#[test]
fn all_cooled_candidates_have_typed_no_selection() {
    let cooling =
        RoutingStrategyEligibility::Ineligible(RoutingStrategyIneligibility::CoolingDown {
            remaining: Duration::from_secs(3),
            failure_class: ProviderFailureClass::Timeout,
        });
    let input = RoutingStrategyEvaluationInput::configured(
        9,
        vec![
            snapshot(
                direct_candidate(RoutingRole::Main, "account-a"),
                cooling.clone(),
            ),
            snapshot(direct_candidate(RoutingRole::Main, "account-b"), cooling),
        ],
    )
    .expect("configured candidates");

    let result = evaluate_routing_strategy_candidates(input, 9).expect("evaluation");

    assert_eq!(
        result.outcome(),
        &RoutingStrategyEvaluationOutcome::NoSelection(
            RoutingStrategyNoSelectionReason::AllCandidatesCoolingDown
        )
    );
}

#[test]
fn generation_mismatch_rejects_stale_input() {
    let input = RoutingStrategyEvaluationInput::configured(
        11,
        vec![snapshot(
            direct_candidate(RoutingRole::Verifier, "account-a"),
            RoutingStrategyEligibility::Eligible,
        )],
    )
    .expect("configured candidates");

    assert_eq!(
        evaluate_routing_strategy_candidates(input, 12),
        Err(RoutingStrategyGenerationMismatch {
            expected: 12,
            actual: 11,
        })
    );
}

#[test]
fn candidate_debug_contains_only_safe_identity_fields() {
    let candidate = direct_candidate(RoutingRole::Main, "account-a");
    let debug = format!("{candidate:?}");

    assert!(debug.contains("account-a"));
    assert!(!debug.contains("credential"));
    assert!(!debug.contains("authorization"));
    assert!(!debug.contains("token"));
}

#[test]
fn evaluation_does_not_mutate_input_or_expose_rotation_state() {
    let candidate = snapshot(
        direct_candidate(RoutingRole::Planner, "account-a"),
        RoutingStrategyEligibility::Eligible,
    );
    let input = RoutingStrategyEvaluationInput::configured(3, vec![candidate.clone()])
        .expect("configured candidates");
    let input_before = input.clone();

    let result = evaluate_routing_strategy_candidates(input.clone(), 3).expect("evaluation");

    assert_eq!(input, input_before);
    assert_eq!(result.candidates()[0].candidate(), &candidate.candidate);
    assert!(!format!("{result:?}").contains("cursor"));
    assert!(!format!("{result:?}").contains("reservation"));
}

#[test]
fn pool_identity_and_all_cooled_evidence_do_not_select_a_member() {
    let pool_id = PoolId::new("shared").expect("pool id");
    let target = RoutingStrategyCandidateTarget::pool(pool_id.clone(), "codex", "gpt-5")
        .expect("pool target");
    let candidate = RoutingStrategyCandidate::new(RoutingStrategyCandidateId::new(
        profile_id(),
        RoutingRole::Executor,
        target.clone(),
    ));
    let snapshot = RoutingStrategyCandidateSnapshot::new(
        candidate,
        vec![
            RoutingStrategyEvidence::Informational(RoutingStrategyInformationalEvidence::Pool(
                pool_id.clone(),
            )),
            RoutingStrategyEvidence::Eligibility(
                RoutingStrategyEligibilityEvidence::AllPoolTargetsCooling {
                    earliest_recovery: Some(Duration::from_secs(8)),
                },
            ),
        ],
        RoutingStrategyEligibility::Ineligible(
            RoutingStrategyIneligibility::AllPoolTargetsCooling {
                earliest_recovery: Some(Duration::from_secs(8)),
            },
        ),
    )
    .expect("pool snapshot");
    let result = evaluate_routing_strategy_candidates(
        RoutingStrategyEvaluationInput::configured(2, vec![snapshot]).expect("configured input"),
        2,
    )
    .expect("evaluation");

    assert_eq!(target.pool_id(), Some(&pool_id));
    assert_eq!(target.direct_target(), None);
    assert_eq!(
        result.outcome(),
        &RoutingStrategyEvaluationOutcome::NoSelection(
            RoutingStrategyNoSelectionReason::AllCandidatesCoolingDown
        )
    );
    assert!(!format!("{result:?}").contains("next_member"));
}

#[test]
fn no_candidates_and_mixed_ineligibility_have_distinct_outcomes() {
    let empty = evaluate_routing_strategy_candidates(
        RoutingStrategyEvaluationInput::configured(5, Vec::new()).expect("empty input"),
        5,
    )
    .expect("empty evaluation");
    assert_eq!(
        empty.outcome(),
        &RoutingStrategyEvaluationOutcome::NoSelection(
            RoutingStrategyNoSelectionReason::NoConfiguredCandidates
        )
    );

    let mixed = evaluate_routing_strategy_candidates(
        RoutingStrategyEvaluationInput::configured(
            5,
            vec![snapshot(
                direct_candidate(RoutingRole::Main, "account-a"),
                RoutingStrategyEligibility::Ineligible(
                    RoutingStrategyIneligibility::AccountUnavailable,
                ),
            )],
        )
        .expect("mixed input"),
        5,
    )
    .expect("mixed evaluation");
    assert_eq!(
        mixed.outcome(),
        &RoutingStrategyEvaluationOutcome::NoSelection(
            RoutingStrategyNoSelectionReason::NoEligibleCandidates
        )
    );
}

#[test]
fn candidate_and_evidence_bounds_are_enforced() {
    let candidate = direct_candidate(RoutingRole::Main, "account-a");
    let evidence = vec![
        RoutingStrategyEvidence::Informational(
            RoutingStrategyInformationalEvidence::Configured,
        );
        17
    ];

    assert_eq!(
        RoutingStrategyCandidateSnapshot::new(
            candidate,
            evidence,
            RoutingStrategyEligibility::Eligible,
        ),
        Err(RoutingStrategyCandidateError::TooMuchEvidence)
    );

    let candidates = (0..33)
        .map(|index| {
            snapshot(
                direct_candidate(RoutingRole::Main, &format!("account-{index}")),
                RoutingStrategyEligibility::Eligible,
            )
        })
        .collect();
    assert_eq!(
        RoutingStrategyEvaluationInput::configured(1, candidates),
        Err(RoutingStrategyCandidateError::TooManyCandidates)
    );
}
