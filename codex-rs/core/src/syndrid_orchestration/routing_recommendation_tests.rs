use super::super::account_pools::AccountPoolTarget;
use super::super::account_pools::PoolId;
use super::super::codex_accounts::CodexAccountProfileId;
use super::super::provider_failure::ProviderFailureClass;
use super::super::routing_profiles::RoutingProfileId;
use super::super::routing_profiles::RoutingRole;
use super::super::strategy_candidates::*;
use super::*;
use pretty_assertions::assert_eq;
use std::time::Duration;

fn candidate(name: &str) -> RoutingStrategyCandidate {
    RoutingStrategyCandidate::new(RoutingStrategyCandidateId::new(
        RoutingProfileId::new("profile").expect("profile"),
        RoutingRole::Planner,
        RoutingStrategyCandidateTarget::direct(
            AccountPoolTarget::native_codex(CodexAccountProfileId::new(name).expect("account")),
            "codex",
            "gpt-5",
        )
        .expect("target"),
    ))
}

fn snapshot(
    candidate: RoutingStrategyCandidate,
    eligibility: RoutingStrategyEligibility,
    evidence: Vec<RoutingStrategyEvidence>,
) -> RoutingStrategyCandidateSnapshot {
    RoutingStrategyCandidateSnapshot::new(candidate, evidence, eligibility).expect("snapshot")
}

fn eligible(name: &str) -> RoutingStrategyCandidateSnapshot {
    snapshot(
        candidate(name),
        RoutingStrategyEligibility::Eligible,
        vec![
            RoutingStrategyEvidence::Informational(
                RoutingStrategyInformationalEvidence::Configured,
            ),
            RoutingStrategyEvidence::Eligibility(
                RoutingStrategyEligibilityEvidence::RoleCompatible,
            ),
            RoutingStrategyEvidence::Eligibility(RoutingStrategyEligibilityEvidence::AccountReady),
            RoutingStrategyEvidence::Eligibility(
                RoutingStrategyEligibilityEvidence::CapabilityValidated,
            ),
        ],
    )
}

#[test]
fn recommendation_uses_first_eligible_configured_candidate_deterministically() {
    let input = RoutingStrategyEvaluationInput::configured(
        7,
        vec![
            snapshot(
                candidate("cooling"),
                RoutingStrategyEligibility::Ineligible(RoutingStrategyIneligibility::CoolingDown {
                    remaining: Duration::from_secs(3),
                    failure_class: ProviderFailureClass::Timeout,
                }),
                vec![RoutingStrategyEvidence::Eligibility(
                    RoutingStrategyEligibilityEvidence::CoolingDown {
                        remaining: Duration::from_secs(3),
                        failure_class: ProviderFailureClass::Timeout,
                    },
                )],
            ),
            eligible("ready"),
        ],
    )
    .expect("input");
    let evaluation = evaluate_routing_strategy_candidates(input, 7).expect("evaluation");

    let first = derive_routing_recommendation(&evaluation);
    let second = derive_routing_recommendation(&evaluation);

    assert_eq!(first, second);
    assert_eq!(first.runtime_generation(), 7);
    let RoutingRecommendationOutcome::Recommended(recommendation) = first.outcome() else {
        panic!("expected recommendation");
    };
    assert_eq!(recommendation.candidate(), &candidate("ready"));
    assert_eq!(
        recommendation.reasons(),
        &[
            RoutingRecommendationReason::Configured,
            RoutingRecommendationReason::Eligible,
            RoutingRecommendationReason::RoleCompatible,
            RoutingRecommendationReason::AccountReady,
            RoutingRecommendationReason::CapabilityValidated,
            RoutingRecommendationReason::HigherConfiguredOrder,
            RoutingRecommendationReason::AlternativeCoolingDown,
        ]
    );
}

