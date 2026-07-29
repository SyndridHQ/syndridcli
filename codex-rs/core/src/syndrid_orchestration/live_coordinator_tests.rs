use super::ConnectionValidationStatus;
use super::ExecutionModeSelection;
use super::LiveEvent;
use super::LiveOrchestrationCoordinator;
use super::LiveOrchestrationError;
use super::LiveOrchestrationRequest;
use super::LiveOrchestrationTerminal;
use super::LiveRepairResult;
use super::OrchestrationObservationSink;
use super::OrchestrationObservationSnapshot;
use super::PlannerTaskSpecification;
use super::PlanningContract;
use super::RoutingAssignment;
use super::RoutingConnectionDirectory;
use super::RoutingConnectionInfo;
use super::RoutingProfile;
use super::RoutingProfileId;
use super::RoutingProfileRegistry;
use super::RoutingRole;
use super::SubagentError;
use super::SubagentFailurePolicy;
use super::SubagentProvider;
use super::SubagentRepairError;
use super::SubagentRepairTerminal;
use super::SubagentToolPolicy;
use super::VerificationContract;
use super::VerificationDecision;
use super::live_coordinator_mapping::role_from_repair;
use super::live_coordinator_mapping::role_from_repair_error;
use super::live_coordinator_validation::validate_request;
use super::observation_channel;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const MAX_CONTEXT_BYTES: usize = 128 * 1024;
const MAX_INSTRUCTION_BYTES: usize = 32 * 1024;

#[derive(Clone, Default)]
struct RecordingObservationSink {
    snapshots: Arc<StdMutex<Vec<OrchestrationObservationSnapshot>>>,
}

impl OrchestrationObservationSink for RecordingObservationSink {
    fn publish(&self, snapshot: OrchestrationObservationSnapshot) {
        self.snapshots
            .lock()
            .expect("observation lock")
            .push(snapshot);
    }
}

fn request() -> LiveOrchestrationRequest {
    LiveOrchestrationRequest {
        run_id: "run-1".to_string(),
        policy: None,
        routing_profile_id: None,
        instruction: "bounded instruction".to_string(),
        context: None,
        tasks: Vec::new(),
        planning: PlanningContract::NotRequested,
        verification: VerificationContract::NotRequested,
        failure_policy: SubagentFailurePolicy::ContinueIndependent,
        repair_instruction: "repair".to_string(),
        approved_tool_policy: SubagentToolPolicy::empty(),
        cancellation: tokio_util::sync::CancellationToken::new(),
        overall_timeout: None,
    }
}

#[derive(Clone)]
struct CoordinatorProvider {
    calls: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    routes: Arc<Mutex<Vec<(String, String)>>>,
    responses: Arc<Mutex<VecDeque<String>>>,
    hold_provider: Option<String>,
    hold_instruction: Option<String>,
    fail_instruction: Option<String>,
    started: Arc<AtomicUsize>,
    started_notify: Arc<Notify>,
}

impl CoordinatorProvider {
    fn new() -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            routes: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(VecDeque::new())),
            hold_provider: None,
            hold_instruction: None,
            fail_instruction: None,
            started: Arc::new(AtomicUsize::new(0)),
            started_notify: Arc::new(Notify::new()),
        }
    }

    fn holding(provider: &str) -> Self {
        Self {
            hold_provider: Some(provider.to_string()),
            ..Self::new()
        }
    }

    fn holding_instruction(instruction: &str) -> Self {
        Self {
            hold_instruction: Some(instruction.to_string()),
            ..Self::new()
        }
    }

    fn failing_instruction(instruction: &str) -> Self {
        Self {
            fail_instruction: Some(instruction.to_string()),
            ..Self::new()
        }
    }

    async fn push_responses(&self, responses: impl IntoIterator<Item = &'static str>) {
        self.responses
            .lock()
            .await
            .extend(responses.into_iter().map(str::to_string));
    }
}

impl SubagentProvider for CoordinatorProvider {
    async fn invoke(
        &self,
        request: super::ProviderInvocationRequest,
        _cancellation: CancellationToken,
    ) -> Result<super::ProviderInvocationResult, super::ProviderInvocationError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.fetch_add(1, Ordering::SeqCst);
        self.started_notify.notify_waiters();
        let hold = self.hold_provider.as_deref() == Some(request.provider.as_str())
            || self
                .hold_instruction
                .as_deref()
                .is_some_and(|instruction| request.user.contains(instruction));
        self.routes
            .lock()
            .await
            .push((request.provider.clone(), request.model.clone()));
        if hold {
            self.active.fetch_add(1, Ordering::SeqCst);
            let result = tokio::select! {
                _ = _cancellation.cancelled() => Err(super::ProviderInvocationError::Cancelled),
                result = std::future::pending::<Result<super::ProviderInvocationResult, super::ProviderInvocationError>>() => result,
            };
            self.active.fetch_sub(1, Ordering::SeqCst);
            return result;
        }
        if self
            .fail_instruction
            .as_deref()
            .is_some_and(|instruction| request.user.contains(instruction))
        {
            return Err(super::ProviderInvocationError::ProviderRejected);
        }
        let text = self
            .responses
            .lock()
            .await
            .pop_front()
            .unwrap_or_else(|| "bounded result".to_string());
        Ok(super::ProviderInvocationResult {
            provider: request.provider,
            model: request.model,
            text,
            finish_reason: None,
            usage: None,
            request_id: None,
            tool_call: None,
        })
    }
}

