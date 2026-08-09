use super::*;
use crate::syndrid_orchestration::AccountPoolTarget;
use crate::syndrid_orchestration::CodexAccountProfileId;
use crate::syndrid_orchestration::RoutingProfileId;
use crate::syndrid_orchestration::RoutingRole;
use crate::syndrid_orchestration::RoutingStrategyCandidateId;
use crate::syndrid_orchestration::RoutingStrategyCandidateSnapshot;
use crate::syndrid_orchestration::RoutingStrategyCandidateTarget;
use crate::syndrid_orchestration::RoutingStrategyEligibility;
use crate::syndrid_orchestration::RoutingStrategyEligibilityEvidence;
use crate::syndrid_orchestration::RoutingStrategyEvaluationInput;
use crate::syndrid_orchestration::RoutingStrategyEvidence;
use crate::syndrid_orchestration::RoutingStrategyInformationalEvidence;
use crate::syndrid_orchestration::evaluate_routing_strategy_candidates;
use pretty_assertions::assert_eq;

fn candidate(name: &str) -> super::super::RoutingStrategyCandidate {
    let target = RoutingStrategyCandidateTarget::direct(
        AccountPoolTarget::NativeCodexAccount(CodexAccountProfileId::new(name).expect("account")),
        "codex",
        "gpt-5",
    )
    .expect("target");
    super::super::RoutingStrategyCandidate::new(RoutingStrategyCandidateId::new(
        RoutingProfileId::new("automatic").expect("profile"),
        RoutingRole::Main,
        target,
    ))
}

fn snapshot(
    candidate: super::super::RoutingStrategyCandidate,
    eligibility: RoutingStrategyEligibility,
) -> RoutingStrategyCandidateSnapshot {
    RoutingStrategyCandidateSnapshot::new(
        candidate,
        vec![
            RoutingStrategyEvidence::Informational(
                RoutingStrategyInformationalEvidence::Configured,
            ),
            RoutingStrategyEvidence::Eligibility(
                RoutingStrategyEligibilityEvidence::RoleCompatible,
            ),
        ],
        eligibility,
    )
    .expect("candidate snapshot")
}

#[test]
fn automatic_uses_first_eligible_configured_candidate() {
    let input = RoutingStrategyEvaluationInput::configured(
        9,
        vec![
            snapshot(
                candidate("cooling"),
                RoutingStrategyEligibility::Ineligible(
                    super::super::RoutingStrategyIneligibility::CoolingDown {
                        remaining: std::time::Duration::from_secs(5),
                        failure_class: super::super::ProviderFailureClass::RateLimited,
                    },
                ),
            ),
            snapshot(candidate("ready"), RoutingStrategyEligibility::Eligible),
        ],
    )
    .expect("configured candidates");
    let evaluation = evaluate_routing_strategy_candidates(input, 9).expect("evaluation");
    let automatic = derive_automatic_routing_decision(&evaluation);
    assert_eq!(
        automatic.outcome(),
        &AutomaticRoutingDecisionOutcome::Selected(AutomaticRoutingDecision {
            runtime_generation: 9,
            candidate: candidate("ready"),
            reasons: vec![
                AutomaticRoutingReason::Configured,
                AutomaticRoutingReason::Eligible,
                AutomaticRoutingReason::RoleCompatible,
                AutomaticRoutingReason::HigherConfiguredOrder,
                AutomaticRoutingReason::AlternativeCoolingDown,
            ],
        })
    );
}

#[test]
fn automatic_preserves_no_selection_and_generation() {
    let input = RoutingStrategyEvaluationInput::ambiguous(11);
    let evaluation = evaluate_routing_strategy_candidates(input, 11).expect("evaluation");
    let automatic = derive_automatic_routing_decision(&evaluation);
    assert_eq!(
        automatic.outcome(),
        &AutomaticRoutingDecisionOutcome::Unavailable(
            AutomaticRoutingUnavailableReason::CandidateSetAmbiguous
        )
    );
    assert_eq!(automatic.runtime_generation(), 11);
    assert!(automatic.validate_generation(12).is_err());
}
