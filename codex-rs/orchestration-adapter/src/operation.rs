use serde::Deserialize;
use serde::Serialize;

use crate::CancelChildRequest;
use crate::CancelChildResult;
use crate::ChildObservation;
use crate::DeliverHandoffRequest;
use crate::DeliverHandoffResult;
use crate::ObserveChildRequest;
use crate::SpawnChildRequest;
use crate::SpawnChildResult;

/// Closed operation set accepted by a future adapter implementation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AdapterRequestKind {
    SpawnChild(SpawnChildRequest),
    DeliverHandoff(DeliverHandoffRequest),
    CancelChild(CancelChildRequest),
    ObserveChild(ObserveChildRequest),
}

/// Correlation envelope for one data-only adapter request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterRequest {
    request_id: u64,
    kind: AdapterRequestKind,
}

impl AdapterRequest {
    pub fn new(request_id: u64, kind: AdapterRequestKind) -> Self {
        Self { request_id, kind }
    }
    pub fn request_id(&self) -> u64 {
        self.request_id
    }
    pub fn kind(&self) -> &AdapterRequestKind {
        &self.kind
    }
}

/// Closed response set produced by a future adapter implementation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AdapterResponseKind {
    SpawnChild(SpawnChildResult),
    DeliverHandoff(DeliverHandoffResult),
    CancelChild(CancelChildResult),
    ObserveChild(ChildObservation),
}

/// Correlation envelope for one data-only adapter response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterResponse {
    request_id: u64,
    kind: AdapterResponseKind,
}

impl AdapterResponse {
    pub fn new(request_id: u64, kind: AdapterResponseKind) -> Self {
        Self { request_id, kind }
    }
    pub fn request_id(&self) -> u64 {
        self.request_id
    }
    pub fn kind(&self) -> &AdapterResponseKind {
        &self.kind
    }
}
