use super::EnvironmentManager;
use super::InProcessAppServerClient;
use super::InProcessClientStartArgs;
use super::InProcessServerEvent;
use super::ProductionExecutionCapability;
use super::RequestId;
use super::SessionSource;
use super::TrustedProductionRuntimeBuilder;
use super::TrustedProductionRuntimeDependencies;
use super::production_orchestration_turn::ProductionOrchestrationTurnRunner;
use super::production_orchestration_turn::ProductionOrchestrationTurnRunnerInput;
use super::production_runner_adapter::ProductionOrchestrationTurnRunnerFactory;
use codex_app_server::ObjectiveOnlyProductionTurnContext;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStatus;
use codex_app_server_protocol::UserInput;
use codex_arg0::Arg0DispatchPaths;
use codex_config::CloudConfigBundleLoader;
use codex_config::LoaderOverrides;
use codex_core::ConnectionValidationStatus;
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
use codex_core::SubagentProvider;
use codex_core::SubagentToolPolicy;
use codex_core::VerificationContract;
use codex_core::config::ConfigBuilder;
use codex_feedback::CodexFeedback;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tempfile::TempDir;
use tokio::time::Duration;
use tokio::time::timeout;

const PRIVATE_PROVIDER_FAILURE_SENTINEL: &str = "PRIVATE_PROVIDER_FAILURE_SENTINEL";

#[derive(Clone, Debug, Eq, PartialEq)]
struct InvocationRecord {
    turn_id: String,
    role: RoutingRole,
    provider: String,
    connection: String,
    model: String,
    effort: ReasoningEffort,
    tool_count: usize,
}

#[derive(Default)]
struct ProviderState {
    invocations: Vec<InvocationRecord>,
    admissions: Vec<String>,
    active_turn: Option<String>,
    private_failure_details: Vec<String>,
    planner_failure_pending: bool,
}

#[derive(Clone)]
struct RecordingProvider {
    role: RoutingRole,
    connection: String,
    state: Arc<Mutex<ProviderState>>,
}

