//! Pure, bounded inputs and results for higher-level routing strategies.
//!
//! This module does not select, install, construct, or invoke a provider. Trusted callers adapt
//! existing routing, readiness, capability, and cooldown authorities into these immutable
//! snapshots. Later strategy milestones can evaluate the result without duplicating production
//! admission or pool-rotation logic.

use super::account_pools::AccountPoolProviderFamily;
use super::account_pools::AccountPoolTarget;
use super::account_pools::PoolId;
use super::cooldown_state::ProviderCooldownStatus;
use super::provider_failure::ProviderFailureClass;
use super::routing_profiles::RoutingProfileId;
use super::routing_profiles::RoutingRole;
use std::fmt;
use std::time::Duration;

const MAX_CANDIDATES: usize = 32;
const MAX_EVIDENCE_ITEMS: usize = 16;
const MAX_SAFE_TEXT_BYTES: usize = 256;

/// A safe target identity for one already-configured routing alternative.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RoutingStrategyCandidateTarget {
    kind: RoutingStrategyCandidateTargetKind,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum RoutingStrategyCandidateTargetKind {
    Direct {
        target: AccountPoolTarget,
        provider_id: String,
        model_id: String,
    },
    Pool {
        pool_id: PoolId,
        provider_id: String,
        model_id: String,
    },
}

impl RoutingStrategyCandidateTarget {
    pub fn direct(
        target: AccountPoolTarget,
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Result<Self, RoutingStrategyCandidateError> {
        let provider_id = safe_text(provider_id.into())?;
        let model_id = safe_text(model_id.into())?;
        Ok(Self {
            kind: RoutingStrategyCandidateTargetKind::Direct {
                target,
                provider_id,
                model_id,
            },
        })
    }

    pub fn pool(
        pool_id: PoolId,
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Result<Self, RoutingStrategyCandidateError> {
        Ok(Self {
            kind: RoutingStrategyCandidateTargetKind::Pool {
                pool_id,
                provider_id: safe_text(provider_id.into())?,
                model_id: safe_text(model_id.into())?,
            },
        })
    }

    pub fn provider_id(&self) -> &str {
        match &self.kind {
            RoutingStrategyCandidateTargetKind::Direct { provider_id, .. }
            | RoutingStrategyCandidateTargetKind::Pool { provider_id, .. } => provider_id,
        }
    }

    pub fn model_id(&self) -> &str {
        match &self.kind {
            RoutingStrategyCandidateTargetKind::Direct { model_id, .. }
            | RoutingStrategyCandidateTargetKind::Pool { model_id, .. } => model_id,
        }
    }

    pub fn pool_id(&self) -> Option<&PoolId> {
        match &self.kind {
            RoutingStrategyCandidateTargetKind::Direct { .. } => None,
            RoutingStrategyCandidateTargetKind::Pool { pool_id, .. } => Some(pool_id),
        }
    }

    pub fn direct_target(&self) -> Option<&AccountPoolTarget> {
        match &self.kind {
            RoutingStrategyCandidateTargetKind::Direct { target, .. } => Some(target),
            RoutingStrategyCandidateTargetKind::Pool { .. } => None,
        }
    }
}

/// Stable identity for a configured candidate. It contains no labels, credentials, or clients.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RoutingStrategyCandidateId {
    profile_id: RoutingProfileId,
    role: RoutingRole,
    target: RoutingStrategyCandidateTarget,
}

impl RoutingStrategyCandidateId {
    pub fn new(
        profile_id: RoutingProfileId,
        role: RoutingRole,
        target: RoutingStrategyCandidateTarget,
    ) -> Self {
        Self {
            profile_id,
            role,
            target,
        }
    }

    pub fn profile_id(&self) -> &RoutingProfileId {
        &self.profile_id
    }

    pub fn role(&self) -> RoutingRole {
        self.role
    }

    pub fn target(&self) -> &RoutingStrategyCandidateTarget {
        &self.target
    }
}

/// One already-configured routing alternative available to a future strategy.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RoutingStrategyCandidate {
    id: RoutingStrategyCandidateId,
}

impl RoutingStrategyCandidate {
    pub fn new(id: RoutingStrategyCandidateId) -> Self {
        Self { id }
    }

    pub fn id(&self) -> &RoutingStrategyCandidateId {
        &self.id
    }
}

/// Informational facts that are safe to retain but do not make a candidate eligible or preferred.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingStrategyInformationalEvidence {
    Configured,
    ProviderFamily(AccountPoolProviderFamily),
    ExactTarget(AccountPoolTarget),
    Pool(PoolId),
}

/// Facts adapted from canonical authorities that may explain eligibility.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingStrategyEligibilityEvidence {
    RoleCompatible,
    AccountReady,
    ConnectionReady,
    CooldownAvailable,
    CoolingDown {
        remaining: Duration,
        failure_class: ProviderFailureClass,
    },
    CapabilityValidated,
    PoolHasEligibleTargets,
    AllPoolTargetsCooling {
        earliest_recovery: Option<Duration>,
    },
}

