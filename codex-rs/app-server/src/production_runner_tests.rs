use super::ObjectiveOnlyProductionTurnContext;
use super::PreparedProductionTurn;
use super::ProductionOrchestrationRuntime;
use super::ProductionSessionRuntime;
use super::ProductionTurnAdmissionId;
use super::ProductionTurnAdmissionInput;
use super::ProductionTurnContextProvider;
use super::ProductionTurnPreparationError;
use super::ProductionTurnRunError;
use super::ProductionTurnRunnerFactory;
use crate::in_process::InProcessServerEvent;
use codex_core::ProductionOrchestrationCancellationHandle;
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
        assert_eq!(input.thread_id(), "thread-1");
        let cancellation = ProductionOrchestrationCancellationHandle::new();
        let completion_cancellation = cancellation.clone();
        let completion = Box::pin(async move {
            let _ = events;
            let mut lifecycle = ProductionOrchestrationLifecycle::spawn_with_cancellation(
                "turn-1",
                completion_cancellation,
                |_| async { Ok::<(), ()>(()) },
            );
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
fn admission_ids_are_opaque_unique_and_session_scoped() {
    let first = ProductionTurnAdmissionId::new("thread-1").expect("valid session");
    let second = ProductionTurnAdmissionId::new("thread-1").expect("valid session");
    assert_ne!(first, second);
    assert!(first.as_str().starts_with("thread-1:"));
    assert!(first.belongs_to("thread-1"));
    assert!(!first.belongs_to("thread-2"));
    assert!(ProductionTurnAdmissionId::new("").is_err());
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

#[tokio::test]
async fn session_runtime_binds_one_event_destination_without_starting_work() {
    let runtime = Arc::new(ProductionOrchestrationRuntime::new(
        Arc::new(FakeRunner),
        Arc::new(ObjectiveOnlyProductionTurnContext),
    ));
    let (events_tx, mut events_rx) = mpsc::channel(1);
    let admission_id = ProductionTurnAdmissionId::new("thread-1").expect("valid session");
    let admission = ProductionTurnAdmissionInput::new(
        admission_id.as_str(),
        "thread-1",
        "inspect",
        std::path::PathBuf::from("/workspace"),
    )
    .expect("valid admission input");
    let session_runtime = ProductionSessionRuntime::new("thread-1".to_string(), runtime, events_tx);
    let prepared = session_runtime.prepare(admission).expect("prepared");
    assert!(events_rx.try_recv().is_err());
    prepared
        .into_completion()
        .await
        .expect("fake run should complete");
}