#[test]
fn no_selection_outcomes_remain_typed() {
    let cases = [
        (
            RoutingStrategyEvaluationInput::configured(1, Vec::new()),
            RoutingRecommendationUnavailableReason::NoConfiguredCandidates,
        ),
        (
            Ok(RoutingStrategyEvaluationInput::ambiguous(1)),
            RoutingRecommendationUnavailableReason::CandidateSetAmbiguous,
        ),
    ];
    for (input, reason) in cases {
        let evaluation =
            evaluate_routing_strategy_candidates(input.expect("input"), 1).expect("evaluation");
        assert_eq!(
            derive_routing_recommendation(&evaluation).outcome(),
            &RoutingRecommendationOutcome::Unavailable(reason)
        );
    }

    let unavailable = snapshot(
        candidate("unavailable"),
        RoutingStrategyEligibility::Ineligible(RoutingStrategyIneligibility::AccountUnavailable),
        vec![],
    );
    let evaluation = evaluate_routing_strategy_candidates(
        RoutingStrategyEvaluationInput::configured(1, vec![unavailable]).expect("input"),
        1,
    )
    .expect("evaluation");
    assert_eq!(
        derive_routing_recommendation(&evaluation).outcome(),
        &RoutingRecommendationOutcome::Unavailable(
            RoutingRecommendationUnavailableReason::NoEligibleCandidates
        )
    );

    let cooling = snapshot(
        candidate("cooling"),
        RoutingStrategyEligibility::Ineligible(RoutingStrategyIneligibility::CoolingDown {
            remaining: Duration::from_secs(2),
            failure_class: ProviderFailureClass::RateLimited,
        }),
        vec![],
    );
    let evaluation = evaluate_routing_strategy_candidates(
        RoutingStrategyEvaluationInput::configured(1, vec![cooling]).expect("input"),
        1,
    )
    .expect("evaluation");
    assert_eq!(
        derive_routing_recommendation(&evaluation).outcome(),
        &RoutingRecommendationOutcome::Unavailable(
            RoutingRecommendationUnavailableReason::AllCandidatesCoolingDown
        )
    );
}

#[test]
fn generation_validation_rejects_a_stale_snapshot_without_expiry_or_mutation() {
    let evaluation = evaluate_routing_strategy_candidates(
        RoutingStrategyEvaluationInput::configured(8, vec![eligible("ready")]).expect("input"),
        8,
    )
    .expect("evaluation");
    let recommendation = derive_routing_recommendation(&evaluation);
    let before = recommendation.clone();

    assert_eq!(
        recommendation.validate_generation(9),
        Err(RoutingStrategyGenerationMismatch {
            expected: 9,
            actual: 8,
        })
    );
    assert_eq!(recommendation, before);
}

#[test]
fn recommendation_preserves_pool_and_account_identity_without_rotation_state() {
    let pool_id = PoolId::new("research").expect("pool");
    let pool_target =
        RoutingStrategyCandidateTarget::pool(pool_id, "codex", "gpt-5").expect("pool target");
    let pool_candidate = RoutingStrategyCandidate::new(RoutingStrategyCandidateId::new(
        RoutingProfileId::new("profile").expect("profile"),
        RoutingRole::Executor,
        pool_target.clone(),
    ));
    let input = RoutingStrategyEvaluationInput::configured(
        4,
        vec![snapshot(
            pool_candidate.clone(),
            RoutingStrategyEligibility::Eligible,
            vec![RoutingStrategyEvidence::Eligibility(
                RoutingStrategyEligibilityEvidence::PoolHasEligibleTargets,
            )],
        )],
    )
    .expect("input");
    let evaluation = evaluate_routing_strategy_candidates(input, 4).expect("evaluation");
    let recommendation = derive_routing_recommendation(&evaluation);

    let RoutingRecommendationOutcome::Recommended(recommendation) = recommendation.outcome() else {
        panic!("expected recommendation");
    };
    assert_eq!(recommendation.candidate(), &pool_candidate);
    assert_eq!(recommendation.candidate().id().target(), &pool_target);
    assert!(!format!("{recommendation:?}").contains("cursor"));
    assert!(!format!("{recommendation:?}").contains("reservation"));
}

#[test]
fn recommendation_debug_is_presentation_safe() {
    let evaluation = evaluate_routing_strategy_candidates(
        RoutingStrategyEvaluationInput::configured(2, vec![eligible("safe-account")])
            .expect("input"),
        2,
    )
    .expect("evaluation");
    let recommendation = derive_routing_recommendation(&evaluation);
    let debug = format!("{recommendation:?}");

    assert!(debug.contains("safe-account"));
    assert!(!debug.contains("credential"));
    assert!(!debug.contains("authorization"));
    assert!(!debug.contains("token"));
    assert!(!debug.contains("score"));
}