fn coordinator_routing_with_repair() -> (RoutingProfileRegistry, RoutingConnectionDirectory) {
    let (mut profiles, mut connections) = coordinator_routing();
    let profile_id = RoutingProfileId::new("active").expect("profile id");
    profiles
        .get_mut(&profile_id)
        .expect("active profile")
        .assign(
            RoutingRole::Repair,
            RoutingAssignment {
                connection_id: "repair-connection".to_string(),
                provider_id: "openrouter".to_string(),
                model_id: "repair-model".to_string(),
                enabled: true,
                label: None,
            },
        )
        .expect("repair assignment");
    connections.insert(RoutingConnectionInfo {
        connection_id: "repair-connection".to_string(),
        provider_id: "openrouter".to_string(),
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        authentication_supported: true,
        models: Some(vec!["repair-model".to_string()]),
    });
    (profiles, connections)
}

fn coordinator_routing() -> (RoutingProfileRegistry, RoutingConnectionDirectory) {
    let id = RoutingProfileId::new("active").expect("profile id");
    let mut profile = RoutingProfile::new(id.clone(), "Active", 1).expect("profile");
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
    ] {
        profile
            .assign(
                role,
                RoutingAssignment {
                    connection_id: "connection".to_string(),
                    provider_id: "codex".to_string(),
                    model_id: "model".to_string(),
                    enabled: true,
                    label: None,
                },
            )
            .expect("assignment");
    }
    let mut profiles = RoutingProfileRegistry::default();
    profiles.insert(profile).expect("insert profile");
    profiles.activate(&id).expect("activate profile");
    let mut connections = RoutingConnectionDirectory::default();
    connections.insert(RoutingConnectionInfo {
        connection_id: "connection".to_string(),
        provider_id: "codex".to_string(),
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        authentication_supported: true,
        models: Some(vec!["model".to_string()]),
    });
    (profiles, connections)
}

fn task(task_id: &str) -> PlannerTaskSpecification {
    PlannerTaskSpecification {
        task_id: task_id.to_string(),
        instruction: format!("instruction-{task_id}"),
        context: None,
        tool_policy: SubagentToolPolicy::empty(),
        timeout: None,
    }
}

async fn wait_for_started(provider: &CoordinatorProvider, count: usize) {
    loop {
        let notified = provider.started_notify.notified();
        if provider.started.load(Ordering::SeqCst) >= count {
            return;
        }
        notified.await;
    }
}

#[test]
fn request_debug_does_not_include_instruction_contents() {
    let mut request = request();
    request.instruction = "sentinel-prompt".to_string();
    assert!(!format!("{request:?}").contains("sentinel-prompt"));
}

#[test]
fn coordinator_debug_errors_events_and_outcome_exclude_privacy_sentinels() {
    let sentinels = [
        "sentinel-prompt",
        "sentinel-context",
        "sentinel-credential",
        "sentinel-token",
        "sentinel-account-secret",
        "sentinel-provider-response",
        "sentinel-verifier-reason",
        "sentinel-repair-instruction",
        "sentinel-tool-output",
        "sentinel-hidden-reasoning",
    ];
    let mut request = request();
    request.instruction = sentinels[0].to_string();
    request.context = Some(sentinels[1].to_string());
    request.repair_instruction = sentinels[6].to_string();
    let request_debug = format!("{request:?}");
    assert!(
        sentinels
            .iter()
            .all(|sentinel| !request_debug.contains(sentinel))
    );
    let error_debug = format!("{:?}", LiveOrchestrationError::InvalidRequest);
    assert!(
        sentinels
            .iter()
            .all(|sentinel| !error_debug.contains(sentinel))
    );
    let events_debug = format!(
        "{:?}",
        [
            LiveEvent::RunPrepared,
            LiveEvent::RunTerminal(LiveOrchestrationTerminal::Failed)
        ]
    );
    assert!(
        sentinels
            .iter()
            .all(|sentinel| !events_debug.contains(sentinel))
    );
}

#[test]
fn request_bounds_are_explicit() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("policy");
    let mut request = request();
    request.instruction = "x".repeat(MAX_INSTRUCTION_BYTES + 1);
    assert_eq!(
        validate_request(&request, &policy),
        Err(LiveOrchestrationError::InvalidRequest)
    );
    request.instruction = "ok".to_string();
    request.context = Some("x".repeat(MAX_CONTEXT_BYTES + 1));
    assert_eq!(
        validate_request(&request, &policy),
        Err(LiveOrchestrationError::InvalidRequest)
    );
}

#[test]
fn duplicate_task_ids_are_rejected_before_execution() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("policy");
    let mut request = request();
    request.tasks = vec![
        PlannerTaskSpecification {
            task_id: "duplicate".to_string(),
            instruction: "one".to_string(),
            context: None,
            tool_policy: SubagentToolPolicy::empty(),
            timeout: None,
        },
        PlannerTaskSpecification {
            task_id: "duplicate".to_string(),
            instruction: "two".to_string(),
            context: None,
            tool_policy: SubagentToolPolicy::empty(),
            timeout: None,
        },
    ];
    assert_eq!(
        validate_request(&request, &policy),
        Err(LiveOrchestrationError::InvalidTaskIdentifiers)
    );
}

