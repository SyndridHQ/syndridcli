//! Pure Automatic routing decisions derived from the trusted candidate authority.
//!
//! Automatic routing selects only from the already-evaluated configured candidates. It does not
//! construct providers, invoke providers, persist a winner, reserve a pool member, or mutate
//! cooldown and rotation state. The production dispatcher remains the authority for exact target
//! admission after this strategy-level decision.

use super::RoutingRecommendationReason;
use super::RoutingRecommendationUnavailableReason;
use super::RoutingStrategyCandidate;
use super::RoutingStrategyEvaluation;
use super::derive_routing_recommendation;

const MAX_AUTOMATIC_REASONS: usize = 16;

/// A bounded fact explaining an Automatic decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutomaticRoutingReason {
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

/// A typed reason why an installed Automatic policy cannot select a candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutomaticRoutingUnavailableReason {
    NoConfiguredCandidates,
    NoEligibleCandidates,
    AllCandidatesCoolingDown,
    CandidateSetAmbiguous,
    RuntimeGenerationMismatch,
}

/// One configured candidate selected by the pure Automatic authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutomaticRoutingDecision {
    runtime_generation: u64,
    candidate: RoutingStrategyCandidate,
    reasons: Vec<AutomaticRoutingReason>,
}

impl AutomaticRoutingDecision {
    pub fn runtime_generation(&self) -> u64 {
        self.runtime_generation
    }

    pub fn candidate(&self) -> &RoutingStrategyCandidate {
        &self.candidate
    }

    pub fn reasons(&self) -> &[AutomaticRoutingReason] {
        &self.reasons
    }
}

/// The result of one pure Automatic evaluation pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AutomaticRoutingDecisionOutcome {
    Selected(AutomaticRoutingDecision),
    Unavailable(AutomaticRoutingUnavailableReason),
}

/// An immutable Automatic decision stamped with the installed runtime generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutomaticRoutingSnapshot {
    runtime_generation: u64,
    outcome: AutomaticRoutingDecisionOutcome,
}

impl AutomaticRoutingSnapshot {
    pub fn runtime_generation(&self) -> u64 {
        self.runtime_generation
    }

    pub fn outcome(&self) -> &AutomaticRoutingDecisionOutcome {
        &self.outcome
    }

    /// Prevents a decision captured from one installed runtime from being used with another.
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

/// Derives an Automatic decision from the already-authoritative Phase 9B evaluation.
///
/// The Phase 9C recommendation authority supplies the candidate and bounded explanation so
/// Recommended and Automatic cannot drift into separate eligibility or ordering rules.
pub fn derive_automatic_routing_decision(
    evaluation: &RoutingStrategyEvaluation,
) -> AutomaticRoutingSnapshot {
    let recommendation = derive_routing_recommendation(evaluation);
    let outcome = match recommendation.outcome() {
        super::RoutingRecommendationOutcome::Recommended(recommendation) => {
            let reasons = recommendation
                .reasons()
                .iter()
                .copied()
                .map(map_reason)
                .take(MAX_AUTOMATIC_REASONS)
                .collect();
            AutomaticRoutingDecisionOutcome::Selected(AutomaticRoutingDecision {
                runtime_generation: recommendation.runtime_generation(),
                candidate: recommendation.candidate().clone(),
                reasons,
            })
        }
        super::RoutingRecommendationOutcome::Unavailable(reason) => {
            AutomaticRoutingDecisionOutcome::Unavailable(map_unavailable_reason(*reason))
        }
    };
    AutomaticRoutingSnapshot {
        runtime_generation: evaluation.runtime_generation(),
        outcome,
    }
}

fn map_reason(reason: RoutingRecommendationReason) -> AutomaticRoutingReason {
    match reason {
        RoutingRecommendationReason::Configured => AutomaticRoutingReason::Configured,
        RoutingRecommendationReason::Eligible => AutomaticRoutingReason::Eligible,
        RoutingRecommendationReason::RoleCompatible => AutomaticRoutingReason::RoleCompatible,
        RoutingRecommendationReason::AccountReady => AutomaticRoutingReason::AccountReady,
        RoutingRecommendationReason::ConnectionReady => AutomaticRoutingReason::ConnectionReady,
        RoutingRecommendationReason::CapabilityValidated => {
            AutomaticRoutingReason::CapabilityValidated
        }
        RoutingRecommendationReason::PoolHasEligibleTargets => {
            AutomaticRoutingReason::PoolHasEligibleTargets
        }
        RoutingRecommendationReason::HigherConfiguredOrder => {
            AutomaticRoutingReason::HigherConfiguredOrder
        }
        RoutingRecommendationReason::AlternativeCoolingDown => {
            AutomaticRoutingReason::AlternativeCoolingDown
        }
    }
}

fn map_unavailable_reason(
    reason: RoutingRecommendationUnavailableReason,
) -> AutomaticRoutingUnavailableReason {
    match reason {
        RoutingRecommendationUnavailableReason::NoConfiguredCandidates => {
            AutomaticRoutingUnavailableReason::NoConfiguredCandidates
        }
        RoutingRecommendationUnavailableReason::NoEligibleCandidates => {
            AutomaticRoutingUnavailableReason::NoEligibleCandidates
        }
        RoutingRecommendationUnavailableReason::AllCandidatesCoolingDown => {
            AutomaticRoutingUnavailableReason::AllCandidatesCoolingDown
        }
        RoutingRecommendationUnavailableReason::CandidateSetAmbiguous => {
            AutomaticRoutingUnavailableReason::CandidateSetAmbiguous
        }
    }
}

#[cfg(test)]
#[path = "automatic_routing_tests.rs"]
mod tests;
