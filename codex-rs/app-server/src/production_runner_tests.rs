use super::ObjectiveOnlyProductionTurnContext;
use super::PreparedProductionTurn;
use super::ProductionOrchestrationRuntime;
use super::ProductionTurnAdmissionInput;
use super::ProductionTurnContextProvider;
use super::ProductionTurnPreparationError;
use super::ProductionTurnRunError;
use super::ProductionTurnRunnerFactory;
use crate::in_process::InProcessServerEvent;
use codex_core::ProductionOrchestrationLifecycle;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use tokio::sync::mpsc;

struct FakeRunner;

impl ProductionTurnRunnerFactory for FakeRunner {
    fn prepare(
        &self,
        input: ProductionTurnAdmissionInput,
        _context: Option<String>,
        events: mpsc::Sender<InProcessServerEvent>,
    ) -> Result<PreparedProductionTurn, ProductionTurnPreparationError> {
        assert_eq!(input.turn_id(), "turn-1");
        let lifecycle =
            ProductionOrchestrationLifecycle::spawn("turn-1", |_| async { Ok::<(), ()>(()) });
        let cancellation = lifecycle.cancellation_handle();
        let completion = Box::pin(async move {
            let _ = events;
            let mut lifecycle = lifecycle;
            lifecycle
                .complete()
                .await
                .map(|_| ())
                .map_err(|_| ProductionTurnRunError::RunnerFailed)
        });
        Ok(PreparedProductionTurn::new(cancellation, completion))
    }
}

struct MissingContext;

impl ProductionTurnContextProvider for MissingContext {
    fn capture(
        &self,
        _input: &ProductionTurnAdmissionInput,
    ) -> Result<Option<String>, ProductionTurnPreparationError> {
        Err(ProductionTurnPreparationError::ContextUnavailable)
    }
}

fn input(objective: &str) -> ProductionTurnAdmissionInput {
    ProductionTurnAdmissionInput::new(
        "turn-1",
        "thread-1",
        objective,
        std::path::PathBuf::from("/workspace"),
    )
    .expect("valid admission input")
}

#[test]
fn admission_input_is_bounded_and_utf8_safe() {
    let value = input("inspect café");
    assert_eq!(value.objective(), "inspect café");
    assert_eq!(value.thread_id(), "thread-1");
    assert!(
        ProductionTurnAdmissionInput::new(
            "turn-1",
            "thread-1",
            "x".repeat(super::MAX_PRODUCTION_OBJECTIVE_BYTES + 1),
            std::path::PathBuf::from("/workspace"),
        )
        .is_err()
    );
}

#[test]
fn runtime_debug_is_redacted_and_context_provider_is_explicit() {
    let runtime = ProductionOrchestrationRuntime::new(
        Arc::new(FakeRunner),
        Arc::new(ObjectiveOnlyProductionTurnContext),
    );
    let debug = format!("{runtime:?}");
    assert!(!debug.contains("credential"));
    assert!(!debug.contains("token"));
}

#[test]
fn missing_context_fails_before_runner_preparation() {
    let runtime =
        ProductionOrchestrationRuntime::new(Arc::new(FakeRunner), Arc::new(MissingContext));
    let (events_tx, _events_rx) = mpsc::channel(1);
    assert!(matches!(
        runtime.prepare(input("inspect"), events_tx),
        Err(ProductionTurnPreparationError::ContextUnavailable)
    ));
}

#[test]
fn missing_runtime_fails_before_runner_preparation() {
    let (events_tx, _events_rx) = mpsc::channel(1);
    assert!(matches!(
        ProductionOrchestrationRuntime::prepare_optional(None, input("inspect"), events_tx),
        Err(ProductionTurnPreparationError::RunnerUnavailable)
    ));
}

#[tokio::test]
async fn prepared_run_exposes_cancellation_and_owned_completion() {
    let runtime = ProductionOrchestrationRuntime::new(
        Arc::new(FakeRunner),
        Arc::new(ObjectiveOnlyProductionTurnContext),
    );
    let (events_tx, _events_rx) = mpsc::channel(1);
    let prepared = runtime
        .prepare(input("inspect"), events_tx)
        .expect("prepared");
    let cancellation = prepared.cancellation_handle();
    assert!(!cancellation.is_cancelled());
    prepared
        .into_completion()
        .await
        .expect("fake run should complete");
}
