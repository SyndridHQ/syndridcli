use codex_protocol::openai_models::ReasoningEffort;
use serde::Deserialize;
use serde::Serialize;

use crate::DataQuality;

/// Where a requested route came from.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteSource {
    User,
    Policy,
    Inherited,
    Fallback,
}

/// Whether a route has been observed as resolved or rerouted.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteStatus {
    Requested,
    Resolved,
    Rerouted,
}

/// Requested and observed model routing without a model registry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRoute {
    pub requested: Option<String>,
    pub resolved: Option<String>,
    pub source: RouteSource,
    pub status: RouteStatus,
    pub data_quality: DataQuality,
}

/// Requested and observed reasoning effort using Codex's shared public type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffortRoute {
    pub requested: Option<ReasoningEffort>,
    pub resolved: Option<ReasoningEffort>,
    pub source: RouteSource,
    pub status: RouteStatus,
    pub data_quality: DataQuality,
}
