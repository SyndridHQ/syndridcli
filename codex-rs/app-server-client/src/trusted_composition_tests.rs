use super::*;
use codex_app_server::ObjectiveOnlyProductionTurnContext;
use codex_core::ConnectionValidationStatus;
use codex_core::ExecutionModeSelection;
use codex_core::RoleCapabilityConfiguration;
use codex_core::RoleCapabilityDeclaration;
use codex_core::RoleCapabilityValidationContext;
use codex_core::RoutingAssignment;
use codex_core::RoutingConnectionInfo;
use codex_core::RoutingRole;
use codex_core::ValidatedRoleCapabilitySet;
use pretty_assertions::assert_eq;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;

struct FakeRouting {
    snapshot: TrustedRoutingSnapshot,
}

impl TrustedRoutingAuthority for FakeRouting {
    fn snapshot(&self) -> Result<TrustedRoutingSnapshot, TrustedCompositionSnapshotError> {
        Ok(self.snapshot.clone())
    }

    fn snapshot_for_profile(
        &self,
        profile: &codex_core::RoutingProfile,
    ) -> Result<TrustedRoutingSnapshot, TrustedCompositionSnapshotError> {
        TrustedRoutingSnapshot::from_profile(profile, &self.snapshot.connections)
    }
}

struct FakeProviders {
    calls: Arc<AtomicUsize>,
}

impl TrustedProductionProviderAuthority for FakeProviders {
    fn validate_routes(
        &self,
        _routing: &TrustedRoutingSnapshot,
    ) -> Result<(), TrustedCompositionSnapshotError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn construction_snapshot(
        &self,
        _routing: &TrustedRoutingSnapshot,
        _policy: &codex_core::ResolvedExecutionPolicy,
    ) -> Result<ProductionProviderConstructionSnapshot, TrustedCompositionSnapshotError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ProductionProviderConstructionSnapshot::new(
            std::collections::BTreeMap::new(),
        ))
    }
}

struct FakeTools {
    calls: Arc<AtomicUsize>,
    snapshot: TrustedApprovedToolSnapshot,
}

impl TrustedApprovedToolAuthority for FakeTools {
    fn snapshot(
        &self,
        _workspace_root: &Path,
    ) -> Result<TrustedApprovedToolSnapshot, TrustedCompositionSnapshotError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.snapshot.clone())
    }
}

fn no_tool_snapshot() -> TrustedApprovedToolSnapshot {
    let policy = ExecutionModeSelection::Balanced.resolve().expect("policy");
    let configuration = RoleCapabilityConfiguration::new(
        [
            RoutingRole::Planner,
            RoutingRole::Executor,
            RoutingRole::Verifier,
            RoutingRole::Repair,
        ]
        .into_iter()
        .map(RoleCapabilityDeclaration::no_tools)
        .collect(),
    );
    let context = RoleCapabilityValidationContext::new(
        PathBuf::from("/workspace"),
        BTreeSet::new(),
        false,
        false,
    );
    let capabilities: ValidatedRoleCapabilitySet =
        codex_core::validate_role_capabilities(&configuration, &policy, &context)
            .expect("capabilities");
    TrustedApprovedToolSnapshot::from_validated(capabilities)
}

fn routing_snapshot() -> TrustedRoutingSnapshot {
    let profile_id = codex_core::RoutingProfileId::new("trusted-test").expect("profile ID");
    let mut profile =
        codex_core::RoutingProfile::new(profile_id.clone(), "Trusted test", 1).expect("profile");
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
                    connection_id: "connection-1".to_string(),
                    provider_id: "codex".to_string(),
                    model_id: "model-1".to_string(),
                    enabled: true,
                    label: None,
                    pool_id: None,
                },
            )
            .expect("assignment");
    }
    let mut connections = codex_core::RoutingConnectionDirectory::default();
    connections.insert(RoutingConnectionInfo {
        connection_id: "connection-1".to_string(),
        provider_id: "codex".to_string(),
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        authentication_supported: true,
        models: Some(vec!["model-1".to_string()]),
    });
    TrustedRoutingSnapshot {
        profile_id,
        profile,
        connections,
        pools: None,
    }
}

fn dependencies(
    policy_state: Option<Arc<codex_core::SessionExecutionPolicyState>>,
    routing_authority: Option<Arc<dyn TrustedRoutingAuthority>>,
    provider_authority: Option<Arc<dyn TrustedProductionProviderAuthority>>,
    tool_authority: Option<Arc<dyn TrustedApprovedToolAuthority>>,
    context_provider: Option<Arc<dyn codex_app_server::ProductionTurnContextProvider>>,
) -> TrustedSyndridCompositionDependencies {
    let (event_sender, _event_receiver) = mpsc::channel(4);
    TrustedSyndridCompositionDependencies {
        session_id: "session-1".to_string(),
        workspace_root: PathBuf::from("/workspace"),
        policy_state,
        routing_authority,
        provider_authority,
        tool_authority,
        context_provider,
        event_sender,
    }
}

