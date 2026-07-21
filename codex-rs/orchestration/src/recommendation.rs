use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::AgentProfile;
use crate::BoundedText;
use crate::DataQuality;
use crate::EffortRoute;
use crate::ModelRoute;
use crate::OrchestrationMode;
use crate::UsageBudgetMultiplier;
use crate::UsageQuantity;

/// Confidence in a forecast; confidence does not make an estimate exact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForecastConfidence {
    Low,
    Medium,
    High,
}

/// Bounded forecast values recorded by a future recommendation source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forecast {
    pub predicted_usage: Option<UsageQuantity>,
    pub predicted_orchestration_overhead: Option<UsageQuantity>,
    pub predicted_completion_time_ms: Option<u64>,
    pub predicted_latency_ms: Option<u64>,
    pub confidence: ForecastConfidence,
    pub data_quality: DataQuality,
}

/// Maximum number of notes a future producer should retain.
pub const MAX_RECOMMENDATION_NOTES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RecommendationError {
    #[error("recommendation notes exceed the 16-item limit")]
    TooManyNotes,
}

/// A proposed workflow plan; recording one does not launch it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RecommendationWire")]
pub struct Recommendation {
    mode: OrchestrationMode,
    proposed_profiles: Vec<AgentProfile>,
    resolved_model_routes: Vec<ModelRoute>,
    resolved_effort_routes: Vec<EffortRoute>,
    proposed_concurrency: u16,
    proposed_multiplier: UsageBudgetMultiplier,
    forecast: Forecast,
    reasons: Vec<BoundedText>,
    uncertainties: Vec<BoundedText>,
}

#[derive(Clone, Debug, Deserialize)]
struct RecommendationWire {
    mode: OrchestrationMode,
    proposed_profiles: Vec<AgentProfile>,
    resolved_model_routes: Vec<ModelRoute>,
    resolved_effort_routes: Vec<EffortRoute>,
    proposed_concurrency: u16,
    proposed_multiplier: UsageBudgetMultiplier,
    forecast: Forecast,
    reasons: Vec<BoundedText>,
    uncertainties: Vec<BoundedText>,
}

impl TryFrom<RecommendationWire> for Recommendation {
    type Error = RecommendationError;

    fn try_from(value: RecommendationWire) -> Result<Self, Self::Error> {
        if value.reasons.len() > MAX_RECOMMENDATION_NOTES
            || value.uncertainties.len() > MAX_RECOMMENDATION_NOTES
        {
            return Err(RecommendationError::TooManyNotes);
        }
        Ok(Self {
            mode: value.mode,
            proposed_profiles: value.proposed_profiles,
            resolved_model_routes: value.resolved_model_routes,
            resolved_effort_routes: value.resolved_effort_routes,
            proposed_concurrency: value.proposed_concurrency,
            proposed_multiplier: value.proposed_multiplier,
            forecast: value.forecast,
            reasons: value.reasons,
            uncertainties: value.uncertainties,
        })
    }
}

impl Recommendation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mode: OrchestrationMode,
        proposed_profiles: Vec<AgentProfile>,
        resolved_model_routes: Vec<ModelRoute>,
        resolved_effort_routes: Vec<EffortRoute>,
        proposed_concurrency: u16,
        proposed_multiplier: UsageBudgetMultiplier,
        forecast: Forecast,
        reasons: Vec<BoundedText>,
        uncertainties: Vec<BoundedText>,
    ) -> Result<Self, RecommendationError> {
        Self::try_from(RecommendationWire {
            mode,
            proposed_profiles,
            resolved_model_routes,
            resolved_effort_routes,
            proposed_concurrency,
            proposed_multiplier,
            forecast,
            reasons,
            uncertainties,
        })
    }

    pub fn mode(&self) -> OrchestrationMode {
        self.mode
    }
    pub fn proposed_profiles(&self) -> &[AgentProfile] {
        &self.proposed_profiles
    }
    pub fn resolved_model_routes(&self) -> &[ModelRoute] {
        &self.resolved_model_routes
    }
    pub fn resolved_effort_routes(&self) -> &[EffortRoute] {
        &self.resolved_effort_routes
    }
    pub fn forecast(&self) -> &Forecast {
        &self.forecast
    }
}
