use super::ExecutionModeSelection;
use super::ExecutionPolicyError;
use super::ResolvedExecutionPolicy;
use codex_orchestration::OrchestrationMode;
use std::fmt;

/// Explains why a strategy cannot currently select a production workflow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrchestrationStrategyUnavailableReason {
    /// No deterministic automatic workflow selector is connected.
    AutomaticSelectorUnavailable,
    /// No trusted recommendation and confirmation authority is connected.
    RecommendationAuthorityUnavailable,
    /// No trusted adaptive routing authority is connected.
    AdaptiveUsageAuthorityUnavailable,
}

impl fmt::Display for OrchestrationStrategyUnavailableReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::AutomaticSelectorUnavailable => "automatic orchestration selector is unavailable",
            Self::RecommendationAuthorityUnavailable => "recommendation authority is unavailable",
            Self::AdaptiveUsageAuthorityUnavailable => "adaptive routing authority is unavailable",
        };
        formatter.write_str(message)
    }
}

/// Availability of the selected orchestration strategy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrchestrationStrategyAvailability {
    /// The strategy can select the existing trusted production workflow.
    Available,
    /// The strategy is known but its required authority is not connected.
    Unavailable(OrchestrationStrategyUnavailableReason),
}

/// One immutable strategy and execution-preset resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedOrchestrationPolicy {
    strategy: OrchestrationMode,
    execution: ResolvedExecutionPolicy,
    availability: OrchestrationStrategyAvailability,
}

impl ResolvedOrchestrationPolicy {
    /// Resolves the canonical preset and records strategy availability without aliasing.
    pub fn resolve(
        strategy: OrchestrationMode,
        preset: ExecutionModeSelection,
    ) -> Result<Self, ExecutionPolicyError> {
        let execution = preset.resolve()?;
        let availability = match strategy {
            OrchestrationMode::Single | OrchestrationMode::Manual => {
                OrchestrationStrategyAvailability::Available
            }
            OrchestrationMode::Recommended => OrchestrationStrategyAvailability::Available,
            OrchestrationMode::Automatic => OrchestrationStrategyAvailability::Available,
            OrchestrationMode::Adaptive => OrchestrationStrategyAvailability::Available,
        };
        Ok(Self {
            strategy,
            execution,
            availability,
        })
    }

    pub fn strategy(&self) -> OrchestrationMode {
        self.strategy
    }

    pub fn execution(&self) -> &ResolvedExecutionPolicy {
        &self.execution
    }

    pub fn availability(&self) -> OrchestrationStrategyAvailability {
        self.availability
    }

    pub fn requires_syndrid_runtime(&self) -> bool {
        matches!(
            self.availability,
            OrchestrationStrategyAvailability::Available
        ) && !matches!(self.strategy, OrchestrationMode::Single)
    }
}

#[cfg(test)]
#[path = "orchestration_policy_tests.rs"]
mod tests;
