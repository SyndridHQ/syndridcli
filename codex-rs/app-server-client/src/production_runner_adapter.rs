//! Adapter from the concrete production runner to app-server's neutral seam.

use std::sync::Arc;

use codex_app_server::PreparedProductionTurn;
use codex_app_server::ProductionTurnAdmissionInput;
use codex_app_server::ProductionTurnPreparationError;
use codex_app_server::ProductionTurnRunError;
use codex_app_server::ProductionTurnRunnerFactory;
use codex_app_server::in_process::InProcessServerEvent;
use codex_core::ProductionOrchestrationCancellationHandle;
use codex_core::ProductionOrchestrationLifecycleError;
use tokio::sync::mpsc;

use crate::production_orchestration_turn::ProductionOrchestrationTurnRunner;
use crate::production_orchestration_turn::ProductionOrchestrationTurnRunnerError;

type RunnerBuilder = dyn Fn(
        &ProductionTurnAdmissionInput,
        Option<&str>,
    ) -> Result<ProductionOrchestrationTurnRunner, ProductionTurnPreparationError>
    + Send
    + Sync;

/// Binds trusted immutable production dependencies to the concrete 7G6B runner.
///
/// The builder is called during preparation only. It must validate and capture dependencies;
/// it must not spawn work or invoke providers. Execution starts only when the returned
/// [`PreparedProductionTurn`] future is polled.
pub(crate) struct ProductionOrchestrationTurnRunnerFactory {
    builder: Arc<RunnerBuilder>,
}

impl ProductionOrchestrationTurnRunnerFactory {
    pub(crate) fn new(
        builder: impl Fn(
            &ProductionTurnAdmissionInput,
            Option<&str>,
        )
            -> Result<ProductionOrchestrationTurnRunner, ProductionTurnPreparationError>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            builder: Arc::new(builder),
        }
    }
}

impl ProductionTurnRunnerFactory for ProductionOrchestrationTurnRunnerFactory {
    fn prepare(
        &self,
        input: ProductionTurnAdmissionInput,
        context: Option<String>,
        events: mpsc::Sender<InProcessServerEvent>,
    ) -> Result<PreparedProductionTurn, ProductionTurnPreparationError> {
        let runner = (self.builder)(&input, context.as_deref())?;
        let cancellation = ProductionOrchestrationCancellationHandle::new();
        let future_cancellation = cancellation.clone();
        let completion = Box::pin(async move {
            runner
                .run_in_process_with_cancellation(events, future_cancellation)
                .await
                .map(|_| ())
                .map_err(map_runner_error)
        });
        Ok(PreparedProductionTurn::new(cancellation, completion))
    }
}

fn map_runner_error(error: ProductionOrchestrationTurnRunnerError) -> ProductionTurnRunError {
    match error {
        ProductionOrchestrationTurnRunnerError::EventChannelClosed => {
            ProductionTurnRunError::EventDestinationClosed
        }
        ProductionOrchestrationTurnRunnerError::FinalDeliverable(_) => {
            ProductionTurnRunError::FinalDeliverableFailed
        }
        ProductionOrchestrationTurnRunnerError::Lifecycle(
            ProductionOrchestrationLifecycleError::CoordinatorJoinFailure,
        ) => ProductionTurnRunError::CoordinatorJoinFailure,
        ProductionOrchestrationTurnRunnerError::Lifecycle(
            ProductionOrchestrationLifecycleError::ObservationBridgeJoinFailure,
        ) => ProductionTurnRunError::ObservationBridgeJoinFailure,
        ProductionOrchestrationTurnRunnerError::Lifecycle(
            ProductionOrchestrationLifecycleError::ShutdownTimedOut,
        ) => ProductionTurnRunError::ShutdownTimedOut,
        ProductionOrchestrationTurnRunnerError::InvalidInput(_)
        | ProductionOrchestrationTurnRunnerError::Request(_)
        | ProductionOrchestrationTurnRunnerError::RouteDispatch(_)
        | ProductionOrchestrationTurnRunnerError::RouteMismatch(_)
        | ProductionOrchestrationTurnRunnerError::Lifecycle(
            ProductionOrchestrationLifecycleError::Coordinator(_),
        ) => ProductionTurnRunError::RunnerFailed,
    }
}