/// The only ordering evidence accepted by this milestone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingStrategyConfiguredOrder {
    position: usize,
}

impl RoutingStrategyConfiguredOrder {
    pub fn position(&self) -> usize {
        self.position
    }
}

/// Bounded evidence retained with a candidate snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingStrategyEvidence {
    Informational(RoutingStrategyInformationalEvidence),
    Eligibility(RoutingStrategyEligibilityEvidence),
    Ordering(RoutingStrategyConfiguredOrder),
}

/// Canonical eligibility adapted by a trusted caller without reimplementing its rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingStrategyEligibility {
    Eligible,
    Ineligible(RoutingStrategyIneligibility),
}

impl RoutingStrategyEligibility {
    pub fn from_cooldown_status(status: &ProviderCooldownStatus) -> Self {
        match status {
            ProviderCooldownStatus::Available => Self::Eligible,
            ProviderCooldownStatus::CoolingDown {
                remaining,
                failure_class,
            } => Self::Ineligible(RoutingStrategyIneligibility::CoolingDown {
                remaining: *remaining,
                failure_class: *failure_class,
            }),
        }
    }

    pub fn is_eligible(&self) -> bool {
        matches!(self, Self::Eligible)
    }

    fn is_cooling_down(&self) -> bool {
        matches!(
            self,
            Self::Ineligible(RoutingStrategyIneligibility::CoolingDown { .. })
                | Self::Ineligible(RoutingStrategyIneligibility::AllPoolTargetsCooling { .. })
        )
    }
}

/// Bounded reasons supplied by existing readiness, capability, and cooldown authorities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingStrategyIneligibility {
    NotConfigured,
    StructurallyInvalid,
    RoleIncompatible,
    AccountUnavailable,
    ConnectionUnavailable,
    CapabilityUnavailable,
    CoolingDown {
        remaining: Duration,
        failure_class: ProviderFailureClass,
    },
    AllPoolTargetsCooling {
        earliest_recovery: Option<Duration>,
    },
    NoEligiblePoolMembers,
}

/// An immutable candidate plus safe evidence from one coherent authority snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingStrategyCandidateSnapshot {
    candidate: RoutingStrategyCandidate,
    evidence: Vec<RoutingStrategyEvidence>,
    eligibility: RoutingStrategyEligibility,
}

impl RoutingStrategyCandidateSnapshot {
    pub fn new(
        candidate: RoutingStrategyCandidate,
        evidence: Vec<RoutingStrategyEvidence>,
        eligibility: RoutingStrategyEligibility,
    ) -> Result<Self, RoutingStrategyCandidateError> {
        if evidence.len() > MAX_EVIDENCE_ITEMS {
            return Err(RoutingStrategyCandidateError::TooMuchEvidence);
        }
        Ok(Self {
            candidate,
            evidence,
            eligibility,
        })
    }

    pub fn candidate(&self) -> &RoutingStrategyCandidate {
        &self.candidate
    }

    pub fn evidence(&self) -> &[RoutingStrategyEvidence] {
        &self.evidence
    }

    pub fn eligibility(&self) -> &RoutingStrategyEligibility {
        &self.eligibility
    }

    fn with_configured_order(
        mut self,
        position: usize,
    ) -> Result<Self, RoutingStrategyCandidateError> {
        if self.evidence.len() >= MAX_EVIDENCE_ITEMS {
            return Err(RoutingStrategyCandidateError::TooMuchEvidence);
        }
        self.evidence.push(RoutingStrategyEvidence::Ordering(
            RoutingStrategyConfiguredOrder { position },
        ));
        Ok(self)
    }
}

/// Errors while constructing a bounded candidate snapshot or ordered input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingStrategyCandidateError {
    InvalidSafeText,
    TooManyCandidates,
    TooMuchEvidence,
    DuplicateCandidate,
}

impl fmt::Display for RoutingStrategyCandidateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSafeText => "strategy candidate contains invalid bounded identity text",
            Self::TooManyCandidates => "strategy candidate set exceeds its bounded size",
            Self::TooMuchEvidence => "strategy candidate evidence exceeds its bounded size",
            Self::DuplicateCandidate => "strategy candidate set contains a duplicate identity",
        })
    }
}

impl std::error::Error for RoutingStrategyCandidateError {}

/// Immutable input for one pure evaluation pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingStrategyEvaluationInput {
    runtime_generation: u64,
    ordering: RoutingStrategyCandidateOrdering,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RoutingStrategyCandidateOrdering {
    Configured(Vec<RoutingStrategyCandidateSnapshot>),
    Ambiguous,
}

