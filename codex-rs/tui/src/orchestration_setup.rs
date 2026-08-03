//! Typed, side-effect-free readiness for the local Syndrid setup surface.

use crate::legacy_core::OrchestrationMode;
use crate::legacy_core::OrchestrationStrategyAvailability;
use crate::legacy_core::OrchestrationStrategyUnavailableReason;
use crate::legacy_core::ResolvedOrchestrationPolicy;
use crate::orchestration_profile::OrchestrationProfileSelection;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SetupReadinessState {
    Ready,
    NotRequired,
    Unavailable(String),
    Invalid(String),
    MissingAuthority(String),
}

impl SetupReadinessState {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::NotRequired => "Not required",
            Self::Unavailable(_) => "Unavailable",
            Self::Invalid(_) | Self::MissingAuthority(_) => "Needs attention",
        }
    }

    pub(crate) fn reason(&self) -> Option<&str> {
        match self {
            Self::Ready | Self::NotRequired => None,
            Self::Unavailable(reason) | Self::Invalid(reason) | Self::MissingAuthority(reason) => {
                Some(reason)
            }
        }
    }

    fn is_ready(&self) -> bool {
        matches!(self, Self::Ready | Self::NotRequired)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OrchestrationSetupReadiness {
    pub(crate) strategy: SetupReadinessState,
    pub(crate) preset: SetupReadinessState,
    pub(crate) routing: SetupReadinessState,
    pub(crate) required_roles: SetupReadinessState,
    pub(crate) runtime_assembly: SetupReadinessState,
}

pub(crate) const FIRST_RUN_SETUP_INVITATION: &str =
    "Syndrid setup is ready. Run /setup to choose an orchestration strategy and preset.";

pub(crate) fn should_show_first_run_invitation(
    profile_exists: bool,
    profile_warning: bool,
    local_interactive_session: bool,
) -> bool {
    local_interactive_session && !profile_exists && !profile_warning
}

impl OrchestrationSetupReadiness {
    pub(crate) fn for_selection(
        selection: &OrchestrationProfileSelection,
        manual_runtime_ready: bool,
    ) -> Self {
        let resolved =
            ResolvedOrchestrationPolicy::resolve(selection.strategy, selection.preset.clone());
        let strategy = match resolved
            .as_ref()
            .map(ResolvedOrchestrationPolicy::availability)
        {
            Ok(OrchestrationStrategyAvailability::Available) => SetupReadinessState::Ready,
            Ok(OrchestrationStrategyAvailability::Unavailable(reason)) => {
                SetupReadinessState::Unavailable(unavailable_reason(reason))
            }
            Err(_) => SetupReadinessState::Invalid("strategy could not be resolved".to_string()),
        };
        let preset = match resolved {
            Ok(_) => SetupReadinessState::Ready,
            Err(_) => SetupReadinessState::Invalid("preset configuration is invalid".to_string()),
        };
        if selection.strategy == OrchestrationMode::Single {
            return Self {
                strategy,
                preset,
                routing: SetupReadinessState::NotRequired,
                required_roles: SetupReadinessState::NotRequired,
                runtime_assembly: SetupReadinessState::NotRequired,
            };
        }
        let manual_state = if manual_runtime_ready {
            SetupReadinessState::Ready
        } else {
            SetupReadinessState::MissingAuthority(
                "trusted routing, provider, and role configuration is not ready".to_string(),
            )
        };
        Self {
            strategy,
            preset,
            routing: manual_state.clone(),
            required_roles: manual_state.clone(),
            runtime_assembly: manual_state,
        }
    }

    pub(crate) fn can_apply(&self) -> bool {
        self.strategy.is_ready()
            && self.preset.is_ready()
            && self.routing.is_ready()
            && self.required_roles.is_ready()
            && self.runtime_assembly.is_ready()
    }
}

fn unavailable_reason(reason: OrchestrationStrategyUnavailableReason) -> String {
    match reason {
        OrchestrationStrategyUnavailableReason::AutomaticSelectorUnavailable => {
            "automatic workflow selection is not implemented yet".to_string()
        }
        OrchestrationStrategyUnavailableReason::RecommendationAuthorityUnavailable => {
            "recommendation authority is not implemented yet".to_string()
        }
        OrchestrationStrategyUnavailableReason::AdaptiveUsageAuthorityUnavailable => {
            "account, quota, and usage authorities are not implemented yet".to_string()
        }
    }
}

#[cfg(test)]
#[path = "orchestration_setup_tests.rs"]
mod tests;
