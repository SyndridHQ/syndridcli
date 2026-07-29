use super::spawn_observation_bridge;
use codex_core::ConnectionValidationStatus;
use codex_core::ExecutionModeSelection;
use codex_core::LiveOrchestrationCoordinator;
use codex_core::LiveOrchestrationRequest;
use codex_core::OpenRouterSetupCancellation as CancellationToken;
use codex_core::PlanningContract;
use codex_core::ProviderInvocationError;
use codex_core::ProviderInvocationRequest;
use codex_core::ProviderInvocationResult;
use codex_core::RoutingAssignment;
use codex_core::RoutingConnectionDirectory;
use codex_core::RoutingConnectionInfo;
use codex_core::RoutingProfile;
use codex_core::RoutingProfileId;
use codex_core::RoutingProfileRegistry;
use codex_core::RoutingRole;
use codex_core::SessionExecutionPolicyState;
use codex_core::SubagentFailurePolicy;
use codex_core::SubagentProvider;
use codex_core::SubagentToolPolicy;
use codex_core::VerificationContract;
use codex_core::observation_channel;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tokio::time::timeout;

use crate::AppServerEvent;

#[tokio::test]
async fn closed_observation_source_joins_cleanly() {
    let (sink, receiver) = observation_channel();
    let (events, mut received) = mpsc::channel(1);
    let bridge = spawn_observation_bridge(receiver, events);

    drop(sink);
    bridge.await.expect("observation bridge task");
    assert!(received.try_recv().is_err());
}

#[tokio::test]
async fn bridge_handle_has_explicit_join_ownership() {
    let (_sink, receiver) = observation_channel();
    let (events, received) = mpsc::channel(1);
    drop(received);
    let bridge = spawn_observation_bridge(receiver, events);

    bridge.abort();
    bridge.await.expect_err("aborted bridge");
}

#[tokio::test]
async fn coordinator_snapshot_becomes_an_app_server_event() {
    let (sink, receiver) = observation_channel();
    let (events, mut received) = mpsc::channel(8);
    let bridge = spawn_observation_bridge(receiver, events);
    let state = SessionExecutionPolicyState::new().expect("session state");
    let (profiles, connections) = routing();
    let request = LiveOrchestrationRequest {
        run_id: "bridge-run".to_string(),
        policy: Some(ExecutionModeSelection::Fast.resolve().expect("policy")),
        routing_profile_id: Some(RoutingProfileId::new("active").expect("profile id")),
        instruction: "bridge instruction".to_string(),
        context: None,
        tasks: Vec::new(),
        planning: PlanningContract::NotRequested,
        verification: VerificationContract::NotRequested,
        failure_policy: SubagentFailurePolicy::ContinueIndependent,
        repair_instruction: "repair".to_string(),
        approved_tool_policy: SubagentToolPolicy::empty(),
        cancellation: CancellationToken::new(),
        overall_timeout: None,
    };

    let outcome = LiveOrchestrationCoordinator::new(TestProvider, profiles, connections)
        .with_observation_sink(sink)
        .run(&state, request)
        .await
        .expect("coordinator run");
    let event = timeout(Duration::from_secs(1), received.recv())
        .await
        .expect("bridge event timeout")
        .expect("bridge event");
    let AppServerEvent::OrchestrationObservation(update) = event else {
        panic!("unexpected app-server event");
    };
    assert_eq!(update.snapshot, outcome.observation);
    assert_eq!(
        update.generation,
        outcome.observation.generation.value.expect("generation")
    );
    assert!(update.sequence > 0);
    drop(outcome);
    bridge.await.expect("observation bridge task");
}

struct TestProvider;

impl SubagentProvider for TestProvider {
    fn invoke(
        &self,
        request: ProviderInvocationRequest,
        _cancellation: CancellationToken,
    ) -> impl std::future::Future<Output = Result<ProviderInvocationResult, ProviderInvocationError>>
    + Send {
        std::future::ready(Ok(ProviderInvocationResult {
            provider: request.provider,
            model: request.model,
            text: "bounded result".to_string(),
            finish_reason: None,
            usage: None,
            request_id: None,
            tool_call: None,
        }))
    }
}

fn routing() -> (RoutingProfileRegistry, RoutingConnectionDirectory) {
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
    profiles.insert(profile).expect("profile insert");
    profiles.activate(&id).expect("profile activation");
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
