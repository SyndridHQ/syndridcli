//! Read-only cooldown presentation for the Syndrid TUI.
//!
//! The snapshot is copied from the session-owned cooldown authority while holding its lock. The
//! UI only receives bounded target identity, status, remaining duration, and canonical failure
//! class; it never retains or mutates the session state.

use crate::legacy_core::AccountPoolTarget;
use crate::legacy_core::ProviderCooldownState;
use crate::legacy_core::ProviderCooldownStatus;
use crate::legacy_core::ProviderFailureClass;
use crate::legacy_core::SessionExecutionPolicyState;
use std::collections::BTreeMap;
use std::time::Duration;
use std::time::Instant;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TuiCooldownStatus {
    Available,
    CoolingDown {
        remaining: Duration,
        failure_class: ProviderFailureClass,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TuiProviderCooldownSnapshot {
    statuses: BTreeMap<AccountPoolTarget, TuiCooldownStatus>,
}

impl TuiProviderCooldownSnapshot {
    pub(crate) fn from_policy_state(policy_state: &SessionExecutionPolicyState) -> Self {
        let now = Instant::now();
        let cooldown_state = policy_state.cooldown_state();
        let Ok(state) = cooldown_state.lock() else {
            return Self::default();
        };
        Self::from_state(&state, now)
    }

    pub(crate) fn from_state(state: &ProviderCooldownState, now: Instant) -> Self {
        let statuses = state
            .snapshot(now)
            .into_iter()
            .map(|(key, status)| {
                let status = match status {
                    ProviderCooldownStatus::Available => TuiCooldownStatus::Available,
                    ProviderCooldownStatus::CoolingDown {
                        remaining,
                        failure_class,
                    } => TuiCooldownStatus::CoolingDown {
                        remaining,
                        failure_class,
                    },
                };
                (key.target().clone(), status)
            })
            .collect();
        Self { statuses }
    }

    pub(crate) fn status_for_target(&self, target: &AccountPoolTarget) -> TuiCooldownStatus {
        self.statuses
            .get(target)
            .cloned()
            .unwrap_or(TuiCooldownStatus::Available)
    }

    pub(crate) fn cooling_status_for_target(
        &self,
        target: &AccountPoolTarget,
    ) -> Option<(Duration, ProviderFailureClass)> {
        match self.status_for_target(target) {
            TuiCooldownStatus::Available => None,
            TuiCooldownStatus::CoolingDown {
                remaining,
                failure_class,
            } => Some((remaining, failure_class)),
        }
    }

    pub(crate) fn earliest_recovery_for_targets<'a>(
        &self,
        targets: impl IntoIterator<Item = &'a AccountPoolTarget>,
    ) -> Option<Duration> {
        targets
            .into_iter()
            .filter_map(|target| {
                self.cooling_status_for_target(target)
                    .map(|(remaining, _)| remaining)
            })
            .min()
    }

    pub(crate) fn available_target_count<'a>(
        &self,
        targets: impl IntoIterator<Item = &'a AccountPoolTarget>,
    ) -> usize {
        targets
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|target| matches!(self.status_for_target(target), TuiCooldownStatus::Available))
            .count()
    }

    pub(crate) fn cooling_target_count<'a>(
        &self,
        targets: impl IntoIterator<Item = &'a AccountPoolTarget>,
    ) -> usize {
        targets
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|target| self.cooling_status_for_target(target).is_some())
            .count()
    }
}

pub(crate) fn format_cooldown_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        return format!("{seconds}s");
    }
    if seconds < 60 * 60 {
        let minutes = seconds / 60;
        let seconds = seconds % 60;
        return if seconds == 0 {
            format!("{minutes}m")
        } else {
            format!("{minutes}m {seconds:02}s")
        };
    }
    let hours = seconds / (60 * 60);
    let minutes = (seconds / 60) % 60;
    if minutes == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h {minutes:02}m")
    }
}

pub(crate) fn failure_class_label(class: ProviderFailureClass) -> &'static str {
    match class {
        ProviderFailureClass::RateLimited => "Rate limited",
        ProviderFailureClass::QuotaExhausted => "Usage limit reached",
        ProviderFailureClass::Authentication => "Authentication required",
        ProviderFailureClass::Authorization => "Not authorized",
        ProviderFailureClass::InvalidRequest => "Invalid request",
        ProviderFailureClass::ModelUnavailable => "Model unavailable",
        ProviderFailureClass::ContextLengthExceeded => "Context limit exceeded",
        ProviderFailureClass::ProviderUnavailable => "Provider temporarily unavailable",
        ProviderFailureClass::Network => "Network/provider connection issue",
        ProviderFailureClass::Timeout => "Provider timeout",
        ProviderFailureClass::Cancelled => "Cancelled",
        ProviderFailureClass::Internal | ProviderFailureClass::Unknown => "Temporarily unavailable",
    }
}

pub(crate) fn cooldown_label(status: &TuiCooldownStatus) -> String {
    match status {
        TuiCooldownStatus::Available => "Available".to_string(),
        TuiCooldownStatus::CoolingDown {
            remaining,
            failure_class,
        } => format!(
            "Cooling down · {} · {}",
            format_cooldown_duration(*remaining),
            failure_class_label(*failure_class)
        ),
    }
}

#[cfg(test)]
#[path = "cooldown_status_tests.rs"]
mod tests;
