use super::orchestration_observability::OrchestrationObservationSnapshot;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use tokio::sync::watch;

/// A bounded, generation-aware observation update for an embedded consumer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestrationObservationUpdate {
    pub generation: u64,
    pub sequence: u64,
    pub snapshot: OrchestrationObservationSnapshot,
}

/// Publishes authoritative Phase 7D snapshots without depending on a UI or transport crate.
///
/// Implementations must not block the coordinator. Closed consumers are treated as normal
/// teardown and must not change the orchestration result.
pub trait OrchestrationObservationSink: Send + Sync {
    fn publish(&self, snapshot: OrchestrationObservationSnapshot);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopOrchestrationObservationSink;

impl OrchestrationObservationSink for NoopOrchestrationObservationSink {
    fn publish(&self, _snapshot: OrchestrationObservationSnapshot) {}
}

/// Creates a latest-state observation channel for one orchestration run.
pub fn observation_channel() -> (
    WatchOrchestrationObservationSink,
    OrchestrationObservationReceiver,
) {
    let (sender, receiver) = watch::channel(None);
    (
        WatchOrchestrationObservationSink {
            sender,
            next_sequence: Arc::new(AtomicU64::new(0)),
        },
        OrchestrationObservationReceiver { receiver },
    )
}

/// A bounded watch-backed sink. Only the latest update is retained.
#[derive(Clone)]
pub struct WatchOrchestrationObservationSink {
    sender: watch::Sender<Option<OrchestrationObservationUpdate>>,
    next_sequence: Arc<AtomicU64>,
}

impl std::fmt::Debug for WatchOrchestrationObservationSink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WatchOrchestrationObservationSink")
            .field("receiver_count", &self.sender.receiver_count())
            .finish()
    }
}

impl OrchestrationObservationSink for WatchOrchestrationObservationSink {
    fn publish(&self, snapshot: OrchestrationObservationSnapshot) {
        let Some(generation) = snapshot.generation.value else {
            // A runtime snapshot without an exact generation is not safe to route
            // to a per-run consumer, so drop it rather than inventing an identity.
            return;
        };
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        let _ = self
            .sender
            .send_replace(Some(OrchestrationObservationUpdate {
                generation,
                sequence,
                snapshot,
            }));
    }
}

/// Consumer handle for one bounded observation channel.
pub struct OrchestrationObservationReceiver {
    receiver: watch::Receiver<Option<OrchestrationObservationUpdate>>,
}

impl OrchestrationObservationReceiver {
    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        self.receiver.changed().await
    }

    pub fn borrow(&self) -> Option<OrchestrationObservationUpdate> {
        self.receiver.borrow().clone()
    }

    pub fn latest(&self) -> Option<OrchestrationObservationUpdate> {
        self.borrow()
    }
}

impl Clone for OrchestrationObservationReceiver {
    fn clone(&self) -> Self {
        Self {
            receiver: self.receiver.clone(),
        }
    }
}