#[test]
fn empty_task_id_is_rejected_before_execution() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("policy");
    let mut request = request();
    request.tasks.push(PlannerTaskSpecification {
        task_id: String::new(),
        instruction: "task".to_string(),
        context: None,
        tool_policy: SubagentToolPolicy::empty(),
        timeout: None,
    });
    assert_eq!(
        validate_request(&request, &policy),
        Err(LiveOrchestrationError::InvalidTaskIdentifiers)
    );
}

#[test]
fn task_ceiling_is_rejected_before_execution() {
    let policy = ExecutionModeSelection::Fast.resolve().expect("policy");
    let mut request = request();
    request.tasks = (0..=policy.policy().max_subagents)
        .map(|index| PlannerTaskSpecification {
            task_id: format!("task-{index}"),
            instruction: "task".to_string(),
            context: None,
            tool_policy: SubagentToolPolicy::empty(),
            timeout: None,
        })
        .collect();
    assert_eq!(
        validate_request(&request, &policy),
        Err(LiveOrchestrationError::ExecutorTasksExceedPolicyCeiling)
    );
}

#[tokio::test]
async fn fast_flow_is_bounded_ordered_and_route_exact() {
    let provider = CoordinatorProvider::new();
    let calls = provider.calls.clone();
    let routes = provider.routes.clone();
    let (profiles, connections) = coordinator_routing();
    let profile_id = RoutingProfileId::new("active").expect("profile id");
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Fast.resolve().expect("policy"));
    request.routing_profile_id = Some(profile_id);
    request.tasks.push(PlannerTaskSpecification {
        task_id: "task-1".to_string(),
        instruction: "bounded task".to_string(),
        context: None,
        tool_policy: SubagentToolPolicy::empty(),
        timeout: None,
    });
    let outcome = LiveOrchestrationCoordinator::new(provider, profiles, connections)
        .run(&state, request)
        .await
        .expect("fast flow");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::Completed);
    assert!(outcome.synthesis_permitted);
    assert_eq!(
        outcome.observation.lifecycle.value,
        Some(super::SessionExecutionStatus::Completed)
    );
    assert_eq!(
        outcome.observation.stage.value,
        Some(super::OrchestrationObservationStage::Terminal)
    );
    assert_eq!(
        outcome.observation.generation.quality,
        super::ObservationQuality::Exact
    );
    assert_eq!(outcome.observation.provider.cached_input_tokens.value, None);
    assert_eq!(outcome.observation.cleanup_pending.value, Some(false));
    assert_eq!(outcome.peak_concurrency, 1);
    assert_eq!(outcome.provider_invocations, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        routes.lock().await.as_slice(),
        [("codex".to_string(), "model".to_string())]
    );
    assert_eq!(
        outcome
            .roles
            .iter()
            .map(|role| role.role)
            .collect::<Vec<_>>(),
        vec![
            RoutingRole::Planner,
            RoutingRole::Executor,
            RoutingRole::Verifier,
            RoutingRole::Repair,
        ]
    );
    assert_eq!(
        outcome.events,
        vec![
            LiveEvent::RunPrepared,
            LiveEvent::PolicyValidated,
            LiveEvent::RoleSkipped(
                RoutingRole::Planner,
                super::LiveRoleSkipReason::NotRequested
            ),
            LiveEvent::ExecutorBatchStarted,
            LiveEvent::RoleStarted(RoutingRole::Executor),
            LiveEvent::RoleSkipped(RoutingRole::Verifier, super::LiveRoleSkipReason::Disabled),
            LiveEvent::RoleSkipped(RoutingRole::Repair, super::LiveRoleSkipReason::Disabled),
            LiveEvent::RunTerminal(LiveOrchestrationTerminal::Completed),
        ]
    );
}

#[tokio::test]
async fn observation_sink_publishes_progress_and_final_cleanup_state() {
    let sink = RecordingObservationSink::default();
    let snapshots = sink.snapshots.clone();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut first_request = request();
    first_request.policy = Some(ExecutionModeSelection::Fast.resolve().expect("policy"));
    first_request.routing_profile_id = Some(RoutingProfileId::new("active").expect("profile id"));
    first_request.tasks.push(task("observe"));

    let outcome =
        LiveOrchestrationCoordinator::new(CoordinatorProvider::new(), profiles, connections)
            .with_observation_sink(sink)
            .run(&state, first_request)
            .await
            .expect("fast flow");

    let snapshots = snapshots.lock().expect("observation lock").clone();
    assert!(snapshots.len() >= 3);
    assert_eq!(snapshots.last(), Some(&outcome.observation));
    assert!(snapshots.windows(2).all(|pair| {
        pair[0].generation.value == pair[1].generation.value
            && pair[0].stage.value != Some(super::OrchestrationObservationStage::Idle)
    }));
    assert!(
        snapshots.iter().any(|snapshot| snapshot.stage.value
            == Some(super::OrchestrationObservationStage::Executing))
    );
    assert_eq!(outcome.observation.cleanup_pending.value, Some(false));
    assert!(!format!("{snapshots:?}").contains("bounded instruction"));
}

#[tokio::test]
async fn closed_observation_receiver_does_not_change_coordinator_result() {
    let (sink, receiver) = observation_channel();
    drop(receiver);
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Fast.resolve().expect("policy"));
    request.routing_profile_id = Some(RoutingProfileId::new("active").expect("profile id"));
    request.tasks.push(task("observe"));

    let outcome =
        LiveOrchestrationCoordinator::new(CoordinatorProvider::new(), profiles, connections)
            .with_observation_sink(sink)
            .run(&state, request)
            .await
            .expect("closed observation receiver must be harmless");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::Completed);
}

