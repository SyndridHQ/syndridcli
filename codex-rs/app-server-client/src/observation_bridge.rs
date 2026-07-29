use codex_core::OrchestrationObservationReceiver;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::AppServerEvent;

/// Forwards bounded core observation updates into an embedded app-server event stream.
///
/// The returned handle is the bridge's ownership boundary: callers must retain and
/// await it during session or run shutdown. A closed source or destination ends the
/// bridge without turning observation delivery into an execution failure.
pub fn spawn_observation_bridge(
    mut observations: OrchestrationObservationReceiver,
    events: mpsc::Sender<AppServerEvent>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while observations.changed().await.is_ok() {
            let Some(update) = observations.latest() else {
                continue;
            };
            if events
                .send(AppServerEvent::OrchestrationObservation(update))
                .await
                .is_err()
            {
                break;
            }
        }
    })
}

#[cfg(test)]
#[path = "observation_bridge_tests.rs"]
mod tests;