impl RoutingStrategyEvaluationInput {
    /// Captures candidates in the exact order supplied by the trusted configuration authority.
    pub fn configured(
        runtime_generation: u64,
        candidates: Vec<RoutingStrategyCandidateSnapshot>,
    ) -> Result<Self, RoutingStrategyCandidateError> {
        if candidates.len() > MAX_CANDIDATES {
            return Err(RoutingStrategyCandidateError::TooManyCandidates);
        }
        let mut ordered = Vec::with_capacity(candidates.len());
        for (position, candidate) in candidates.into_iter().enumerate() {
            if ordered
                .iter()
                .any(|existing: &RoutingStrategyCandidateSnapshot| {
                    existing.candidate().id() == candidate.candidate().id()
                })
            {
                return Err(RoutingStrategyCandidateError::DuplicateCandidate);
            }
            ordered.push(candidate.with_configured_order(position)?);
        }
        Ok(Self {
            runtime_generation,
            ordering: RoutingStrategyCandidateOrdering::Configured(ordered),
        })
    }

    /// Constructs an input for which the trusted authority could not establish an order.
    pub fn ambiguous(runtime_generation: u64) -> Self {
        Self {
            runtime_generation,
            ordering: RoutingStrategyCandidateOrdering::Ambiguous,
        }
    }

    pub fn runtime_generation(&self) -> u64 {
        self.runtime_generation
    }
}

/// A bounded explanation for why no candidate can currently be considered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingStrategyNoSelectionReason {
    NoConfiguredCandidates,
    NoEligibleCandidates,
    AllCandidatesCoolingDown,
    CandidateSetAmbiguous,
}

/// Result of evaluating candidates without selecting or mutating production routing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingStrategyEvaluation {
    runtime_generation: u64,
    candidates: Vec<RoutingStrategyCandidateSnapshot>,
    outcome: RoutingStrategyEvaluationOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingStrategyEvaluationOutcome {
    CandidatesAvailable { eligible_count: usize },
    NoSelection(RoutingStrategyNoSelectionReason),
}

impl RoutingStrategyEvaluation {
    pub fn runtime_generation(&self) -> u64 {
        self.runtime_generation
    }

    pub fn candidates(&self) -> &[RoutingStrategyCandidateSnapshot] {
        &self.candidates
    }

    pub fn eligible_candidates(&self) -> impl Iterator<Item = &RoutingStrategyCandidateSnapshot> {
        self.candidates
            .iter()
            .filter(|candidate| candidate.eligibility().is_eligible())
    }

    pub fn outcome(&self) -> &RoutingStrategyEvaluationOutcome {
        &self.outcome
    }
}

/// Rejects a pure evaluation whose input belongs to another installed runtime generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoutingStrategyGenerationMismatch {
    pub expected: u64,
    pub actual: u64,
}

impl fmt::Display for RoutingStrategyGenerationMismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "strategy input generation {} does not match expected generation {}",
            self.actual, self.expected
        )
    }
}

impl std::error::Error for RoutingStrategyGenerationMismatch {}

/// Evaluates an immutable candidate input while preserving configured order.
pub fn evaluate_routing_strategy_candidates(
    input: RoutingStrategyEvaluationInput,
    expected_generation: u64,
) -> Result<RoutingStrategyEvaluation, RoutingStrategyGenerationMismatch> {
    if input.runtime_generation != expected_generation {
        return Err(RoutingStrategyGenerationMismatch {
            expected: expected_generation,
            actual: input.runtime_generation,
        });
    }

    let (candidates, outcome) = match input.ordering {
        RoutingStrategyCandidateOrdering::Ambiguous => (
            Vec::new(),
            RoutingStrategyEvaluationOutcome::NoSelection(
                RoutingStrategyNoSelectionReason::CandidateSetAmbiguous,
            ),
        ),
        RoutingStrategyCandidateOrdering::Configured(candidates) => {
            let eligible_count = candidates
                .iter()
                .filter(|candidate| candidate.eligibility().is_eligible())
                .count();
            let outcome = if candidates.is_empty() {
                RoutingStrategyEvaluationOutcome::NoSelection(
                    RoutingStrategyNoSelectionReason::NoConfiguredCandidates,
                )
            } else if eligible_count > 0 {
                RoutingStrategyEvaluationOutcome::CandidatesAvailable { eligible_count }
            } else if candidates
                .iter()
                .all(|candidate| candidate.eligibility().is_cooling_down())
            {
                RoutingStrategyEvaluationOutcome::NoSelection(
                    RoutingStrategyNoSelectionReason::AllCandidatesCoolingDown,
                )
            } else {
                RoutingStrategyEvaluationOutcome::NoSelection(
                    RoutingStrategyNoSelectionReason::NoEligibleCandidates,
                )
            };
            (candidates, outcome)
        }
    };

    Ok(RoutingStrategyEvaluation {
        runtime_generation: expected_generation,
        candidates,
        outcome,
    })
}

fn safe_text(value: String) -> Result<String, RoutingStrategyCandidateError> {
    if value.trim().is_empty()
        || value.len() > MAX_SAFE_TEXT_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(RoutingStrategyCandidateError::InvalidSafeText);
    }
    Ok(value)
}

#[cfg(test)]
#[path = "strategy_candidates_tests.rs"]
mod tests;