#[tokio::test]
async fn observation_delivery_keeps_generation_and_sequence_ordered() {
    let (sink, receiver) = observation_channel();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut first_request = request();
    first_request.policy = Some(ExecutionModeSelection::Fast.resolve().expect("policy"));
    first_request.routing_profile_id = Some(RoutingProfileId::new("active").expect("profile id"));
    first_request.tasks.push(task("observe"));

    let first =
        LiveOrchestrationCoordinator::new(CoordinatorProvider::new(), profiles, connections)
            .with_observation_sink(sink.clone())
            .run(&state, first_request)
            .await
            .expect("first flow");
    state.reset_to_idle().expect("reset state");
    let (profiles, connections) = coordinator_routing();
    let mut second_request = request();
    second_request.policy = Some(ExecutionModeSelection::Fast.resolve().expect("policy"));
    second_request.routing_profile_id = Some(RoutingProfileId::new("active").expect("profile id"));
    second_request.tasks.push(task("observe"));
    let second =
        LiveOrchestrationCoordinator::new(CoordinatorProvider::new(), profiles, connections)
            .with_observation_sink(sink)
            .run(&state, second_request)
            .await
            .expect("second flow");

    let update = receiver.latest().expect("latest observation");
    assert_eq!(update.snapshot, second.observation);
    assert_eq!(
        update.generation,
        second.observation.generation.value.expect("generation")
    );
    assert_ne!(
        first.observation.generation.value,
        second.observation.generation.value
    );
    assert!(update.sequence > 0);
}

#[tokio::test]
async fn terminal_observation_uses_frozen_failure_and_cleanup_state() {
    let provider = CoordinatorProvider::new();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Fast.resolve().expect("policy"));
    request.tasks = vec![task("observation")];
    let outcome = LiveOrchestrationCoordinator::new(provider, profiles, connections)
        .run(&state, request)
        .await
        .expect("observation flow");
    assert_eq!(outcome.observation.failure.accepted_cause.value, Some(None));
    assert_eq!(outcome.observation.cleanup.complete.value, Some(true));
    assert_eq!(
        outcome.observation.cleanup.active_provider_children.value,
        Some(0)
    );
    assert_eq!(
        outcome.observation.cleanup.active_tool_children.value,
        Some(0)
    );
    assert_eq!(
        outcome
            .observation
            .cleanup
            .unresolved_provider_reservations
            .value,
        Some(0)
    );
    assert_eq!(
        outcome
            .observation
            .cleanup
            .unresolved_tool_reservations
            .value,
        Some(0)
    );
}

#[tokio::test]
async fn verifier_rejection_repair_uses_exact_repair_route_once() {
    let provider = CoordinatorProvider::new();
    provider
        .push_responses(["bounded result", "REJECT\nneeds repair", "fixed result"])
        .await;
    let routes = provider.routes.clone();
    let (profiles, connections) = coordinator_routing_with_repair();
    let profile_id = RoutingProfileId::new("active").expect("profile id");
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Balanced.resolve().expect("policy"));
    request.routing_profile_id = Some(profile_id);
    request.verification = VerificationContract::Provider {
        instruction: "verify".to_string(),
    };
    request.tasks.push(PlannerTaskSpecification {
        task_id: "task-1".to_string(),
        instruction: "bounded task".to_string(),
        context: None,
        tool_policy: SubagentToolPolicy::empty(),
        timeout: None,
    });
    let outcome = LiveOrchestrationCoordinator::new(provider, profiles, connections)
        .run(&state, request)
        .await
        .expect("repair flow");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::Completed);
    assert!(outcome.synthesis_permitted);
    assert_eq!(outcome.provider_invocations, 3);
    assert_eq!(
        routes.lock().await.as_slice(),
        [
            ("codex".to_string(), "model".to_string()),
            ("codex".to_string(), "model".to_string()),
            ("openrouter".to_string(), "repair-model".to_string()),
        ]
    );
    assert_eq!(
        outcome.roles.last().and_then(|role| role.repair_result),
        Some(super::LiveRepairResult::RepairSucceeded)
    );
    assert_eq!(
        outcome
            .roles
            .iter()
            .map(|role| role.role)
            .collect::<Vec<_>>(),
        vec![
            RoutingRole::Planner,
            RoutingRole::Executor,
            RoutingRole::Verifier,
            RoutingRole::Repair,
        ]
    );
}

