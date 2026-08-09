//! Pure advisory routing recommendations derived from strategy evaluations.
//!
//! This module consumes the Phase 9B candidate/evidence authority only. It does not select,
//! install, reserve, rotate, persist, or invoke a production route.

use super::RoutingStrategyCandidate;
use super::RoutingStrategyEligibilityEvidence;
use super::RoutingStrategyEvaluation;
use super::RoutingStrategyEvaluationOutcome;
use super::RoutingStrategyInformationalEvidence;
use super::RoutingStrategyNoSelectionReason;
use std::fmt;

const MAX_RECOMMENDATION_REASONS: usize = 16;

/// A bounded fact explaining an advisory routing recommendation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingRecommendationReason {
    Configured,
    Eligible,
    RoleCompatible,
    AccountReady,
    ConnectionReady,
    CapabilityValidated,
    PoolHasEligibleTargets,
    HigherConfiguredOrder,
    AlternativeCoolingDown,
}

/// A bounded reason why no advisory routing recommendation is available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingRecommendationUnavailableReason {
    NoConfiguredCandidates,
    NoEligibleCandidates,
    AllCandidatesCoolingDown,
    CandidateSetAmbiguous,
}

/// One configured candidate recommended by the pure advisory authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingRecommendation {
    runtime_generation: u64,
    candidate: RoutingStrategyCandidate,
    reasons: Vec<RoutingRecommendationReason>,
}

impl RoutingRecommendation {
    pub fn runtime_generation(&self) -> u64 {
        self.runtime_generation
    }

    pub fn candidate(&self) -> &RoutingStrategyCandidate {
        &self.candidate
    }

    pub fn reasons(&self) -> &[RoutingRecommendationReason] {
        &self.reasons
    }
}

/// The bounded outcome of one advisory routing evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingRecommendationOutcome {
    Recommended(RoutingRecommendation),
    Unavailable(RoutingRecommendationUnavailableReason),
}

/// An immutable recommendation snapshot stamped with the evaluated runtime generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingRecommendationSnapshot {
    runtime_generation: u64,
    outcome: RoutingRecommendationOutcome,
}

impl RoutingRecommendationSnapshot {
    pub fn runtime_generation(&self) -> u64 {
        self.runtime_generation
    }

    pub fn outcome(&self) -> &RoutingRecommendationOutcome {
        &self.outcome
    }

    /// Rejects a snapshot when its captured runtime generation is no longer current.
    pub fn validate_generation(
        &self,
        expected_generation: u64,
    ) -> Result<(), super::RoutingStrategyGenerationMismatch> {
        if self.runtime_generation != expected_generation {
            return Err(super::RoutingStrategyGenerationMismatch {
                expected: expected_generation,
                actual: self.runtime_generation,
            });
        }
        Ok(())
    }
}

/// Derives a recommendation from an already-evaluated Phase 9B candidate snapshot.
pub fn derive_routing_recommendation(
    evaluation: &RoutingStrategyEvaluation,
) -> RoutingRecommendationSnapshot {
    let outcome = match evaluation.outcome() {
        RoutingStrategyEvaluationOutcome::CandidatesAvailable { .. } => {
            let candidate = evaluation
                .eligible_candidates()
                .next()
                .expect("candidate evaluation with eligible candidates must contain one");
            let candidate_index = evaluation
                .candidates()
                .iter()
                .position(|current| current.candidate().id() == candidate.candidate().id())
                .expect("eligible candidate must belong to its evaluation");
            let mut reasons = vec![
                RoutingRecommendationReason::Configured,
                RoutingRecommendationReason::Eligible,
            ];
            for evidence in candidate.evidence() {
                let reason = match evidence {
                    super::RoutingStrategyEvidence::Informational(
                        RoutingStrategyInformationalEvidence::Configured,
                    ) => None,
                    super::RoutingStrategyEvidence::Informational(_) => None,
                    super::RoutingStrategyEvidence::Eligibility(evidence) => match evidence {
                        RoutingStrategyEligibilityEvidence::RoleCompatible => {
                            Some(RoutingRecommendationReason::RoleCompatible)
                        }
                        RoutingStrategyEligibilityEvidence::AccountReady => {
                            Some(RoutingRecommendationReason::AccountReady)
                        }
                        RoutingStrategyEligibilityEvidence::ConnectionReady => {
                            Some(RoutingRecommendationReason::ConnectionReady)
                        }
                        RoutingStrategyEligibilityEvidence::CapabilityValidated => {
                            Some(RoutingRecommendationReason::CapabilityValidated)
                        }
                        RoutingStrategyEligibilityEvidence::PoolHasEligibleTargets => {
                            Some(RoutingRecommendationReason::PoolHasEligibleTargets)
                        }
                        RoutingStrategyEligibilityEvidence::CooldownAvailable
                        | RoutingStrategyEligibilityEvidence::CoolingDown { .. }
                        | RoutingStrategyEligibilityEvidence::AllPoolTargetsCooling { .. } => None,
                    },
                    super::RoutingStrategyEvidence::Ordering(_) => None,
                };
                if let Some(reason) = reason
                    && !reasons.contains(&reason)
                    && reasons.len() < MAX_RECOMMENDATION_REASONS
                {
                    reasons.push(reason);
                }
            }
            if candidate_index > 0 {
                reasons.push(RoutingRecommendationReason::HigherConfiguredOrder);
                if evaluation.candidates()[..candidate_index]
                    .iter()
                    .any(|previous| previous.eligibility().is_cooling_down())
                {
                    reasons.push(RoutingRecommendationReason::AlternativeCoolingDown);
                }
            }
            RoutingRecommendationOutcome::Recommended(RoutingRecommendation {
                runtime_generation: evaluation.runtime_generation(),
                candidate: candidate.candidate().clone(),
                reasons,
            })
        }
        RoutingStrategyEvaluationOutcome::NoSelection(reason) => {
            RoutingRecommendationOutcome::Unavailable(match reason {
                RoutingStrategyNoSelectionReason::NoConfiguredCandidates => {
                    RoutingRecommendationUnavailableReason::NoConfiguredCandidates
                }
                RoutingStrategyNoSelectionReason::NoEligibleCandidates => {
                    RoutingRecommendationUnavailableReason::NoEligibleCandidates
                }
                RoutingStrategyNoSelectionReason::AllCandidatesCoolingDown => {
                    RoutingRecommendationUnavailableReason::AllCandidatesCoolingDown
                }
                RoutingStrategyNoSelectionReason::CandidateSetAmbiguous => {
                    RoutingRecommendationUnavailableReason::CandidateSetAmbiguous
                }
            })
        }
    };
    RoutingRecommendationSnapshot {
        runtime_generation: evaluation.runtime_generation(),
        outcome,
    }
}

impl fmt::Display for RoutingRecommendationUnavailableReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoConfiguredCandidates => "no configured routing candidates",
            Self::NoEligibleCandidates => "no configured routing candidates are eligible",
            Self::AllCandidatesCoolingDown => "all configured routing candidates are cooling down",
            Self::CandidateSetAmbiguous => "configured routing candidate order is ambiguous",
        })
    }
}

#[cfg(test)]
#[path = "routing_recommendation_tests.rs"]
mod tests;
