use std::sync::Arc;

use codex_app_server::ObjectiveOnlyProductionTurnContext;
use codex_app_server::PreparedProductionTurn;
use codex_app_server::ProductionTurnAdmissionInput;
use codex_app_server::ProductionTurnFuture;
use codex_app_server::ProductionTurnPreparationError;
use codex_app_server::ProductionTurnRunnerFactory;
use codex_app_server::in_process::InProcessServerEvent;
use codex_core::ProductionCancellationReason;
use codex_core::ProductionOrchestrationCancellationHandle;
use pretty_assertions::assert_eq;
use tokio::sync::mpsc;

use super::TrustedProductionRuntimeBuilder;
use super::TrustedProductionRuntimeDependencies;
use super::TrustedRuntimeConstructionError;

#[derive(Debug)]
struct FakeRunnerFactory;

impl ProductionTurnRunnerFactory for FakeRunnerFactory {
    fn prepare(
        &self,
        _input: ProductionTurnAdmissionInput,
        _context: Option<String>,
        _events: mpsc::Sender<InProcessServerEvent>,
    ) -> Result<PreparedProductionTurn, ProductionTurnPreparationError> {
        let cancellation = ProductionOrchestrationCancellationHandle::new();
        let completion: ProductionTurnFuture = Box::pin(async { Ok(()) });
        Ok(PreparedProductionTurn::new(cancellation, completion))
    }
}

fn dependencies(
    runner_factory: Option<Arc<dyn ProductionTurnRunnerFactory>>,
    context_provider: Option<Arc<dyn codex_app_server::ProductionTurnContextProvider>>,
) -> TrustedProductionRuntimeDependencies {
    TrustedProductionRuntimeDependencies {
        session_id: "session-1".to_owned(),
        runner_factory,
        context_provider,
    }
}

#[test]
fn trusted_builder_constructs_runtime_without_starting_work() {
    let (events, mut event_rx) = mpsc::channel(1);
    let runtime = TrustedProductionRuntimeBuilder::new(dependencies(
        Some(Arc::new(FakeRunnerFactory)),
        Some(Arc::new(ObjectiveOnlyProductionTurnContext)),
    ))
    .build(events)
    .expect("trusted dependencies should construct a runtime");

    let admission_id = codex_app_server::ProductionTurnAdmissionId::new("session-1")
        .expect("admission identity should be valid");
    let input = ProductionTurnAdmissionInput::new(
        admission_id.as_str(),
        "thread-1",
        "bounded objective",
        std::env::current_dir().expect("current directory should be available"),
    )
    .expect("admission input should be valid");
    let prepared = runtime
        .prepare(input)
        .expect("runtime preparation should succeed");
    assert!(prepared.request_cancel(ProductionCancellationReason::User));
    assert!(event_rx.try_recv().is_err());
}

#[test]
fn missing_runner_is_rejected_before_runtime_construction() {
    let (_events, _event_rx) = mpsc::channel(1);
    let error = TrustedProductionRuntimeBuilder::new(dependencies(
        None,
        Some(Arc::new(ObjectiveOnlyProductionTurnContext)),
    ))
    .build(_events)
    .expect_err("missing runner should fail construction");
    assert_eq!(error, TrustedRuntimeConstructionError::RunnerUnavailable);
}

#[test]
fn missing_context_provider_is_rejected_before_runtime_construction() {
    let (events, _event_rx) = mpsc::channel(1);
    let error =
        TrustedProductionRuntimeBuilder::new(dependencies(Some(Arc::new(FakeRunnerFactory)), None))
            .build(events)
            .expect_err("missing context should fail construction");
    assert_eq!(
        error,
        TrustedRuntimeConstructionError::ContextProviderUnavailable
    );
}

#[test]
fn invalid_session_identity_is_rejected() {
    let (events, _event_rx) = mpsc::channel(1);
    let dependencies = TrustedProductionRuntimeDependencies {
        session_id: "   ".to_owned(),
        runner_factory: Some(Arc::new(FakeRunnerFactory)),
        context_provider: Some(Arc::new(ObjectiveOnlyProductionTurnContext)),
    };
    let error = TrustedProductionRuntimeBuilder::new(dependencies)
        .build(events)
        .expect_err("invalid session identity should fail construction");
    assert_eq!(
        error,
        TrustedRuntimeConstructionError::InvalidSessionIdentity
    );
}

#[test]
fn trusted_dependency_debug_output_is_redacted() {
    let dependencies = dependencies(
        Some(Arc::new(FakeRunnerFactory)),
        Some(Arc::new(ObjectiveOnlyProductionTurnContext)),
    );
    let debug = format!("{dependencies:?}");
    assert!(!debug.contains("FakeRunnerFactory"));
    assert!(!debug.contains("session-1"));
    assert!(debug.contains("<redacted>"));
}