#[tokio::test]
async fn balanced_flow_orders_planner_executor_verifier_and_repair() {
    let provider = CoordinatorProvider::new();
    provider
        .push_responses(["plan", "executor-one", "executor-two", "ACCEPT"])
        .await;
    let calls = provider.calls.clone();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Balanced.resolve().expect("policy"));
    request.planning = PlanningContract::Required {
        instruction: "plan".to_string(),
    };
    request.verification = VerificationContract::Provider {
        instruction: "verify".to_string(),
    };
    request.tasks = vec![task("first"), task("second")];
    let outcome = LiveOrchestrationCoordinator::new(provider, profiles, connections)
        .run(&state, request)
        .await
        .expect("balanced flow");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::Completed);
    assert!(outcome.synthesis_permitted);
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert!(outcome.peak_concurrency <= 2);
    assert_eq!(
        outcome
            .roles
            .iter()
            .map(|role| role.role)
            .collect::<Vec<_>>(),
        vec![
            RoutingRole::Planner,
            RoutingRole::Executor,
            RoutingRole::Verifier,
            RoutingRole::Repair,
        ]
    );
    assert_eq!(outcome.roles[1].task_ids, vec!["first", "second"]);
    assert_eq!(
        outcome.events,
        vec![
            LiveEvent::RunPrepared,
            LiveEvent::PolicyValidated,
            LiveEvent::RoleStarted(RoutingRole::Planner),
            LiveEvent::ExecutorBatchStarted,
            LiveEvent::RoleStarted(RoutingRole::Executor),
            LiveEvent::RoleStarted(RoutingRole::Verifier),
            LiveEvent::VerifierDecision,
            LiveEvent::RoleSkipped(
                RoutingRole::Repair,
                super::LiveRoleSkipReason::NoEligibleRepair
            ),
            LiveEvent::RunTerminal(LiveOrchestrationTerminal::Completed),
        ]
    );
}

#[tokio::test]
async fn balanced_verifier_rejection_without_repair_is_terminal() {
    let provider = CoordinatorProvider::new();
    provider
        .push_responses(["bounded result", "REJECT\ninvalid"])
        .await;
    let calls = provider.calls.clone();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Fast.resolve().expect("policy"));
    request.verification = VerificationContract::Decision(VerificationDecision::Rejected {
        category: super::SubagentRepairFailureCategory::VerifierRejected,
        reason: "invalid".to_string(),
        repair_instruction: "repair".to_string(),
    });
    request.tasks = vec![task("one")];
    let result = LiveOrchestrationCoordinator::new(provider, profiles, connections)
        .run(&state, request)
        .await;
    assert_eq!(
        result,
        Err(LiveOrchestrationError::DisabledRequiredRole(
            RoutingRole::Verifier
        ))
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn usage_saver_is_single_bounded_executor_flow() {
    let provider = CoordinatorProvider::new();
    let calls = provider.calls.clone();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    let policy = ExecutionModeSelection::UsageSaver
        .resolve()
        .expect("policy");
    request.policy = Some(policy.clone());
    request.tasks = vec![task("one")];
    let outcome = LiveOrchestrationCoordinator::new(provider, profiles, connections)
        .run(&state, request)
        .await
        .expect("usage saver flow");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::Completed);
    assert!(outcome.synthesis_permitted);
    assert_eq!(outcome.peak_concurrency, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(outcome.budget_exhaustion_category, None);
    assert_eq!(outcome.budget.provider_started, 1);
    assert_eq!(outcome.budget.provider_completed, 1);
    assert_eq!(outcome.budget.provider_reserved, 0);
    assert_eq!(outcome.budget.executor_tasks_admitted, 1);
    assert!(outcome.budget.terminal);
    assert!(outcome.provider_invocations <= policy.policy().max_provider_invocations);
    assert!(outcome.tool_calls <= policy.policy().max_tool_calls);
    assert!(outcome.resolved_policy.output_budget_tokens <= 1_000);
    assert_eq!(
        outcome.roles[0].skip_reason,
        Some(super::LiveRoleSkipReason::NotRequested)
    );
    assert_eq!(
        outcome.roles[2].skip_reason,
        Some(super::LiveRoleSkipReason::Disabled)
    );
    assert_eq!(
        outcome.roles[3].skip_reason,
        Some(super::LiveRoleSkipReason::Disabled)
    );
}

#[test]
fn usage_saver_rejects_second_task_before_provider_invocation() {
    let policy = ExecutionModeSelection::UsageSaver
        .resolve()
        .expect("policy");
    let mut request = request();
    request.tasks = vec![task("one"), task("two")];
    assert_eq!(
        validate_request(&request, &policy),
        Err(LiveOrchestrationError::ExecutorTasksExceedPolicyCeiling)
    );
}

#[tokio::test]
async fn deep_flow_validates_required_roles_and_preserves_repair_cap() {
    let provider = CoordinatorProvider::new();
    provider
        .push_responses(["plan", "executor", "ACCEPT"])
        .await;
    let calls = provider.calls.clone();
    let (profiles, connections) = coordinator_routing_with_repair();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    let policy = ExecutionModeSelection::Deep.resolve().expect("policy");
    request.policy = Some(policy.clone());
    request.planning = PlanningContract::Required {
        instruction: "plan".to_string(),
    };
    request.verification = VerificationContract::Provider {
        instruction: "verify".to_string(),
    };
    request.tasks = vec![task("deep")];
    let outcome = LiveOrchestrationCoordinator::new(provider, profiles, connections)
        .run(&state, request)
        .await
        .expect("deep flow");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::Completed);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert!(outcome.peak_concurrency <= policy.policy().max_concurrency);
    assert!(outcome.roles[1].task_ids == vec!["deep"]);
    assert_eq!(
        outcome.roles.last().and_then(|role| role.repair_result),
        None
    );
    assert_eq!(
        outcome.events[2],
        LiveEvent::RoleStarted(RoutingRole::Planner)
    );
    assert_eq!(outcome.events[3], LiveEvent::ExecutorBatchStarted);
    assert_eq!(
        outcome.events[5],
        LiveEvent::RoleStarted(RoutingRole::Verifier)
    );
}

