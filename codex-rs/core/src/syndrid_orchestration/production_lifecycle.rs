use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const DEFAULT_COMPLETION_TIMEOUT: Duration = Duration::from_secs(30);

/// Identifies why a production orchestration lifecycle was asked to stop.
///
/// The reason is metadata for the lifecycle boundary. Phase 7E remains the
/// authority that selects the terminal orchestration cause.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductionCancellationReason {
    User,
    Timeout,
    SessionShutdown,
}

#[derive(Debug)]
struct CancellationState {
    reason: Mutex<Option<ProductionCancellationReason>>,
    token: CancellationToken,
}

impl Default for CancellationState {
    fn default() -> Self {
        Self {
            reason: Mutex::new(None),
            token: CancellationToken::new(),
        }
    }
}

/// A cloneable cancellation handle for one admitted orchestration run.
///
/// App-server integration may retain this handle to request cancellation. It
/// does not own child tasks, terminal-cause arbitration, or cleanup.
#[derive(Clone, Debug)]
pub struct ProductionOrchestrationCancellationHandle {
    state: Arc<CancellationState>,
}

impl ProductionOrchestrationCancellationHandle {
    /// Requests cancellation once and returns whether this call set the reason.
    pub fn request_cancel(&self, reason: ProductionCancellationReason) -> bool {
        let Ok(mut current_reason) = self.state.reason.lock() else {
            return false;
        };
        if current_reason.is_some() {
            return false;
        }
        *current_reason = Some(reason);
        self.state.token.cancel();
        true
    }

    /// Reports the first cancellation reason selected for this run.
    pub fn cancellation_reason(&self) -> Option<ProductionCancellationReason> {
        self.state.reason.lock().ok().and_then(|reason| *reason)
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.state.token.is_cancelled()
    }
}

/// Lifecycle states visible to the owner of one production orchestration run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductionOrchestrationLifecycleState {
    Running,
    CancellationRequested,
    Completed,
    ShutdownTimedOut,
}

/// Errors produced while joining owned production orchestration tasks.
#[derive(Debug, Eq, PartialEq)]
pub enum ProductionOrchestrationLifecycleError<E> {
    Coordinator(E),
    CoordinatorJoinFailure,
    ObservationBridgeJoinFailure,
    ShutdownTimedOut,
}

impl<E: fmt::Display> fmt::Display for ProductionOrchestrationLifecycleError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Coordinator(error) => write!(formatter, "coordinator failed: {error}"),
            Self::CoordinatorJoinFailure => formatter.write_str("coordinator task join failed"),
            Self::ObservationBridgeJoinFailure => {
                formatter.write_str("observation bridge task join failed")
            }
            Self::ShutdownTimedOut => formatter.write_str("orchestration shutdown timed out"),
        }
    }
}

/// Owns every asynchronous task associated with one future production run.
///
/// This owner is independent of providers, policy, TUI types, and app-server
/// protocols. The coordinator future receives the root token; its existing
/// Phase 7 cleanup remains responsible for child work and reservations. The
/// owner only requests cancellation and joins tasks.
pub struct ProductionOrchestrationLifecycle<T, E> {
    run_id: String,
    cancellation: ProductionOrchestrationCancellationHandle,
    coordinator: Option<JoinHandle<Result<T, E>>>,
    observation_bridge: Option<JoinHandle<()>>,
    state: ProductionOrchestrationLifecycleState,
}