impl SubagentProvider for RecordingProvider {
    async fn invoke(
        &self,
        request: ProviderInvocationRequest,
        _cancellation: CancellationToken,
    ) -> Result<ProviderInvocationResult, ProviderInvocationError> {
        let model = request.model.clone();
        let provider = request.provider.clone();
        let mut state = self.state.lock().expect("provider state lock");
        let turn_id = state
            .active_turn
            .clone()
            .expect("provider invocation has an active admission");
        state.invocations.push(InvocationRecord {
            turn_id,
            role: self.role,
            provider: provider.clone(),
            connection: self.connection.clone(),
            model: model.clone(),
            effort: ReasoningEffort::Medium,
            tool_count: request.tools.len(),
        });
        if self.role == RoutingRole::Planner && state.planner_failure_pending {
            state.planner_failure_pending = false;
            state
                .private_failure_details
                .push(PRIVATE_PROVIDER_FAILURE_SENTINEL.to_string());
            return Err(ProviderInvocationError::ProviderUnavailable);
        }
        Ok(ProviderInvocationResult {
            provider,
            model,
            text: "bounded provider success".to_string(),
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

fn route_for(role: RoutingRole) -> ProductionProviderRoute {
    let name = role.to_string().to_lowercase();
    ProductionProviderRoute::new(
        ProviderSelection::new(
            format!("{name}-connection"),
            "codex",
            format!("{name}-model"),
        )
        .expect("fake provider selection"),
        ReasoningEffort::Medium,
    )
}

fn profiles_and_connections() -> (
    RoutingProfileRegistry,
    RoutingConnectionDirectory,
    RoutingProfileId,
) {
    let profile_id = RoutingProfileId::new("provider-failure-test").expect("profile id");
    let mut profile =
        RoutingProfile::new(profile_id.clone(), "Provider failure test", 1).expect("profile");
    let mut connections = RoutingConnectionDirectory::default();
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ] {
        let route = route_for(role);
        let selection = route.selection();
        profile
            .assign(
                role,
                RoutingAssignment {
                    connection_id: selection.connection_id.clone(),
                    provider_id: selection.provider_id.clone(),
                    model_id: selection.model_id.clone(),
                    enabled: true,
                    label: None,
                    pool_id: None,
                },
            )
            .expect("role assignment");
        connections.insert(RoutingConnectionInfo {
            connection_id: selection.connection_id.clone(),
            provider_id: selection.provider_id.clone(),
            enabled: true,
            validation: ConnectionValidationStatus::Valid,
            authentication_supported: true,
            models: Some(vec![selection.model_id.clone()]),
        });
    }
    let mut profiles = RoutingProfileRegistry::default();
    profiles.insert(profile).expect("profile insert");
    profiles.activate(&profile_id).expect("profile activation");
    (profiles, connections, profile_id)
}

fn dispatcher(state: Arc<Mutex<ProviderState>>) -> ProductionRoleDispatcher {
    ProductionRoleDispatcher::new(
        [
            RoutingRole::Main,
            RoutingRole::Planner,
            RoutingRole::Executor,
            RoutingRole::Verifier,
            RoutingRole::Repair,
        ]
        .into_iter()
        .map(|role| {
            let route = route_for(role);
            let provider = RecordingProvider {
                role,
                connection: route.selection().connection_id.clone(),
                state: Arc::clone(&state),
            };
            (role, ProductionRoleBinding::new(route, provider))
        }),
    )
    .expect("dispatcher")
}

fn runner_factory(
    state: Arc<Mutex<ProviderState>>,
    workspace: PathBuf,
) -> ProductionOrchestrationTurnRunnerFactory {
    let (profiles, connections, profile_id) = profiles_and_connections();
    ProductionOrchestrationTurnRunnerFactory::new(move |admission, _context| {
        let mut provider_state = state.lock().expect("provider state lock");
        provider_state
            .admissions
            .push(admission.turn_id().to_string());
        provider_state.active_turn = Some(admission.turn_id().to_string());
        drop(provider_state);
        let policy = ExecutionModeSelection::Balanced;
        let builder = ProductionOrchestrationRequestBuilder::new(
            policy.clone(),
            profile_id.clone(),
            profiles.clone(),
            connections.clone(),
        )
        .map_err(|_| codex_app_server::ProductionTurnPreparationError::RoutingUnavailable)?;
        let tool_policy = SubagentToolPolicy::for_workspace(&workspace, Default::default())
            .map_err(|_| codex_app_server::ProductionTurnPreparationError::PolicyUnavailable)?;
        let input = ProductionOrchestrationInput {
            run_id: admission.turn_id().to_string(),
            instruction: admission.objective().to_string(),
            context: Some("bounded provider-failure context".to_string()),
            workspace_root: workspace.clone(),
            tasks: vec![PlannerTaskSpecification {
                task_id: "provider-failure-task".to_string(),
                instruction: "perform the bounded test task".to_string(),
                context: None,
                tool_policy: tool_policy.clone(),
                timeout: None,
            }],
            planning: PlanningContract::Required {
                instruction: "plan the bounded test task".to_string(),
            },
            verification: VerificationContract::NotRequested,
            failure_policy: SubagentFailurePolicy::ContinueIndependent,
            repair_instruction: String::new(),
            approved_tool_policy: tool_policy.clone(),
            cancellation: CancellationToken::new(),
            overall_timeout: None,
        };
        let tool_adapter = ProductionApprovedToolAdapter::new(tool_policy);
        ProductionOrchestrationTurnRunner::new(ProductionOrchestrationTurnRunnerInput {
            strategy: codex_core::OrchestrationMode::Manual,
            builder,
            input,
            policy_state: SessionExecutionPolicyState::with_strategy_selection(
                codex_core::OrchestrationMode::Manual,
                policy,
                SessionPolicySource::SessionOverride,
            )
            .map_err(|_| codex_app_server::ProductionTurnPreparationError::PolicyUnavailable)?,
            dispatcher: dispatcher(Arc::clone(&state)),
            profiles: profiles.clone(),
            connections: connections.clone(),
            tool_adapter,
            transcript_context: codex_app_server::OrchestrationTranscriptContext {
                thread_id: admission.thread_id().to_string(),
                turn_id: admission.turn_id().to_string(),
                assistant_item_id: format!("assistant-{}", admission.turn_id()),
                completed_at_ms: 1,
            },
        })
        .map_err(|_| codex_app_server::ProductionTurnPreparationError::RunnerUnavailable)
    })
}

async fn start_client() -> (InProcessAppServerClient, TempDir, TempDir) {
    let codex_home = tempfile::tempdir().expect("codex home");
    let workspace = tempfile::tempdir().expect("workspace");
    let config = Arc::new(
        ConfigBuilder::default()
            .codex_home(codex_home.path().to_path_buf())
            .build()
            .await
            .expect("test config"),
    );
    let client = InProcessAppServerClient::start(InProcessClientStartArgs {
        arg0_paths: Arg0DispatchPaths::default(),
        config,
        cli_overrides: Vec::new(),
        loader_overrides: LoaderOverrides::default(),
        strict_config: false,
        cloud_config_bundle: CloudConfigBundleLoader::default(),
        feedback: CodexFeedback::new(),
        log_db: None,
        state_db: None,
        environment_manager: Arc::new(EnvironmentManager::default_for_tests()),
        config_warnings: Vec::new(),
        session_source: SessionSource::VSCode,
        enable_codex_api_key_env: false,
        client_name: "provider-failure-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: 64,
        production_orchestration_runtime: None,
    })
    .await
    .expect("in-process client");
    (client, codex_home, workspace)
}

async fn collect_turn_events(client: &mut InProcessAppServerClient) -> Vec<ServerNotification> {
    let mut notifications = Vec::new();
    loop {
        let event = timeout(Duration::from_secs(10), client.next_event())
            .await
            .expect("turn event should arrive")
            .expect("client event stream should remain open");
        if let InProcessServerEvent::ServerNotification(notification) = event {
            let completed = matches!(notification, ServerNotification::TurnCompleted(_));
            notifications.push(notification);
            if completed {
                return notifications;
            }
        }
    }
}

async fn submit_turn(
    client: &mut InProcessAppServerClient,
    thread_id: &str,
    workspace: &Path,
    request_id: i64,
) -> Vec<ServerNotification> {
    client
        .request_typed::<codex_app_server_protocol::TurnStartResponse>(ClientRequest::TurnStart {
            request_id: RequestId::Integer(request_id),
            params: TurnStartParams {
                thread_id: thread_id.to_string(),
                input: vec![UserInput::Text {
                    text: "run provider failure test".to_string(),
                    text_elements: Vec::new(),
                }],
                cwd: Some(workspace.to_path_buf()),
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("turn start response");
    collect_turn_events(client).await
}

#[test]
fn explicit_syndrid_provider_failure_does_not_fallback_or_invoke_later_roles() {
    std::thread::Builder::new()
        .name("provider-failure-activation".to_string())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(16 * 1024 * 1024)
                .enable_all()
                .build()
                .expect("test runtime")
                .block_on(
                    explicit_syndrid_provider_failure_does_not_fallback_or_invoke_later_roles_inner(
                    ),
                );
        })
        .expect("provider-failure test thread")
        .join()
        .expect("provider-failure test thread should finish");
}

async fn explicit_syndrid_provider_failure_does_not_fallback_or_invoke_later_roles_inner() {
    let (mut client, _codex_home, workspace) = start_client().await;
    let thread: ThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                ephemeral: Some(true),
                cwd: Some(workspace.path().to_string_lossy().into_owned()),
                ..ThreadStartParams::default()
            },
        })
        .await
        .expect("thread start response");
    let state = Arc::new(Mutex::new(ProviderState {
        planner_failure_pending: true,
        ..ProviderState::default()
    }));
    let factory = runner_factory(Arc::clone(&state), workspace.path().to_path_buf());
    let runtime = TrustedProductionRuntimeBuilder::new(TrustedProductionRuntimeDependencies {
        session_id: thread.thread.id.clone(),
        runner_factory: Some(Arc::new(factory)),
        context_provider: Some(Arc::new(ObjectiveOnlyProductionTurnContext)),
    })
    .build(client.event_sender())
    .expect("trusted runtime");
    client
        .install_production_runtime(
            ProductionExecutionCapability::SyndridOrchestration,
            Some(Arc::new(runtime)),
        )
        .await
        .expect("runtime installation");

    let first = submit_turn(&mut client, &thread.thread.id, workspace.path(), 2).await;
    let first_completed = first
        .iter()
        .find_map(|notification| match notification {
            ServerNotification::TurnCompleted(notification) => Some(notification),
            _ => None,
        })
        .expect("first terminal notification");
    assert_eq!(first_completed.turn.status, TurnStatus::Failed);
    assert_eq!(
        first
            .iter()
            .filter(|notification| matches!(notification, ServerNotification::TurnStarted(_)))
            .count(),
        1
    );
    assert_eq!(
        first
            .iter()
            .filter(|notification| matches!(notification, ServerNotification::TurnCompleted(_)))
            .count(),
        1
    );
    let first_serialized = serde_json::to_string(&first).expect("notifications serialize");
    assert!(!first_serialized.contains(PRIVATE_PROVIDER_FAILURE_SENTINEL));

    let second = submit_turn(&mut client, &thread.thread.id, workspace.path(), 3).await;
    let second_completed = second
        .iter()
        .find_map(|notification| match notification {
            ServerNotification::TurnCompleted(notification) => Some(notification),
            _ => None,
        })
        .expect("second terminal notification");
    assert_eq!(second_completed.turn.status, TurnStatus::Completed);
    assert_eq!(
        second
            .iter()
            .filter(|notification| matches!(notification, ServerNotification::TurnCompleted(_)))
            .count(),
        1
    );

    let state = state.lock().expect("provider state lock");
    assert_eq!(state.invocations.len(), 3);
    assert_eq!(
        state
            .invocations
            .iter()
            .filter(|invocation| invocation.role == RoutingRole::Planner)
            .count(),
        2
    );
    assert_eq!(
        state
            .invocations
            .iter()
            .filter(|invocation| invocation.role == RoutingRole::Executor)
            .count(),
        1
    );
    assert_eq!(
        state
            .invocations
            .iter()
            .filter(|invocation| {
                matches!(
                    invocation.role,
                    RoutingRole::Verifier | RoutingRole::Repair | RoutingRole::Main
                )
            })
            .count(),
        0
    );
    for invocation in &state.invocations {
        let route = route_for(invocation.role);
        assert_eq!(invocation.provider, route.selection().provider_id);
        assert_eq!(invocation.connection, route.selection().connection_id);
        assert_eq!(invocation.model, route.selection().model_id);
        assert_eq!(invocation.effort, route.effort());
        assert_eq!(invocation.tool_count, 0);
    }
    assert_eq!(state.admissions.len(), 2);
    assert_ne!(state.admissions[0], state.admissions[1]);
    assert_eq!(
        state.private_failure_details,
        vec![PRIVATE_PROVIDER_FAILURE_SENTINEL.to_string()]
    );
    let first_admission = &state.admissions[0];
    let second_admission = &state.admissions[1];
    assert_eq!(
        state
            .invocations
            .iter()
            .filter(|invocation| invocation.turn_id == *first_admission)
            .map(|invocation| invocation.role)
            .collect::<Vec<_>>(),
        vec![RoutingRole::Planner]
    );
    assert_eq!(
        state
            .invocations
            .iter()
            .filter(|invocation| invocation.turn_id == *second_admission)
            .map(|invocation| invocation.role)
            .collect::<Vec<_>>(),
        vec![RoutingRole::Planner, RoutingRole::Executor]
    );
    drop(state);
    client.shutdown().await.expect("client shutdown");
}

#[test]
fn explicit_syndrid_without_runtime_stays_unavailable() {
    std::thread::Builder::new()
        .name("provider-unavailable-activation".to_string())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(16 * 1024 * 1024)
                .enable_all()
                .build()
                .expect("test runtime")
                .block_on(explicit_syndrid_without_runtime_stays_unavailable_inner());
        })
        .expect("provider-unavailable test thread")
        .join()
        .expect("provider-unavailable test thread should finish");
}

async fn explicit_syndrid_without_runtime_stays_unavailable_inner() {
    let (client, _codex_home, workspace) = start_client().await;
    let thread: ThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(10),
            params: ThreadStartParams {
                ephemeral: Some(true),
                cwd: Some(workspace.path().to_string_lossy().into_owned()),
                ..ThreadStartParams::default()
            },
        })
        .await
        .expect("thread start response");
    client
        .install_production_runtime(ProductionExecutionCapability::SyndridOrchestration, None)
        .await
        .expect("Syndrid authorization installation");
    let result = client
        .request(ClientRequest::TurnStart {
            request_id: RequestId::Integer(11),
            params: TurnStartParams {
                thread_id: thread.thread.id,
                input: vec![UserInput::Text {
                    text: "must remain unavailable".to_string(),
                    text_elements: Vec::new(),
                }],
                cwd: Some(workspace.path().to_path_buf()),
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("request transport");
    let error = result.expect_err("explicit Syndrid must remain unavailable");
    assert!(error.message.contains("unavailable"));
    assert!(!error.message.contains(PRIVATE_PROVIDER_FAILURE_SENTINEL));
    client.shutdown().await.expect("client shutdown");
}