#[tokio::test]
async fn deep_missing_planner_route_fails_before_provider_invocation() {
    let provider = CoordinatorProvider::new();
    let calls = provider.calls.clone();
    let (mut profiles, connections) = coordinator_routing_with_repair();
    let profile_id = RoutingProfileId::new("active").expect("profile id");
    profiles
        .get_mut(&profile_id)
        .expect("profile")
        .assignments
        .remove(&RoutingRole::Planner);
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Deep.resolve().expect("policy"));
    request.planning = PlanningContract::Required {
        instruction: "plan".to_string(),
    };
    request.tasks = vec![task("deep")];
    let result = LiveOrchestrationCoordinator::new(provider, profiles, connections)
        .run(&state, request)
        .await;
    assert_eq!(
        result,
        Err(LiveOrchestrationError::MissingRequiredRoleRoute(
            RoutingRole::Planner
        ))
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancellation_before_execution_makes_zero_provider_calls() {
    let provider = CoordinatorProvider::new();
    let calls = provider.calls.clone();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Fast.resolve().expect("policy"));
    request.tasks = vec![task("cancelled")];
    request.cancellation.cancel();
    let outcome = LiveOrchestrationCoordinator::new(provider, profiles, connections)
        .run(&state, request)
        .await
        .expect("cancelled outcome");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::Cancelled);
    assert!(outcome.cancelled);
    assert_eq!(
        outcome.failure.as_ref().map(|failure| failure.kind),
        Some(super::OrchestrationFailureKind::UserCancelled)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn overall_timeout_during_executor_awaits_provider_cleanup() {
    let provider = CoordinatorProvider::holding("codex");
    let calls = provider.calls.clone();
    let active = provider.active.clone();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Fast.resolve().expect("policy"));
    request.overall_timeout = Some(std::time::Duration::from_millis(10));
    request.tasks = vec![task("timeout")];
    let outcome = LiveOrchestrationCoordinator::new(provider, profiles, connections)
        .run(&state, request)
        .await
        .expect("timeout outcome");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::TimedOut);
    assert!(outcome.timed_out);
    assert_eq!(
        outcome.failure.as_ref().map(|failure| failure.kind),
        Some(super::OrchestrationFailureKind::TotalTimedOut)
    );
    for _ in 0..16 {
        if active.load(Ordering::SeqCst) == 0 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        state.status().expect("status"),
        super::SessionExecutionStatus::TimedOut
    );
}

#[tokio::test]
async fn overall_timeout_during_repair_awaits_o6d_cleanup() {
    let provider = CoordinatorProvider::holding("openrouter");
    provider
        .push_responses(["bounded result", "REJECT\nneeds repair"])
        .await;
    let active = provider.active.clone();
    let calls = provider.calls.clone();
    let (profiles, connections) = coordinator_routing_with_repair();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Balanced.resolve().expect("policy"));
    request.routing_profile_id = Some(RoutingProfileId::new("active").expect("profile id"));
    request.verification = VerificationContract::Provider {
        instruction: "verify".to_string(),
    };
    request.overall_timeout = Some(std::time::Duration::from_millis(10));
    request.tasks = vec![task("repair-timeout")];
    let outcome = LiveOrchestrationCoordinator::new(provider, profiles, connections)
        .run(&state, request)
        .await
        .expect("repair timeout outcome");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::TimedOut);
    assert!(outcome.timed_out);
    assert_eq!(
        outcome.failure.as_ref().map(|failure| failure.kind),
        Some(super::OrchestrationFailureKind::TotalTimedOut)
    );
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn planner_cancellation_is_awaited_before_terminal_state() {
    let provider = CoordinatorProvider::holding_instruction("planner-cancel");
    let active = provider.active.clone();
    let calls = provider.calls.clone();
    let started = provider.started_notify.clone();
    let cancellation = CancellationToken::new();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Balanced.resolve().expect("policy"));
    request.planning = PlanningContract::Required {
        instruction: "planner-cancel".to_string(),
    };
    request.tasks = vec![task("never-started")];
    request.cancellation = cancellation.clone();
    let coordinator = LiveOrchestrationCoordinator::new(provider.clone(), profiles, connections);
    let run = tokio::spawn(async move { coordinator.run(&state, request).await });
    loop {
        let notified = started.notified();
        if provider.started.load(Ordering::SeqCst) >= 1 {
            break;
        }
        notified.await;
    }
    cancellation.cancel();
    let outcome = run
        .await
        .expect("planner join")
        .expect("planner cancellation");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::Cancelled);
    assert!(!outcome.synthesis_permitted);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        outcome.events.last(),
        Some(&LiveEvent::RunTerminal(
            LiveOrchestrationTerminal::Cancelled
        ))
    );
    assert!(
        !outcome
            .events
            .contains(&LiveEvent::RoleStarted(RoutingRole::Executor))
    );
}

#[tokio::test]
async fn planner_timeout_is_awaited_before_timed_out_terminal_state() {
    let provider = CoordinatorProvider::holding_instruction("planner-timeout");
    let active = provider.active.clone();
    let calls = provider.calls.clone();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Balanced.resolve().expect("policy"));
    request.planning = PlanningContract::Required {
        instruction: "planner-timeout".to_string(),
    };
    request.tasks = vec![task("never-started")];
    request.overall_timeout = Some(std::time::Duration::from_millis(10));
    let outcome = LiveOrchestrationCoordinator::new(provider.clone(), profiles, connections)
        .run(&state, request)
        .await
        .expect("planner timeout");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::TimedOut);
    assert!(outcome.timed_out);
    assert!(!outcome.synthesis_permitted);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(!outcome.events.contains(&LiveEvent::ExecutorBatchStarted));
    assert_eq!(
        outcome.events.last(),
        Some(&LiveEvent::RunTerminal(LiveOrchestrationTerminal::TimedOut))
    );
}

