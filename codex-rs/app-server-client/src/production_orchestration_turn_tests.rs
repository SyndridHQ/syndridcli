use super::production_orchestration_turn::ProductionOrchestrationTurnRunner;
use super::production_orchestration_turn::ProductionOrchestrationTurnRunnerInput;
use crate::AppServerEvent;
use crate::InProcessServerEvent;
use codex_app_server::ObjectiveOnlyProductionTurnContext;
use codex_app_server::OrchestrationTranscriptContext;
use codex_app_server::ProductionOrchestrationRuntime;
use codex_app_server::ProductionTurnAdmissionInput;
use codex_core::ExecutionModeSelection;
use codex_core::OpenRouterSetupCancellation as CancellationToken;
use codex_core::PlannerTaskSpecification;
use codex_core::PlanningContract;
use codex_core::ProductionApprovedToolAdapter;
use codex_core::ProductionOrchestrationInput;
use codex_core::ProductionOrchestrationRequestBuilder;
use codex_core::ProductionProviderRoute;
use codex_core::ProductionRoleBinding;
use codex_core::ProductionRoleDispatcher;
use codex_core::ProviderInvocationError;
use codex_core::ProviderInvocationRequest;
use codex_core::ProviderInvocationResult;
use codex_core::ProviderInvocationUsage;
use codex_core::ProviderSelection;
use codex_core::RoutingAssignment;
use codex_core::RoutingConnectionDirectory;
use codex_core::RoutingConnectionInfo;
use codex_core::RoutingProfile;
use codex_core::RoutingProfileId;
use codex_core::RoutingProfileRegistry;
use codex_core::RoutingRole;
use codex_core::SessionExecutionPolicyState;
use codex_core::SessionPolicySource;
use codex_core::SubagentFailurePolicy;
use codex_core::SubagentToolPolicy;
use codex_core::VerificationContract;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use tempfile::tempdir;
use tokio::sync::mpsc;

#[derive(Clone)]
struct FakeProvider;

impl codex_core::SubagentProvider for FakeProvider {
    async fn invoke(
        &self,
        request: ProviderInvocationRequest,
        _cancellation: CancellationToken,
    ) -> Result<ProviderInvocationResult, ProviderInvocationError> {
        Ok(ProviderInvocationResult {
            provider: request.provider,
            model: request.model,
            text: "bounded fake result".to_string(),
            finish_reason: Some("stop".to_string()),
            usage: Some(ProviderInvocationUsage {
                input_tokens: Some(1),
                output_tokens: Some(1),
                total_tokens: Some(2),
            }),
            request_id: None,
            tool_call: None,
        })
    }
}

fn profile_and_connections() -> (
    RoutingProfileRegistry,
    RoutingConnectionDirectory,
    RoutingProfileId,
) {
    let profile_id = RoutingProfileId::new("runner-test").expect("profile ID");
    let mut profile = RoutingProfile::new(profile_id.clone(), "Runner test", 1).expect("profile");
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ] {
        profile
            .assign(
                role,
                RoutingAssignment {
                    connection_id: "runner-connection".to_string(),
                    provider_id: "codex".to_string(),
                    model_id: "runner-model".to_string(),
                    enabled: true,
                    label: None,
                },
            )
            .expect("assignment");
    }
    let mut profiles = RoutingProfileRegistry::default();
    profiles.insert(profile).expect("profile insert");
    profiles.activate(&profile_id).expect("activate profile");
    let mut connections = RoutingConnectionDirectory::default();
    connections.insert(RoutingConnectionInfo {
        connection_id: "runner-connection".to_string(),
        provider_id: "codex".to_string(),
        enabled: true,
        validation: codex_core::ConnectionValidationStatus::Valid,
        authentication_supported: true,
        models: Some(vec!["runner-model".to_string()]),
    });
    (profiles, connections, profile_id)
}

