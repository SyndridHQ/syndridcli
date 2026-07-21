//! Runtime-neutral contract between Syndrid orchestration and Codex native agents.
//!
//! This crate contains only bounded spawn and handoff request/result data.
//! A later Codex-core adapter owns execution and supplies native runtime identities.

mod handoff;
mod identity;
mod spawn;

pub use handoff::DeliverHandoffRequest;
pub use handoff::DeliverHandoffResult;
pub use handoff::HandoffDeliveryOutcome;
pub use identity::MAX_RUNTIME_ID_BYTES;
pub use identity::RuntimeAgentId;
pub use identity::RuntimeIdentityError;
pub use spawn::SpawnChildRequest;
pub use spawn::SpawnChildResult;
pub use spawn::SpawnRequestError;

#[cfg(test)]
#[path = "o2a_tests.rs"]
mod o2a_tests;
