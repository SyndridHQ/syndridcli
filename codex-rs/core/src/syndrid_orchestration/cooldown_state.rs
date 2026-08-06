//! Session-owned, non-persisted cooldown state for exact provider targets.
//!
//! This state is deliberately independent from RoundRobin cursor state. It records evidence for
//! future eligibility policy but does not influence member selection in this milestone.

use super::account_pools::AccountPoolTarget;
use super::provider_failure::MAX_PROVIDER_COOLDOWN;
use super::provider_failure::ProviderFailureClass;
use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;
use std::time::Instant;

/// Exact provider target identity shared by pools and routing roles.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderCooldownKey {
    target: AccountPoolTarget,
}

impl ProviderCooldownKey {
    pub fn new(target: AccountPoolTarget) -> Self {
        Self { target }
    }

    pub fn provider_family(&self) -> super::account_pools::AccountPoolProviderFamily {
        self.target.provider_family()
    }

    pub fn target(&self) -> &AccountPoolTarget {
        &self.target
    }
}

/// Bounded status of an exact target's cooldown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderCooldownStatus {
    Available,
    CoolingDown {
        remaining: Duration,
        failure_class: ProviderFailureClass,
    },
}

/// Errors from explicit cooldown updates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCooldownError {
    ZeroDuration,
    DurationExceedsMaximum,
}

impl fmt::Display for ProviderCooldownError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroDuration => "provider cooldown duration must be non-zero",
            Self::DurationExceedsMaximum => "provider cooldown duration exceeds the maximum",
        })
    }
}

impl std::error::Error for ProviderCooldownError {}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProviderCooldownRecord {
    expires_at: Instant,
    failure_class: ProviderFailureClass,
}

/// In-memory cooldown records owned by one session.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProviderCooldownState {
    records: BTreeMap<ProviderCooldownKey, ProviderCooldownRecord>,
}

impl ProviderCooldownState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records or extends a target cooldown without ever shortening an active record.
    pub fn record_cooldown(
        &mut self,
        key: ProviderCooldownKey,
        failure_class: ProviderFailureClass,
        duration: Duration,
        now: Instant,
    ) -> Result<(), ProviderCooldownError> {
        if duration.is_zero() {
            return Err(ProviderCooldownError::ZeroDuration);
        }
        if duration > MAX_PROVIDER_COOLDOWN {
            return Err(ProviderCooldownError::DurationExceedsMaximum);
        }
        let expires_at = now + duration;
        match self.records.get_mut(&key) {
            Some(existing) if existing.expires_at >= expires_at => {}
            Some(existing) => {
                existing.expires_at = expires_at;
                existing.failure_class = failure_class;
            }
            None => {
                self.records.insert(
                    key,
                    ProviderCooldownRecord {
                        expires_at,
                        failure_class,
                    },
                );
            }
        }
        Ok(())
    }

    /// Returns status at an explicit monotonic instant and lazily removes an expired record.
    pub fn status(&mut self, key: &ProviderCooldownKey, now: Instant) -> ProviderCooldownStatus {
        let Some(record) = self.records.get(key) else {
            return ProviderCooldownStatus::Available;
        };
        if record.expires_at <= now {
            self.records.remove(key);
            return ProviderCooldownStatus::Available;
        }
        ProviderCooldownStatus::CoolingDown {
            remaining: record.expires_at - now,
            failure_class: record.failure_class,
        }
    }

    /// Removes every expired record and returns the number removed.
    pub fn prune_expired(&mut self, now: Instant) -> usize {
        let before = self.records.len();
        self.records.retain(|_, record| record.expires_at > now);
        before - self.records.len()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
#[path = "cooldown_state_tests.rs"]
mod tests;