fn make_input(root: &Path, policy: SubagentToolPolicy) -> ProductionOrchestrationInput {
    ProductionOrchestrationInput {
        run_id: "run-1".to_string(),
        instruction: "inspect the workspace".to_string(),
        context: Some("bounded context".to_string()),
        workspace_root: root.to_path_buf(),
        tasks: vec![PlannerTaskSpecification {
            task_id: "task-1".to_string(),
            instruction: "inspect the workspace".to_string(),
            context: None,
            tool_policy: policy.clone(),
            timeout: None,
        }],
        planning: PlanningContract::NotRequested,
        verification: VerificationContract::NotRequested,
        failure_policy: SubagentFailurePolicy::ContinueIndependent,
        repair_instruction: String::new(),
        approved_tool_policy: policy,
        cancellation: CancellationToken::new(),
        overall_timeout: None,
    }
}

fn make_dispatcher() -> ProductionRoleDispatcher {
    let route = ProductionProviderRoute::new(
        ProviderSelection::new("runner-connection", "codex", "runner-model")
            .expect("provider selection"),
        ReasoningEffort::Medium,
    );
    ProductionRoleDispatcher::new([
        (
            RoutingRole::Main,
            ProductionRoleBinding::new(route.clone(), FakeProvider),
        ),
        (
            RoutingRole::Planner,
            ProductionRoleBinding::new(route.clone(), FakeProvider),
        ),
        (
            RoutingRole::Executor,
            ProductionRoleBinding::new(route.clone(), FakeProvider),
        ),
        (
            RoutingRole::Verifier,
            ProductionRoleBinding::new(route.clone(), FakeProvider),
        ),
        (
            RoutingRole::Repair,
            ProductionRoleBinding::new(route, FakeProvider),
        ),
    ])
    .expect("dispatcher")
}

#[tokio::test]
async fn runner_composes_request_observations_result_and_transcript_once() {
    let root = tempdir().expect("tempdir");
    let policy =
        SubagentToolPolicy::for_workspace(root.path(), Default::default()).expect("tool policy");
    let tool_adapter = ProductionApprovedToolAdapter::new(policy.clone());
    let (profiles, connections, profile_id) = profile_and_connections();
    let builder = ProductionOrchestrationRequestBuilder::new(
        ExecutionModeSelection::Balanced,
        profile_id,
        profiles.clone(),
        connections.clone(),
    )
    .expect("request builder");
    let input = make_input(root.path(), policy);
    let state = SessionExecutionPolicyState::with_selection(
        ExecutionModeSelection::Balanced,
        SessionPolicySource::SessionOverride,
    )
    .expect("policy state");
    let runner = ProductionOrchestrationTurnRunner::new(ProductionOrchestrationTurnRunnerInput {
        builder,
        input,
        policy_state: state,
        dispatcher: make_dispatcher(),
        profiles,
        connections,
        tool_adapter,
        transcript_context: OrchestrationTranscriptContext {
            thread_id: "thread-1".to_string(),
            turn_id: "run-1".to_string(),
            assistant_item_id: "item-1".to_string(),
            completed_at_ms: 1,
        },
    })
    .expect("runner");
    let (events_tx, mut events_rx) = mpsc::channel(32);

    let completion = runner.run(events_tx).await.expect("runner completion");
    assert_eq!(completion.notifications.len(), 3);
    assert!(matches!(
        completion.result,
        codex_core::OrchestrationTurnResult::Completed { .. }
    ));

    let mut observation_count = 0;
    let mut transcript_count = 0;
    while let Ok(event) = events_rx.try_recv() {
        match event {
            AppServerEvent::OrchestrationObservation(_) => observation_count += 1,
            AppServerEvent::ServerNotification(_) => transcript_count += 1,
            AppServerEvent::Lagged { .. }
            | AppServerEvent::ServerRequest(_)
            | AppServerEvent::Disconnected { .. } => {}
        }
    }
    assert!(observation_count >= 1);
    assert_eq!(transcript_count, 3);
}