#[tokio::test]
async fn verifier_cancellation_is_awaited_without_repair() {
    let provider = CoordinatorProvider::holding_instruction("verifier-cancel");
    let active = provider.active.clone();
    let cancellation = CancellationToken::new();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Balanced.resolve().expect("policy"));
    request.verification = VerificationContract::Provider {
        instruction: "verifier-cancel".to_string(),
    };
    request.tasks = vec![task("executor")];
    request.cancellation = cancellation.clone();
    let coordinator = LiveOrchestrationCoordinator::new(provider.clone(), profiles, connections);
    let run = tokio::spawn(async move { coordinator.run(&state, request).await });
    wait_for_started(&provider, 2).await;
    cancellation.cancel();
    let outcome = run
        .await
        .expect("verifier join")
        .expect("verifier cancellation");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::Cancelled);
    assert!(!outcome.synthesis_permitted);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert!(!outcome.events.contains(&LiveEvent::RepairStarted));
    assert_eq!(
        outcome.events.last(),
        Some(&LiveEvent::RunTerminal(
            LiveOrchestrationTerminal::Cancelled
        ))
    );
}

#[tokio::test]
async fn verifier_timeout_is_awaited_without_repair() {
    let provider = CoordinatorProvider::holding_instruction("verifier-timeout");
    let active = provider.active.clone();
    let calls = provider.calls.clone();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Balanced.resolve().expect("policy"));
    request.verification = VerificationContract::Provider {
        instruction: "verifier-timeout".to_string(),
    };
    request.tasks = vec![task("executor")];
    request.overall_timeout = Some(std::time::Duration::from_millis(10));
    let outcome = LiveOrchestrationCoordinator::new(provider.clone(), profiles, connections)
        .run(&state, request)
        .await
        .expect("verifier timeout");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::TimedOut);
    assert!(outcome.timed_out);
    assert!(!outcome.synthesis_permitted);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(
        outcome
            .events
            .contains(&LiveEvent::RoleStarted(RoutingRole::Verifier))
    );
    assert!(!outcome.events.contains(&LiveEvent::RepairStarted));
    assert_eq!(
        outcome.events.last(),
        Some(&LiveEvent::RunTerminal(LiveOrchestrationTerminal::TimedOut))
    );
}

#[tokio::test]
async fn continue_independent_preserves_success_after_executor_failure() {
    let provider = CoordinatorProvider::failing_instruction("fail-task");
    let calls = provider.calls.clone();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    request.policy = Some(ExecutionModeSelection::Balanced.resolve().expect("policy"));
    request.tasks = vec![task("fail-task"), task("success-task")];
    let outcome = LiveOrchestrationCoordinator::new(provider, profiles, connections)
        .run(&state, request)
        .await
        .expect("continue independent outcome");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::Failed);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(outcome.provider_invocations, 2);
    assert_eq!(outcome.roles[1].task_ids, vec!["fail-task", "success-task"]);
    assert!(!outcome.synthesis_permitted);
}

#[tokio::test]
async fn cancel_remaining_stops_queued_executor_work_and_awaits_running_work() {
    let provider = CoordinatorProvider {
        fail_instruction: Some("fail-task".to_string()),
        hold_instruction: Some("running-task".to_string()),
        ..CoordinatorProvider::new()
    };
    let active = provider.active.clone();
    let calls = provider.calls.clone();
    let (profiles, connections) = coordinator_routing();
    let state = super::SessionExecutionPolicyState::new().expect("state");
    let mut request = request();
    let mut policy = ExecutionModeSelection::Deep
        .resolve()
        .expect("policy")
        .policy()
        .clone();
    policy.max_concurrency = 2;
    request.policy = Some(
        ExecutionModeSelection::custom(policy)
            .resolve()
            .expect("bounded custom policy"),
    );
    request.failure_policy = SubagentFailurePolicy::CancelRemaining;
    request.tasks = vec![task("fail-task"), task("running-task"), task("queued-task")];
    let outcome = LiveOrchestrationCoordinator::new(provider, profiles, connections)
        .run(&state, request)
        .await
        .expect("cancel remaining outcome");
    assert_eq!(outcome.terminal, LiveOrchestrationTerminal::Failed);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(
        outcome.roles[1].task_ids,
        vec!["fail-task", "running-task", "queued-task"]
    );
    assert!(!outcome.synthesis_permitted);
}

#[test]
fn repair_terminal_mapping_is_typed_and_deterministic() {
    let cases = [
        (
            SubagentRepairTerminal::InitialSucceeded,
            LiveRepairResult::RepairSucceeded,
        ),
        (
            SubagentRepairTerminal::RepairSucceeded,
            LiveRepairResult::RepairSucceeded,
        ),
        (
            SubagentRepairTerminal::RepairDisabled,
            LiveRepairResult::RepairDisabled,
        ),
        (
            SubagentRepairTerminal::NotEligible,
            LiveRepairResult::NotEligible,
        ),
        (
            SubagentRepairTerminal::InitialFailed,
            LiveRepairResult::RepairFailed,
        ),
        (
            SubagentRepairTerminal::RepairFailed,
            LiveRepairResult::RepairFailed,
        ),
        (
            SubagentRepairTerminal::RepairTimedOut,
            LiveRepairResult::RepairTimedOut,
        ),
        (
            SubagentRepairTerminal::Cancelled,
            LiveRepairResult::Cancelled,
        ),
        (
            SubagentRepairTerminal::BudgetExhausted,
            LiveRepairResult::BudgetExhausted,
        ),
    ];
    for (terminal, expected) in cases {
        let role = role_from_repair(&super::SubagentRepairOutcome {
            task_id: "repair".to_string(),
            terminal,
            attempts: Vec::new(),
        });
        assert_eq!(role.repair_result, Some(expected));
        assert_eq!(role.repair_attempts, 0);
    }
}