impl<T, E> ProductionOrchestrationLifecycle<T, E>
where
    T: Send + 'static,
    E: Send + 'static,
{
    /// Starts ownership of one coordinator future and creates its root token.
    pub fn spawn<F, Fut>(run_id: impl Into<String>, coordinator: F) -> Self
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
    {
        let state = Arc::new(CancellationState::default());
        let token = state.token.clone();
        let coordinator = tokio::spawn(coordinator(token));
        Self {
            run_id: run_id.into(),
            cancellation: ProductionOrchestrationCancellationHandle { state },
            coordinator: Some(coordinator),
            observation_bridge: None,
            state: ProductionOrchestrationLifecycleState::Running,
        }
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn cancellation_handle(&self) -> ProductionOrchestrationCancellationHandle {
        self.cancellation.clone()
    }

    pub fn state(&self) -> ProductionOrchestrationLifecycleState {
        self.state
    }

    /// Retains ownership of the observation bridge until completion or shutdown.
    pub fn attach_observation_bridge(&mut self, bridge: JoinHandle<()>) {
        self.observation_bridge = Some(bridge);
    }

    /// Requests cancellation without directly manipulating child tasks.
    pub fn request_cancel(&mut self, reason: ProductionCancellationReason) -> bool {
        let requested = self.cancellation.request_cancel(reason);
        if requested {
            self.state = ProductionOrchestrationLifecycleState::CancellationRequested;
        }
        requested
    }

    /// Joins the coordinator and then the observation bridge within a bound.
    pub async fn complete(&mut self) -> Result<T, ProductionOrchestrationLifecycleError<E>> {
        self.finish_within(DEFAULT_COMPLETION_TIMEOUT).await
    }

    /// Requests session-shutdown cancellation and joins all tasks within a bound.
    pub async fn shutdown(
        &mut self,
        timeout: Duration,
    ) -> Result<T, ProductionOrchestrationLifecycleError<E>> {
        self.request_cancel(ProductionCancellationReason::SessionShutdown);
        self.finish_within(timeout).await
    }

    async fn finish_within(
        &mut self,
        timeout: Duration,
    ) -> Result<T, ProductionOrchestrationLifecycleError<E>> {
        match tokio::time::timeout(timeout, self.join_owned_tasks()).await {
            Ok(result) => {
                self.state = ProductionOrchestrationLifecycleState::Completed;
                result
            }
            Err(_) => {
                self.abort_and_join().await;
                self.state = ProductionOrchestrationLifecycleState::ShutdownTimedOut;
                Err(ProductionOrchestrationLifecycleError::ShutdownTimedOut)
            }
        }
    }

    async fn join_owned_tasks(&mut self) -> Result<T, ProductionOrchestrationLifecycleError<E>> {
        let coordinator_result = self.join_coordinator().await;
        let bridge_result = self.join_observation_bridge().await;
        match (coordinator_result, bridge_result) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(result), Ok(())) => {
                result.map_err(ProductionOrchestrationLifecycleError::Coordinator)
            }
        }
    }

    async fn join_coordinator(
        &mut self,
    ) -> Result<Result<T, E>, ProductionOrchestrationLifecycleError<E>> {
        let Some(handle) = self.coordinator.as_mut() else {
            return Err(ProductionOrchestrationLifecycleError::CoordinatorJoinFailure);
        };
        let result = (&mut *handle).await;
        self.coordinator = None;
        result.map_err(|_| ProductionOrchestrationLifecycleError::CoordinatorJoinFailure)
    }

    async fn join_observation_bridge(
        &mut self,
    ) -> Result<(), ProductionOrchestrationLifecycleError<E>> {
        let Some(handle) = self.observation_bridge.as_mut() else {
            return Ok(());
        };
        let result = (&mut *handle).await;
        self.observation_bridge = None;
        result.map_err(|_| ProductionOrchestrationLifecycleError::ObservationBridgeJoinFailure)
    }

    async fn abort_and_join(&mut self) {
        if let Some(handle) = self.coordinator.as_mut() {
            handle.abort();
            let _ = (&mut *handle).await;
        }
        self.coordinator = None;
        if let Some(handle) = self.observation_bridge.as_mut() {
            handle.abort();
            let _ = (&mut *handle).await;
        }
        self.observation_bridge = None;
    }
}

impl<T, E> Drop for ProductionOrchestrationLifecycle<T, E> {
    fn drop(&mut self) {
        if matches!(
            self.state,
            ProductionOrchestrationLifecycleState::Running
                | ProductionOrchestrationLifecycleState::CancellationRequested
        ) {
            let _ = self
                .cancellation
                .request_cancel(ProductionCancellationReason::SessionShutdown);
        }
        if let Some(handle) = self.coordinator.as_ref() {
            handle.abort();
        }
        if let Some(handle) = self.observation_bridge.as_ref() {
            handle.abort();
        }
    }
}

#[cfg(test)]
#[path = "production_lifecycle_tests.rs"]
mod tests;
