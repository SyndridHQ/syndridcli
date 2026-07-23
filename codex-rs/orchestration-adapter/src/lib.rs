//! Runtime-neutral contract between Syndrid orchestration and Codex native agents.
//!
//! This crate contains only bounded adapter request, response, capability, observation, and error data.
//! A later Codex-core adapter owns execution and supplies native runtime identities.

mod cancellation;
mod capabilities;
mod error;
mod handoff;
mod identity;
mod observation;
mod operation;
mod spawn;

pub use cancellation::CancelChildOutcome;
pub use cancellation::CancelChildRequest;
pub use cancellation::CancelChildResult;
pub use cancellation::CancellationProvenance;
pub use capabilities::AdapterCapabilities;
pub use error::AdapterError;
pub use error::AdapterErrorKind;
pub use error::Retryability;
pub use handoff::DeliverHandoffRequest;
pub use handoff::DeliverHandoffResult;
pub use handoff::HandoffDeliveryOutcome;
pub use identity::MAX_RUNTIME_ID_BYTES;
pub use identity::RuntimeAgentId;
pub use identity::RuntimeIdentityError;
pub use observation::ChildObservation;
pub use observation::ObserveChildRequest;
pub use operation::AdapterRequest;
pub use operation::AdapterRequestKind;
pub use operation::AdapterResponse;
pub use operation::AdapterResponseKind;
pub use spawn::SpawnChildRequest;
pub use spawn::SpawnChildResult;
pub use spawn::SpawnRequestError;

#[cfg(test)]
#[path = "o2a_tests.rs"]
mod o2a_tests;

#[cfg(test)]
#[path = "o2b_tests.rs"]
mod o2b_tests;