fn valid_source() -> (
    TrustedSyndridCompositionSource,
    Arc<AtomicUsize>,
    Arc<AtomicUsize>,
) {
    let policy = Arc::new(
        codex_core::SessionExecutionPolicyState::with_selection(
            ExecutionModeSelection::Balanced,
            codex_core::SessionPolicySource::Default,
        )
        .expect("policy"),
    );
    let provider_calls = Arc::new(AtomicUsize::new(0));
    let tool_calls = Arc::new(AtomicUsize::new(0));
    let dependencies = dependencies(
        Some(policy),
        Some(Arc::new(FakeRouting {
            snapshot: routing_snapshot(),
        })),
        Some(Arc::new(FakeProviders {
            calls: Arc::clone(&provider_calls),
        })),
        Some(Arc::new(FakeTools {
            calls: Arc::clone(&tool_calls),
            snapshot: no_tool_snapshot(),
        })),
        Some(Arc::new(ObjectiveOnlyProductionTurnContext)),
    );
    (
        TrustedSyndridCompositionSource::new(dependencies).expect("source"),
        provider_calls,
        tool_calls,
    )
}

fn request() -> TrustedCompositionSnapshotRequest {
    TrustedCompositionSnapshotRequest {
        session_id: "session-1".to_string(),
        workspace_root: PathBuf::from("/workspace"),
    }
}

#[test]
fn captures_one_immutable_session_snapshot() {
    let (source, provider_calls, tool_calls) = valid_source();
    let snapshot = source.snapshot(request()).expect("snapshot");
    let clone = snapshot.clone();
    assert_eq!(snapshot.session_id, clone.session_id);
    assert_eq!(snapshot.routing, clone.routing);
    assert_eq!(snapshot.workspace_root, clone.workspace_root);
    assert_eq!(snapshot.approved_tools.role_capabilities.roles().count(), 4);
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
    assert_eq!(tool_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn candidate_snapshot_uses_candidate_strategy_and_preset_without_publishing() {
    let (source, provider_calls, tool_calls) = valid_source();
    let candidate = codex_core::SessionExecutionPolicyState::with_strategy_selection(
        codex_core::OrchestrationMode::Manual,
        ExecutionModeSelection::Fast,
        codex_core::SessionPolicySource::SessionOverride,
    )
    .expect("candidate policy");

    let snapshot = source
        .snapshot_with_policy_state(request(), &candidate)
        .expect("candidate snapshot");

    assert_eq!(snapshot.strategy, codex_core::OrchestrationMode::Manual);
    assert_eq!(
        snapshot.policy.selected_mode(),
        &ExecutionModeSelection::Fast
    );
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
    assert_eq!(tool_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn candidate_snapshot_uses_exact_routing_without_publishing() {
    let (source, provider_calls, tool_calls) = valid_source();
    let profile_id = codex_core::RoutingProfileId::new("candidate-routing").expect("profile ID");
    let mut candidate =
        codex_core::RoutingProfile::new(profile_id, "Candidate", 1).expect("profile");
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ] {
        candidate
            .assign(
                role,
                RoutingAssignment {
                    connection_id: "connection-1".to_string(),
                    provider_id: "codex".to_string(),
                    model_id: "model-1".to_string(),
                    enabled: true,
                    label: Some("exact-candidate".to_string()),
                    pool_id: None,
                },
            )
            .expect("assignment");
    }
    let policy = codex_core::SessionExecutionPolicyState::with_selection(
        ExecutionModeSelection::Balanced,
        codex_core::SessionPolicySource::Default,
    )
    .expect("policy");
    let snapshot = source
        .snapshot_with_policy_and_routing(request(), &policy, &candidate)
        .expect("candidate routing snapshot");

    assert_eq!(snapshot.routing.profile, candidate);
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
    assert_eq!(tool_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn rejects_missing_authorities_without_fallbacks() {
    let dependencies = dependencies(None, None, None, None, None);
    let source = TrustedSyndridCompositionSource::new(dependencies).expect("source");
    assert!(matches!(
        source.snapshot(request()),
        Err(TrustedCompositionSnapshotError::PolicyUnavailable)
    ));
}

#[test]
fn rejects_cross_session_snapshot_requests() {
    let (source, _, _) = valid_source();
    let mut request = request();
    request.session_id = "session-2".to_string();
    assert!(matches!(
        source.snapshot(request),
        Err(TrustedCompositionSnapshotError::SessionMismatch)
    ));
}

#[test]
fn source_and_snapshot_debug_output_is_redacted() {
    let (source, _, _) = valid_source();
    let snapshot = source.snapshot(request()).expect("snapshot");
    let source_debug = format!("{source:?}");
    let snapshot_debug = format!("{snapshot:?}");
    assert!(!source_debug.contains("session-1"));
    assert!(!source_debug.contains("/workspace"));
    assert!(!snapshot_debug.contains("session-1"));
    assert!(!snapshot_debug.contains("/workspace"));
}

#[test]
fn validates_workspace_and_identity_at_source_creation() {
    let mut dependencies = dependencies(None, None, None, None, None);
    dependencies.session_id.clear();
    assert_eq!(
        TrustedSyndridCompositionSource::new(dependencies).unwrap_err(),
        TrustedCompositionSnapshotError::InvalidSessionIdentity
    );
}

#[test]
fn snapshot_does_not_capture_context_or_emit_events() {
    let (source, _, _) = valid_source();
    let snapshot = source.snapshot(request()).expect("snapshot");
    assert!(
        snapshot
            .context_provider
            .capture(
                &codex_app_server::ProductionTurnAdmissionInput::new(
                    "turn-1",
                    "session-1",
                    "objective",
                    PathBuf::from("/workspace"),
                )
                .expect("admission input")
            )
            .expect("context")
            .is_none()
    );
}