#[test]
fn repair_error_mapping_is_typed_without_message_inference() {
    let cases = [
        (
            SubagentRepairError::PolicyInvalid,
            LiveRepairResult::PolicyInvalid,
        ),
        (
            SubagentRepairError::InitialValidationFailed(SubagentError::InvalidTaskId),
            LiveRepairResult::InitialValidationFailed,
        ),
        (
            SubagentRepairError::RouteMismatch,
            LiveRepairResult::RouteMismatch,
        ),
        (
            SubagentRepairError::BudgetExhausted,
            LiveRepairResult::BudgetExhausted,
        ),
        (
            SubagentRepairError::CancelledBeforeRepair,
            LiveRepairResult::Cancelled,
        ),
        (
            SubagentRepairError::BatchInvalid,
            LiveRepairResult::BatchInvalid,
        ),
        (
            SubagentRepairError::JoinFailure,
            LiveRepairResult::RepairFailed,
        ),
    ];
    for (error, expected) in cases {
        let role = role_from_repair_error(error);
        assert_eq!(role.repair_result, Some(expected));
        assert_eq!(role.repair_attempts, 0);
    }
}

#[test]
fn repair_errors_preserve_terminal_and_coordinator_error_categories() {
    let cases = [
        (
            SubagentRepairError::PolicyInvalid,
            LiveOrchestrationTerminal::Failed,
            LiveOrchestrationError::RepairPolicyInvalid,
        ),
        (
            SubagentRepairError::InitialValidationFailed(SubagentError::InvalidTaskId),
            LiveOrchestrationTerminal::Failed,
            LiveOrchestrationError::RepairInitialValidationFailed,
        ),
        (
            SubagentRepairError::RouteMismatch,
            LiveOrchestrationTerminal::Failed,
            LiveOrchestrationError::RepairRouteMismatch,
        ),
        (
            SubagentRepairError::BudgetExhausted,
            LiveOrchestrationTerminal::BudgetExhausted,
            LiveOrchestrationError::BudgetExhaustion,
        ),
        (
            SubagentRepairError::CancelledBeforeRepair,
            LiveOrchestrationTerminal::Cancelled,
            LiveOrchestrationError::Cancellation,
        ),
        (
            SubagentRepairError::BatchInvalid,
            LiveOrchestrationTerminal::Failed,
            LiveOrchestrationError::RepairBatchInvalid,
        ),
        (
            SubagentRepairError::JoinFailure,
            LiveOrchestrationTerminal::Failed,
            LiveOrchestrationError::RepairJoinFailure,
        ),
    ];
    for (error, terminal, coordinator_error) in cases {
        assert_eq!(
            super::live_coordinator_mapping::repair_error_terminal(error),
            (terminal, coordinator_error)
        );
    }
}

#[tokio::test]
async fn deep_required_route_matrix_rejects_before_provider_calls() {
    let cases = [
        (
            RoutingRole::Main,
            LiveOrchestrationError::MissingRequiredRoleRoute(RoutingRole::Main),
        ),
        (
            RoutingRole::Planner,
            LiveOrchestrationError::MissingRequiredRoleRoute(RoutingRole::Planner),
        ),
        (
            RoutingRole::Executor,
            LiveOrchestrationError::MissingRequiredRoleRoute(RoutingRole::Executor),
        ),
        (
            RoutingRole::Verifier,
            LiveOrchestrationError::MissingRequiredRoleRoute(RoutingRole::Verifier),
        ),
        (
            RoutingRole::Repair,
            LiveOrchestrationError::MissingRequiredRoleRoute(RoutingRole::Repair),
        ),
    ];
    for (missing_role, expected) in cases {
        let provider = CoordinatorProvider::new();
        let calls = provider.calls.clone();
        let (mut profiles, connections) = coordinator_routing_with_repair();
        let profile_id = RoutingProfileId::new("active").expect("profile id");
        profiles
            .get_mut(&profile_id)
            .expect("profile")
            .assignments
            .remove(&missing_role);
        let state = super::SessionExecutionPolicyState::new().expect("state");
        let mut request = request();
        request.policy = Some(ExecutionModeSelection::Deep.resolve().expect("policy"));
        request.tasks = vec![task("deep-route")];
        if missing_role == RoutingRole::Planner {
            request.planning = PlanningContract::Required {
                instruction: "plan".to_string(),
            };
        }
        if matches!(missing_role, RoutingRole::Verifier | RoutingRole::Repair) {
            request.verification = VerificationContract::Provider {
                instruction: "verify".to_string(),
            };
        }
        let result = LiveOrchestrationCoordinator::new(provider, profiles, connections)
            .run(&state, request)
            .await;
        assert_eq!(result, Err(expected));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
