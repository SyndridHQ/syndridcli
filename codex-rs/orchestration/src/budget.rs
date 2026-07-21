use std::fmt;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::DataQuality;

const MIN_MULTIPLIER_BASIS_POINTS: u16 = 10_000;
const MAX_MULTIPLIER_BASIS_POINTS: u16 = 20_000;

/// A non-negative usage quantity; billing and provider conversion remain outside this crate.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct UsageQuantity(u64);

impl UsageQuantity {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Failure returned for a multiplier outside the initial permitted range.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MultiplierError {
    #[error("usage multiplier must be at least 1.00x")]
    BelowMinimum,
    #[error("usage multiplier must be at most 2.00x")]
    AboveMaximum,
}

/// Fixed-point workflow multiplier stored in basis points, never floating point.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "UsageBudgetMultiplierWire")]
pub struct UsageBudgetMultiplier {
    basis_points: u16,
}

#[derive(Deserialize)]
struct UsageBudgetMultiplierWire {
    basis_points: u16,
}

impl TryFrom<UsageBudgetMultiplierWire> for UsageBudgetMultiplier {
    type Error = MultiplierError;

    fn try_from(value: UsageBudgetMultiplierWire) -> Result<Self, Self::Error> {
        Self::new_basis_points(value.basis_points)
    }
}

impl UsageBudgetMultiplier {
    pub const SINGLE: Self = Self {
        basis_points: 10_000,
    };
    pub const LIGHT_ACCELERATION: Self = Self {
        basis_points: 11_000,
    };
    pub const BALANCED: Self = Self {
        basis_points: 12_500,
    };
    pub const AGGRESSIVE: Self = Self {
        basis_points: 15_000,
    };
    pub const MAXIMUM: Self = Self {
        basis_points: 20_000,
    };

    pub const fn new_basis_points(value: u16) -> Result<Self, MultiplierError> {
        if value < MIN_MULTIPLIER_BASIS_POINTS {
            return Err(MultiplierError::BelowMinimum);
        }
        if value > MAX_MULTIPLIER_BASIS_POINTS {
            return Err(MultiplierError::AboveMaximum);
        }
        Ok(Self {
            basis_points: value,
        })
    }

    pub const fn basis_points(self) -> u16 {
        self.basis_points
    }
}

impl fmt::Display for UsageBudgetMultiplier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}.{:02}×",
            self.basis_points / 10_000,
            (self.basis_points % 10_000) / 100
        )
    }
}

/// Qualitative posture for a future adaptive policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EfficiencyPosture {
    Conservative,
    Balanced,
    Aggressive,
}

/// Data-only budget snapshot; it performs no token or quota calculations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowBudget {
    pub baseline_estimate: Option<UsageQuantity>,
    pub selected_multiplier: UsageBudgetMultiplier,
    pub maximum_permitted_usage: Option<UsageQuantity>,
    pub actual_attributed_usage: Option<UsageQuantity>,
    pub remaining_workflow_usage: Option<UsageQuantity>,
    pub projected_final_usage: Option<UsageQuantity>,
    pub optional_work_allowance: Option<UsageQuantity>,
    pub protected_reserve: Option<UsageQuantity>,
    pub data_quality: DataQuality,
}

/// Data-only inputs and ceilings for future adaptive allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveBudgetPolicy {
    pub protected_reserve: Option<UsageQuantity>,
    pub remaining_allowance_quality: DataQuality,
    pub reset_time_quality: DataQuality,
    pub minimum_multiplier: UsageBudgetMultiplier,
    pub maximum_multiplier: UsageBudgetMultiplier,
    pub posture: EfficiencyPosture,
    pub retry_ceiling: u16,
    pub repair_ceiling: u16,
    pub concurrency_ceiling: u16,
    pub writer_ceiling: u16,
}
