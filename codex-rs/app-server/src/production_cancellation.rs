use codex_core::ProductionCancellationReason;
use codex_core::ProductionOrchestrationCancellationHandle;

/// Associates one future orchestration lifecycle with its admitted turn.
///
/// The registration is stored alongside the existing per-thread state rather
/// than introducing a second active-turn registry. It is not populated until
/// production Syndrid execution is enabled in a later milestone.
pub(crate) struct ProductionOrchestrationCancellationRegistration {
    turn_id: String,
    handle: ProductionOrchestrationCancellationHandle,
}

impl ProductionOrchestrationCancellationRegistration {
    pub(crate) fn new(
        turn_id: impl Into<String>,
        handle: ProductionOrchestrationCancellationHandle,
    ) -> Self {
        Self {
            turn_id: turn_id.into(),
            handle,
        }
    }

    pub(crate) fn matches(&self, turn_id: &str) -> bool {
        self.turn_id == turn_id
    }

    pub(crate) fn request_cancel(
        &self,
        turn_id: &str,
        reason: ProductionCancellationReason,
    ) -> bool {
        self.matches(turn_id) && self.handle.request_cancel(reason)
    }

    pub(crate) fn request_shutdown(&self) -> bool {
        self.handle
            .request_cancel(ProductionCancellationReason::SessionShutdown)
    }
}

#[cfg(test)]
#[path = "production_cancellation_tests.rs"]
mod tests;