#[tokio::test]
async fn runner_rejects_inconsistent_workspace_before_coordinator_activity() {
    let root = tempdir().expect("tempdir");
    let other_root = tempdir().expect("other tempdir");
    let policy =
        SubagentToolPolicy::for_workspace(root.path(), Default::default()).expect("tool policy");
    let tool_adapter = ProductionApprovedToolAdapter::new(policy.clone());
    let (profiles, connections, profile_id) = profile_and_connections();
    let builder = ProductionOrchestrationRequestBuilder::new(
        ExecutionModeSelection::Balanced,
        profile_id,
        profiles.clone(),
        connections.clone(),
    )
    .expect("request builder");
    let result = ProductionOrchestrationTurnRunner::new(ProductionOrchestrationTurnRunnerInput {
        builder,
        input: make_input(other_root.path(), policy),
        policy_state: SessionExecutionPolicyState::new().expect("policy state"),
        dispatcher: make_dispatcher(),
        profiles,
        connections,
        tool_adapter,
        transcript_context: OrchestrationTranscriptContext {
            thread_id: "thread-1".to_string(),
            turn_id: "run-1".to_string(),
            assistant_item_id: "item-1".to_string(),
            completed_at_ms: 1,
        },
    });
    let Err(error) = result else {
        panic!("workspace mismatch must reject runner");
    };
    assert!(matches!(
        error,
        super::production_orchestration_turn::ProductionOrchestrationTurnRunnerError::InvalidInput(
            "workspace_root"
        )
    ));
}

#[tokio::test]
async fn concrete_runner_factory_prepares_owned_cancellable_work() {
    let root = tempdir().expect("tempdir");
    let policy =
        SubagentToolPolicy::for_workspace(root.path(), Default::default()).expect("tool policy");
    let tool_adapter = ProductionApprovedToolAdapter::new(policy.clone());
    let (profiles, connections, profile_id) = profile_and_connections();
    let builder = ProductionOrchestrationRequestBuilder::new(
        ExecutionModeSelection::Balanced,
        profile_id,
        profiles.clone(),
        connections.clone(),
    )
    .expect("request builder");
    let runner = ProductionOrchestrationTurnRunner::new(ProductionOrchestrationTurnRunnerInput {
        builder,
        input: make_input(root.path(), policy),
        policy_state: SessionExecutionPolicyState::with_selection(
            ExecutionModeSelection::Balanced,
            SessionPolicySource::SessionOverride,
        )
        .expect("policy state"),
        dispatcher: make_dispatcher(),
        profiles,
        connections,
        tool_adapter,
        transcript_context: OrchestrationTranscriptContext {
            thread_id: "thread-1".to_string(),
            turn_id: "run-1".to_string(),
            assistant_item_id: "item-1".to_string(),
            completed_at_ms: 1,
        },
    })
    .expect("runner");
    let runner_slot = Arc::new(Mutex::new(Some(runner)));
    let factory =
        super::production_runner_adapter::ProductionOrchestrationTurnRunnerFactory::new({
            let runner_slot = Arc::clone(&runner_slot);
            move |_input, _context| {
                runner_slot
                    .lock()
                    .expect("runner slot")
                    .take()
                    .ok_or(codex_app_server::ProductionTurnPreparationError::RunnerUnavailable)
            }
        });
    let runtime = ProductionOrchestrationRuntime::new(
        Arc::new(factory),
        Arc::new(ObjectiveOnlyProductionTurnContext),
    );
    let admission = ProductionTurnAdmissionInput::new(
        "run-1",
        "thread-1",
        "inspect the workspace",
        root.path().to_path_buf(),
    )
    .expect("admission input");
    let (events_tx, mut events_rx) = mpsc::channel::<InProcessServerEvent>(32);

    let prepared = runtime
        .prepare(admission, events_tx)
        .expect("prepared turn");
    assert!(events_rx.try_recv().is_err());
    assert!(prepared.request_cancel(codex_core::ProductionCancellationReason::User));
    assert!(!prepared.request_cancel(codex_core::ProductionCancellationReason::Timeout));
    prepared
        .into_completion()
        .await
        .expect("cancelled completion");
    assert!(matches!(
        events_rx.try_recv(),
        Ok(InProcessServerEvent::OrchestrationObservation(_))
            | Ok(InProcessServerEvent::ServerNotification(_))
    ));
}
