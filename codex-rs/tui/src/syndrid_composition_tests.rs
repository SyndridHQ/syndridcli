use super::*;
use codex_app_server_client::TrustedCompositionSnapshotRequest;
use codex_app_server_client::legacy_core::AccountPoolMember;
use codex_app_server_client::legacy_core::AccountPoolProviderFamily;
use codex_app_server_client::legacy_core::AccountPoolSelectionPolicy;
use codex_app_server_client::legacy_core::AccountPoolTarget;
use codex_app_server_client::legacy_core::CodexAccountConnectionMetadata;
use codex_app_server_client::legacy_core::CodexAccountProfileId;
use codex_app_server_client::legacy_core::CodexAccountProfileRegistry;
use codex_app_server_client::legacy_core::CodexAccountProfileState;
use codex_app_server_client::legacy_core::ConnectionValidationStatus;
use codex_app_server_client::legacy_core::NamedAccountPool;
use codex_app_server_client::legacy_core::NamedAccountPoolRegistry;
use codex_app_server_client::legacy_core::PoolId;
use codex_app_server_client::legacy_core::PoolMemberId;
use codex_app_server_client::legacy_core::RoleCapabilityConfiguration;
use codex_app_server_client::legacy_core::RoleCapabilityDeclaration;
use codex_app_server_client::legacy_core::RoleCapabilityValidationContext;
use codex_app_server_client::legacy_core::RoutingAssignment;
use codex_app_server_client::legacy_core::RoutingConnectionInfo;
use codex_app_server_client::legacy_core::RoutingProfile;
use codex_app_server_client::legacy_core::RoutingProfileId;
use codex_app_server_client::legacy_core::RoutingProfileRegistry;
use codex_app_server_client::legacy_core::RoutingRole;
use codex_app_server_client::legacy_core::SessionExecutionPolicyState;
use codex_app_server_client::legacy_core::SubagentToolKind;
use codex_app_server_client::legacy_core::ValidatedRoleCapabilitySet;
use pretty_assertions::assert_eq;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tempfile::tempdir;
use tokio::sync::mpsc;

fn pool_transaction_profile(id: &str, pool_id: Option<&str>) -> RoutingProfile {
    let mut profile = RoutingProfile::new(RoutingProfileId::new(id).unwrap(), id, 1).unwrap();
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
                    connection_id: pool_id
                        .is_none()
                        .then(|| "account-a1".to_string())
                        .unwrap_or_default(),
                    provider_id: "codex".to_string(),
                    model_id: "configured-model".to_string(),
                    enabled: true,
                    label: Some(role.to_string()),
                    pool_id: pool_id.map(|id| PoolId::new(id).unwrap()),
                },
            )
            .unwrap();
    }
    profile
}

fn pool_transaction_account_registry() -> CodexAccountProfileRegistry {
    let mut accounts = CodexAccountProfileRegistry::default();
    for id in ["account-a1", "account-a2"] {
        accounts
            .insert(CodexAccountConnectionMetadata {
                connection_id: id.to_string(),
                profile_id: CodexAccountProfileId::new(id).unwrap(),
                provider_id: "codex".to_string(),
                label: id.to_string(),
                state: CodexAccountProfileState::Connected,
                account_email: None,
                account_id: Some(format!("canonical-{id}")),
                plan_label: None,
                enabled: true,
                validation: ConnectionValidationStatus::Valid,
                last_authenticated_at: None,
                last_validated_at: None,
                credential_reference: CodexAccountProfileRegistry::credential_reference_for(id)
                    .unwrap(),
                schema_version: 1,
            })
            .unwrap();
    }
    accounts
}

fn pool_transaction_registry() -> NamedAccountPoolRegistry {
    let mut pools = NamedAccountPoolRegistry::default();
    pools
        .insert(NamedAccountPool {
            id: PoolId::new("codex-primary").unwrap(),
            display_name: "Codex primary".to_string(),
            provider_family: AccountPoolProviderFamily::NativeCodex,
            members: vec![
                AccountPoolMember {
                    id: PoolMemberId::new("member-a1").unwrap(),
                    target: AccountPoolTarget::native_codex(
                        CodexAccountProfileId::new("account-a1").unwrap(),
                    ),
                },
                AccountPoolMember {
                    id: PoolMemberId::new("member-a2").unwrap(),
                    target: AccountPoolTarget::native_codex(
                        CodexAccountProfileId::new("account-a2").unwrap(),
                    ),
                },
            ],
            selection_policy: AccountPoolSelectionPolicy::ExplicitMember(
                PoolMemberId::new("member-a1").unwrap(),
            ),
        })
        .unwrap();
    pools
}

fn pool_transaction_composition(
    profile_path: Option<PathBuf>,
) -> (
    TuiSyndridSessionComposition,
    RoutingProfile,
    Arc<RwLock<NamedAccountPoolRegistry>>,
) {
    let workspace_root = std::env::current_dir().expect("workspace");
    let accounts = Arc::new(pool_transaction_account_registry());
    let mut connections = RoutingConnectionDirectory::default();
    connections.add_codex(&accounts);
    let pools = Arc::new(RwLock::new(pool_transaction_registry()));
    let persisted = pool_transaction_profile("persisted", None);
    let mut profiles = RoutingProfileRegistry::default();
    profiles.insert(persisted).unwrap();
    profiles.active_profile_id = Some(RoutingProfileId::new("persisted").unwrap());
    let routing_authority = Arc::new(TuiRoutingAuthority::from_loaded(
        Some(Arc::new(profiles)),
        Some(Arc::new(connections)),
        Some(Arc::clone(&pools)),
        Some(Arc::clone(&accounts)),
        Some(Arc::new(OmniRouteRegistry::default())),
        None,
        profile_path,
    ));
    let policy = ExecutionModeSelection::Balanced.resolve().unwrap();
    let capabilities = codex_app_server_client::legacy_core::validate_role_capabilities(
        &RoleCapabilityConfiguration::new(
            [
                RoutingRole::Planner,
                RoutingRole::Executor,
                RoutingRole::Verifier,
                RoutingRole::Repair,
            ]
            .into_iter()
            .map(RoleCapabilityDeclaration::no_tools)
            .collect(),
        ),
        &policy,
        &RoleCapabilityValidationContext::new(
            workspace_root.clone(),
            [SubagentToolKind::ReadFile].into_iter().collect(),
            false,
            false,
        ),
    )
    .unwrap();
    let policy_state = Arc::new(
        SessionExecutionPolicyState::with_strategy_selection(
            OrchestrationMode::Single,
            ExecutionModeSelection::Balanced,
            codex_app_server_client::legacy_core::SessionPolicySource::Default,
        )
        .unwrap(),
    );
    let (event_sender, _event_receiver) = mpsc::channel(8);
    let provider = Arc::new(TuiProviderAuthority::from_registries(
        (*accounts).clone(),
        OmniRouteRegistry::default(),
    ));
    let tools = Arc::new(TuiApprovedToolAuthority::from_validated(
        capabilities,
        workspace_root.clone(),
    ));
    let mut composition = TuiSyndridSessionComposition::new_with_authorities(
        "pool-transaction-session".to_string(),
        workspace_root,
        policy_state,
        routing_authority.clone(),
        provider,
        tools,
        Arc::new(TuiProductionContextProvider::new()),
        event_sender,
    )
    .unwrap();
    composition.routing_authority = Some(routing_authority);
    (
        composition,
        pool_transaction_profile("candidate", Some("codex-primary")),
        pools,
    )
}

fn admission(objective: &str) -> ProductionTurnAdmissionInput {
    ProductionTurnAdmissionInput::new(
        "turn-1",
        "session-1",
        objective,
        std::env::current_dir().expect("current directory"),
    )
    .expect("admission input")
}

#[test]
fn context_is_bounded_deterministic_and_objective_is_not_duplicated() {
    let provider = TuiProductionContextProvider::new();
    provider.record_user_message("objective");
    provider.record_assistant_message(&"é".repeat(20_000));

    let captured = provider
        .capture(&admission("objective"))
        .expect("context capture")
        .expect("bounded context should be present");
    assert!(captured.len() <= MAX_CONTEXT_BYTES);
    assert!(captured.is_char_boundary(captured.len()));
    assert!(!captured.contains("user: objective"));
    assert!(captured.starts_with("assistant: "));
}

#[test]
fn missing_product_authority_remains_typed_unavailable() {
    let authority = TuiRoutingAuthority::unavailable();
    assert_eq!(
        authority.snapshot().unwrap_err(),
        TrustedCompositionSnapshotError::RoutingUnavailable
    );

    let authority = TuiApprovedToolAuthority::unavailable();
    assert_eq!(
        authority
            .snapshot(PathBuf::from("/workspace").as_path())
            .unwrap_err(),
        TrustedCompositionSnapshotError::RoleCapabilityAuthorityUnavailable
    );
}

#[test]
fn explicit_role_capabilities_produce_a_redacted_snapshot() {
    let policy = codex_app_server_client::legacy_core::ExecutionModeSelection::Balanced
        .resolve()
        .expect("policy");
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
        std::env::current_dir().expect("workspace"),
        [SubagentToolKind::ReadFile].into_iter().collect(),
        false,
        false,
    );
    let capabilities: ValidatedRoleCapabilitySet =
        codex_app_server_client::legacy_core::validate_role_capabilities(
            &configuration,
            &policy,
            &context,
        )
        .expect("capabilities");
    let authority =
        TuiApprovedToolAuthority::from_validated(capabilities, PathBuf::from("/workspace"));
    let snapshot = authority
        .snapshot(PathBuf::from("/workspace").as_path())
        .expect("approved tools");
    assert_eq!(snapshot.role_capabilities.roles().count(), 4);
    assert!(format!("{authority:?}").contains("<tool-authority>"));
    assert!(format!("{snapshot:?}").contains("<redacted>"));
}

#[test]
fn persisted_role_capabilities_produce_an_immutable_tool_snapshot() {
    let home = tempdir().expect("codex home");
    fs::write(
        home.path().join("syndrid-role-capabilities.json"),
        r#"{
          "schema_version": 1,
          "planner": {"mode": "no_tools"},
          "executor": {"mode": "no_tools"},
          "verifier": {"mode": "no_tools"},
          "repair": {"mode": "no_tools"}
        }"#,
    )
    .expect("role capabilities");
    let policy = codex_app_server_client::legacy_core::ExecutionModeSelection::Balanced
        .resolve()
        .expect("policy");
    let context = RoleCapabilityValidationContext::new(
        PathBuf::from("/workspace"),
        [SubagentToolKind::ReadFile].into_iter().collect(),
        false,
        false,
    );
    let authority = TuiApprovedToolAuthority::from_persisted(home.path(), &policy, &context);
    let snapshot = authority
        .snapshot(PathBuf::from("/workspace").as_path())
        .expect("persisted capabilities");
    assert_eq!(snapshot.role_capabilities.roles().count(), 4);
}

#[test]
fn composition_source_is_session_scoped_and_redacted() {
    let policy_state = Arc::new(SessionExecutionPolicyState::new().expect("policy"));
    let (event_sender, _event_receiver) = mpsc::channel(1);
    let composition = TuiSyndridSessionComposition::new(
        "session-1".to_string(),
        PathBuf::from("/workspace"),
        policy_state,
        event_sender,
    )
    .expect("composition");
    let source = composition.source();
    assert!(composition.runtime().is_none());
    let debug = format!("{source:?}");
    assert!(!debug.contains("session-1"));
    assert!(!debug.contains("/workspace"));
    assert_eq!(source.session_id(), "session-1");
    assert_eq!(
        source
            .snapshot(TrustedCompositionSnapshotRequest {
                session_id: "other-session".to_string(),
                workspace_root: PathBuf::from("/workspace"),
            })
            .unwrap_err(),
        TrustedCompositionSnapshotError::SessionMismatch
    );
}

#[test]
fn routing_recommendation_view_does_not_mutate_routing_state() {
    let (composition, _, pools) = pool_transaction_composition(None);
    let saved_before = composition
        .current_routing_profile()
        .expect("saved profile");
    let installed_before = composition.runtime();
    let pool_before = pools.read().expect("pool registry").clone();
    let cooldown_before = composition.cooldown_snapshot();

    let recommendation = composition.routing_recommendation();

    assert!(recommendation.is_some());
    assert_eq!(
        composition
            .current_routing_profile()
            .expect("saved profile"),
        saved_before
    );
    assert!(installed_before.is_none());
    assert!(composition.runtime().is_none());
    assert_eq!(pools.read().expect("pool registry").clone(), pool_before);
    assert_eq!(composition.cooldown_snapshot(), cooldown_before);
    assert!(composition.session_routing_override().is_none());
}

#[test]
fn canonical_loader_reuses_existing_registry_formats() {
    let home = tempdir().expect("codex home");
    let mut registry = codex_app_server_client::legacy_core::RoutingProfileRegistry::default();
    let profile_id = codex_app_server_client::legacy_core::RoutingProfileId::new("profile-1")
        .expect("profile ID");
    let profile = codex_app_server_client::legacy_core::RoutingProfile::new(
        profile_id.clone(),
        "Profile 1",
        1,
    )
    .expect("profile");
    registry.insert(profile).expect("profile insert");
    registry.active_profile_id = Some(profile_id);
    registry
        .save(&home.path().join("syndrid-routing-profiles.json"))
        .expect("profile save");

    let authorities = TuiCanonicalAuthorities::load(home.path());
    assert!(authorities.routing.profiles.is_some());
    assert!(authorities.provider.accounts.is_some());
    assert!(authorities.provider.omni_route.is_some());
    assert_eq!(
        authorities.routing.snapshot(),
        Err(TrustedCompositionSnapshotError::RoutingInvalid)
    );
}

#[test]
fn pool_bound_routing_snapshot_resolves_exactly_and_is_immutable() {
    let profile_id = RoutingProfileId::new("pool-profile").unwrap();
    let mut profile = RoutingProfile::new(profile_id, "Pool profile", 1).unwrap();
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
                    connection_id: String::new(),
                    provider_id: "codex".to_string(),
                    model_id: "model".to_string(),
                    enabled: true,
                    label: None,
                    pool_id: Some(PoolId::new("codex-primary").unwrap()),
                },
            )
            .unwrap();
    }
    let mut accounts = CodexAccountProfileRegistry::default();
    let mut connections = RoutingConnectionDirectory::default();
    for account_id in ["account-a1", "account-a2"] {
        accounts
            .insert(CodexAccountConnectionMetadata {
                connection_id: account_id.to_string(),
                profile_id: CodexAccountProfileId::new(account_id).unwrap(),
                provider_id: "codex".to_string(),
                label: account_id.to_string(),
                state: CodexAccountProfileState::Connected,
                account_email: None,
                account_id: None,
                plan_label: None,
                enabled: true,
                validation: ConnectionValidationStatus::Valid,
                last_authenticated_at: None,
                last_validated_at: None,
                credential_reference: CodexAccountProfileRegistry::credential_reference_for(
                    account_id,
                )
                .unwrap(),
                schema_version: 1,
            })
            .unwrap();
        connections.insert(RoutingConnectionInfo {
            connection_id: account_id.to_string(),
            provider_id: "codex".to_string(),
            enabled: true,
            validation: ConnectionValidationStatus::Valid,
            authentication_supported: true,
            models: None,
        });
    }
    let mut pools = NamedAccountPoolRegistry::default();
    pools
        .insert(NamedAccountPool {
            id: PoolId::new("codex-primary").unwrap(),
            display_name: "Codex primary".to_string(),
            provider_family: AccountPoolProviderFamily::NativeCodex,
            members: vec![
                AccountPoolMember {
                    id: PoolMemberId::new("a1").unwrap(),
                    target: AccountPoolTarget::native_codex(
                        CodexAccountProfileId::new("account-a1").unwrap(),
                    ),
                },
                AccountPoolMember {
                    id: PoolMemberId::new("a2").unwrap(),
                    target: AccountPoolTarget::native_codex(
                        CodexAccountProfileId::new("account-a2").unwrap(),
                    ),
                },
            ],
            selection_policy: AccountPoolSelectionPolicy::ExplicitMember(
                PoolMemberId::new("a1").unwrap(),
            ),
        })
        .unwrap();
    let shared_pools = Arc::new(RwLock::new(pools));
    let authority = TuiRoutingAuthority::from_loaded(
        None,
        Some(Arc::new(connections)),
        Some(Arc::clone(&shared_pools)),
        Some(Arc::new(accounts)),
        Some(Arc::new(OmniRouteRegistry::default())),
        None,
        None,
    );

    let first = authority.snapshot_for_profile(&profile).unwrap();
    assert_eq!(
        first
            .profile
            .assignments
            .get(&RoutingRole::Planner)
            .unwrap()
            .connection_id,
        "account-a1"
    );
    let mut changed = shared_pools.write().unwrap();
    let mut pool = changed
        .remove(&PoolId::new("codex-primary").unwrap())
        .unwrap();
    pool.selection_policy =
        AccountPoolSelectionPolicy::ExplicitMember(PoolMemberId::new("a2").unwrap());
    changed.insert(pool).unwrap();
    drop(changed);
    let second = authority.snapshot_for_profile(&profile).unwrap();
    assert_eq!(
        first
            .profile
            .assignments
            .get(&RoutingRole::Planner)
            .unwrap()
            .connection_id,
        "account-a1"
    );
    assert_eq!(
        second
            .profile
            .assignments
            .get(&RoutingRole::Planner)
            .unwrap()
            .connection_id,
        "account-a2"
    );
}

#[test]
fn single_strategy_uses_codex_compatibility_path() {
    let policy_state = Arc::new(
        SessionExecutionPolicyState::with_strategy_selection(
            OrchestrationMode::Single,
            codex_app_server_client::legacy_core::ExecutionModeSelection::Fast,
            codex_app_server_client::legacy_core::SessionPolicySource::Default,
        )
        .expect("single policy"),
    );
    let (event_sender, _event_receiver) = mpsc::channel(1);
    let composition = TuiSyndridSessionComposition::new(
        "single-session".to_string(),
        std::env::current_dir().expect("workspace"),
        policy_state,
        event_sender,
    )
    .expect("composition");

    assert_eq!(
        composition.execution_capability(),
        ProductionExecutionCapability::CodexCompatibility
    );
    assert!(composition.runtime().is_none());
}

#[test]
fn automatic_strategy_keeps_syndrid_authority_without_codex_fallback() {
    let policy_state = Arc::new(
        SessionExecutionPolicyState::with_strategy_selection(
            OrchestrationMode::Automatic,
            codex_app_server_client::legacy_core::ExecutionModeSelection::Fast,
            codex_app_server_client::legacy_core::SessionPolicySource::Default,
        )
        .expect("automatic policy"),
    );
    assert_eq!(
        policy_state.strategy_availability().expect("availability"),
        codex_app_server_client::legacy_core::OrchestrationStrategyAvailability::Available
    );
    let (event_sender, _event_receiver) = mpsc::channel(1);
    let composition = TuiSyndridSessionComposition::new(
        "automatic-session".to_string(),
        PathBuf::from("/workspace"),
        policy_state,
        event_sender,
    )
    .expect("composition");

    assert_eq!(
        composition.execution_capability(),
        ProductionExecutionCapability::SyndridOrchestration
    );
    assert!(composition.runtime().is_none());
}

fn routing_fixture(profile_id: &str) -> (RoutingProfileRegistry, RoutingConnectionDirectory) {
    let profile_id = RoutingProfileId::new(profile_id).expect("profile ID");
    let mut profile =
        RoutingProfile::new(profile_id.clone(), "session profile", 1).expect("routing profile");
    let assignment = RoutingAssignment {
        connection_id: "codex-account".to_string(),
        provider_id: "codex".to_string(),
        model_id: "configured-model".to_string(),
        enabled: true,
        label: None,
        pool_id: None,
    };
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
    ] {
        profile
            .assign(role, assignment.clone())
            .expect("role assignment");
    }
    let mut registry = RoutingProfileRegistry::default();
    registry.insert(profile).expect("profile insert");
    registry.active_profile_id = Some(profile_id);
    let mut connections = RoutingConnectionDirectory::default();
    connections.insert(RoutingConnectionInfo {
        connection_id: "codex-account".to_string(),
        provider_id: "codex".to_string(),
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        authentication_supported: true,
        models: Some(vec!["configured-model".to_string()]),
    });
    (registry, connections)
}

#[test]
fn session_routing_override_takes_precedence_and_clear_restores_persisted_profile() {
    let (registry, connections) = routing_fixture("persisted");
    let authority = TuiRoutingAuthority::from_registry(registry, connections);
    assert_eq!(
        authority
            .snapshot()
            .expect("persisted snapshot")
            .profile_id
            .as_str(),
        "persisted"
    );

    let (candidate_registry, _) = routing_fixture("session");
    let candidate = candidate_registry
        .active()
        .expect("candidate profile")
        .clone();
    authority
        .set_session_override(Some(candidate))
        .expect("publish session override");
    assert_eq!(
        authority.session_override().expect("override").id.as_str(),
        "session"
    );
    assert_eq!(
        authority
            .snapshot()
            .expect("override snapshot")
            .profile_id
            .as_str(),
        "session"
    );

    authority
        .publish_session_override(None)
        .expect("clear session override");
    assert!(authority.session_override().is_none());
    assert_eq!(
        authority
            .snapshot()
            .expect("restored snapshot")
            .profile_id
            .as_str(),
        "persisted"
    );
}

#[test]
fn explicit_routing_profile_save_updates_the_canonical_writer_and_can_be_restored() {
    let home = tempdir().expect("routing profile directory");
    let path = home.path().join("syndrid-routing-profiles.json");
    let (registry, connections) = routing_fixture("persisted");
    registry.save(&path).expect("initial profile save");
    let original_bytes = fs::read(&path).expect("initial profile bytes");
    let authority = TuiRoutingAuthority::from_loaded(
        Some(Arc::new(registry)),
        Some(Arc::new(connections)),
        None,
        None,
        None,
        None,
        Some(path.clone()),
    );

    let (candidate_registry, _) = routing_fixture("candidate");
    let candidate = candidate_registry
        .active()
        .expect("candidate profile")
        .clone();
    let previous = authority.save_profile(&candidate).expect("save candidate");
    assert_eq!(
        authority.persisted_profile().expect("saved profile"),
        candidate
    );
    assert_ne!(fs::read(&path).expect("saved bytes"), original_bytes);

    authority
        .restore_profiles(previous)
        .expect("restore profile");
    assert_eq!(fs::read(path).expect("restored bytes"), original_bytes);
    assert_eq!(
        authority
            .persisted_profile()
            .expect("restored profile")
            .id
            .as_str(),
        "persisted"
    );
}

#[tokio::test]
async fn runtime_installation_failure_does_not_publish_the_candidate() {
    let policy_state = Arc::new(SessionExecutionPolicyState::new().expect("policy"));
    let (event_sender, _event_receiver) = mpsc::channel(1);
    let composition = TuiSyndridSessionComposition::new(
        "failure-session".to_string(),
        PathBuf::from("/workspace"),
        policy_state,
        event_sender,
    )
    .expect("composition");
    let calls = Arc::new(AtomicUsize::new(0));
    let prepared = PreparedSessionRoutingUpdate {
        override_profile: None,
        runtime: None,
        pool_snapshot: None,
    };

    let result = composition
        .install_prepared_session_routing_update(
            ProductionExecutionCapability::CodexCompatibility,
            prepared,
            {
                let calls = Arc::clone(&calls);
                move |_, _| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async { Err("injected installation failure".to_string()) }
                }
            },
        )
        .await;

    assert_eq!(result, Err("injected installation failure".to_string()));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(composition.session_routing_override().is_none());
    assert!(composition.runtime().is_none());
}

#[tokio::test]
async fn publication_failure_restores_the_previous_runtime() {
    let policy_state = Arc::new(SessionExecutionPolicyState::new().expect("policy"));
    let (event_sender, _event_receiver) = mpsc::channel(1);
    let composition = TuiSyndridSessionComposition::new(
        "publication-session".to_string(),
        PathBuf::from("/workspace"),
        policy_state,
        event_sender,
    )
    .expect("composition");
    let calls = Arc::new(AtomicUsize::new(0));
    let prepared = PreparedSessionRoutingUpdate {
        override_profile: None,
        runtime: None,
        pool_snapshot: None,
    };

    let result = composition
        .install_prepared_session_routing_update(
            ProductionExecutionCapability::CodexCompatibility,
            prepared,
            {
                let calls = Arc::clone(&calls);
                move |_, _| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async { Ok(()) }
                }
            },
        )
        .await;

    assert!(result.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(composition.session_routing_override().is_none());
    assert!(composition.runtime().is_none());
}

#[tokio::test]
async fn persisted_routing_update_restores_exact_bytes_after_installation_failure() {
    let home = tempdir().expect("routing profile directory");
    let path = home.path().join("syndrid-routing-profiles.json");
    let (registry, connections) = routing_fixture("persisted");
    registry.save(&path).expect("initial profile save");
    let original_bytes = fs::read(&path).expect("initial profile bytes");
    let authority = TuiRoutingAuthority::from_loaded(
        Some(Arc::new(registry)),
        Some(Arc::new(connections)),
        None,
        None,
        None,
        None,
        Some(path.clone()),
    );
    let (candidate_registry, _) = routing_fixture("candidate");
    let candidate = candidate_registry
        .active()
        .expect("candidate profile")
        .clone();
    let policy_state = Arc::new(
        SessionExecutionPolicyState::with_strategy_selection(
            OrchestrationMode::Single,
            codex_app_server_client::legacy_core::ExecutionModeSelection::Balanced,
            codex_app_server_client::legacy_core::SessionPolicySource::Default,
        )
        .expect("single policy"),
    );
    let (event_sender, _event_receiver) = mpsc::channel(1);
    let mut composition = TuiSyndridSessionComposition::new(
        "save-failure-session".to_string(),
        PathBuf::from("/workspace"),
        Arc::clone(&policy_state),
        event_sender,
    )
    .expect("composition");
    composition.routing_authority = Some(Arc::new(authority));
    let prepared = PreparedSessionRoutingUpdate {
        override_profile: Some(candidate.clone()),
        runtime: None,
        pool_snapshot: None,
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let result = composition
        .install_prepared_session_routing_update_and_save(
            ProductionExecutionCapability::CodexCompatibility,
            prepared,
            &candidate,
            {
                let calls = Arc::clone(&calls);
                move |_, _| {
                    let attempt = calls.fetch_add(1, Ordering::SeqCst);
                    async move {
                        if attempt == 0 {
                            Err("injected installation failure".to_string())
                        } else {
                            Ok(())
                        }
                    }
                }
            },
        )
        .await;

    assert!(result.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        fs::read(path).expect("restored profile bytes"),
        original_bytes
    );
    assert!(composition.session_routing_override().is_none());
}

#[tokio::test]
async fn persisted_routing_update_rejects_an_existing_routing_reservation_before_writing() {
    let home = tempdir().expect("routing profile directory");
    let path = home.path().join("syndrid-routing-profiles.json");
    let (registry, connections) = routing_fixture("persisted");
    registry.save(&path).expect("initial profile save");
    let original_bytes = fs::read(&path).expect("initial profile bytes");
    let authority = TuiRoutingAuthority::from_loaded(
        Some(Arc::new(registry)),
        Some(Arc::new(connections)),
        None,
        None,
        None,
        None,
        Some(path.clone()),
    );
    let (candidate_registry, _) = routing_fixture("candidate");
    let candidate = candidate_registry
        .active()
        .expect("candidate profile")
        .clone();
    let policy_state = Arc::new(SessionExecutionPolicyState::new().expect("policy"));
    let (event_sender, _event_receiver) = mpsc::channel(1);
    let mut composition = TuiSyndridSessionComposition::new(
        "busy-session".to_string(),
        PathBuf::from("/workspace"),
        Arc::clone(&policy_state),
        event_sender,
    )
    .expect("composition");
    composition.routing_authority = Some(Arc::new(authority));
    let _guard = policy_state
        .begin_routing_update()
        .expect("first routing update reservation");
    let result = composition
        .install_prepared_session_routing_update_and_save(
            ProductionExecutionCapability::CodexCompatibility,
            PreparedSessionRoutingUpdate {
                override_profile: Some(candidate.clone()),
                runtime: None,
                pool_snapshot: None,
            },
            &candidate,
            |_, _| async { Ok(()) },
        )
        .await;

    assert!(result.is_err());
    assert_eq!(
        fs::read(path).expect("unchanged profile bytes"),
        original_bytes
    );
    assert!(composition.session_routing_override().is_none());
}

#[test]
fn pool_bound_preparation_is_pinned_and_pool_deletion_blocks_only_new_preparation() {
    let (composition, candidate, pools) = pool_transaction_composition(None);
    let resolved = composition
        .routing_authority
        .as_ref()
        .unwrap()
        .snapshot_for_profile(&candidate)
        .unwrap();
    assert_eq!(
        resolved
            .profile
            .assignments
            .get(&RoutingRole::Planner)
            .unwrap()
            .connection_id,
        "account-a1"
    );
    let _prepared = composition
        .prepare_session_routing_override(candidate.clone())
        .expect("pool-bound preparation");
    pools
        .write()
        .unwrap()
        .remove(&PoolId::new("codex-primary").unwrap());
    assert!(
        composition
            .routing_authority
            .as_ref()
            .unwrap()
            .snapshot_for_profile(&candidate)
            .is_err()
    );
}

#[tokio::test]
async fn pool_bound_session_apply_publishes_source_pool_and_resolves_exact_identity() {
    let (composition, candidate, _pools) = pool_transaction_composition(None);
    let resolved = composition
        .routing_authority
        .as_ref()
        .unwrap()
        .snapshot_for_profile(&candidate)
        .unwrap();
    assert_eq!(
        resolved
            .profile
            .assignments
            .get(&RoutingRole::Planner)
            .unwrap()
            .connection_id,
        "account-a1"
    );
    let prepared = composition
        .prepare_session_routing_override(candidate.clone())
        .expect("pool-bound preparation");
    composition
        .install_prepared_session_routing_update(
            ProductionExecutionCapability::SyndridOrchestration,
            prepared,
            |_, runtime| async move {
                assert!(runtime.is_none());
                Ok(())
            },
        )
        .await
        .expect("pool-bound session apply");
    assert_eq!(
        composition
            .session_routing_override()
            .unwrap()
            .assignments
            .get(&RoutingRole::Planner)
            .unwrap()
            .pool_id,
        Some(PoolId::new("codex-primary").unwrap())
    );
    let resolved = composition
        .routing_authority
        .as_ref()
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(
        resolved
            .profile
            .assignments
            .get(&RoutingRole::Planner)
            .unwrap()
            .connection_id,
        "account-a1"
    );
}

#[tokio::test]
async fn pool_bound_persisted_apply_retains_pool_reference_and_rolls_back_on_failure() {
    let home = tempdir().unwrap();
    let path = home.path().join("syndrid-routing-profiles.json");
    let (registry, _) = routing_fixture("persisted");
    registry.save(&path).unwrap();
    let original_bytes = fs::read(&path).unwrap();
    let (composition, candidate, _pools) = pool_transaction_composition(Some(path.clone()));
    assert!(
        composition
            .routing_authority
            .as_ref()
            .unwrap()
            .snapshot_for_profile(&candidate)
            .is_ok()
    );
    let prepared = composition
        .prepare_session_routing_override(candidate.clone())
        .expect("pool-bound preparation");
    composition
        .install_prepared_session_routing_update_and_save(
            ProductionExecutionCapability::SyndridOrchestration,
            prepared,
            &candidate,
            |_, runtime| async move {
                assert!(runtime.is_none());
                Ok(())
            },
        )
        .await
        .expect("pool-bound persisted apply");
    let saved_bytes = fs::read(&path).unwrap();
    assert!(String::from_utf8_lossy(&saved_bytes).contains("codex-primary"));
    assert!(!String::from_utf8_lossy(&saved_bytes).contains("member-a1"));
    assert_eq!(composition.current_routing_profile().unwrap(), candidate);

    let (composition, candidate, _pools) = pool_transaction_composition(Some(path.clone()));
    assert!(
        composition
            .routing_authority
            .as_ref()
            .unwrap()
            .snapshot_for_profile(&candidate)
            .is_ok()
    );
    let prepared = composition
        .prepare_session_routing_override(candidate.clone())
        .expect("pool-bound preparation");
    let result = composition
        .install_prepared_session_routing_update_and_save(
            ProductionExecutionCapability::SyndridOrchestration,
            prepared,
            &candidate,
            |_, _| async { Err("injected pool runtime failure".to_string()) },
        )
        .await;
    assert!(result.is_err());
    assert_eq!(fs::read(&path).unwrap(), saved_bytes);
    assert!(composition.session_routing_override().is_none());
    assert_eq!(
        composition.current_routing_profile().unwrap().id.as_str(),
        "persisted"
    );
    assert_ne!(fs::read(&path).unwrap(), original_bytes);
}

#[tokio::test]
async fn pool_bound_clear_override_restores_saved_pool_profile() {
    let (composition, candidate, _pools) = pool_transaction_composition(None);
    assert!(
        composition
            .routing_authority
            .as_ref()
            .unwrap()
            .snapshot_for_profile(&candidate)
            .is_ok()
    );
    let prepared = composition
        .prepare_session_routing_override(candidate.clone())
        .expect("pool-bound preparation");
    composition
        .install_prepared_session_routing_update(
            ProductionExecutionCapability::SyndridOrchestration,
            prepared,
            |_, _| async { Ok(()) },
        )
        .await
        .expect("pool-bound session apply");
    let clear = composition
        .prepare_clear_session_routing_override()
        .expect("clear preparation");
    composition
        .install_prepared_session_routing_update(
            ProductionExecutionCapability::SyndridOrchestration,
            clear,
            |_, _| async { Ok(()) },
        )
        .await
        .expect("clear override");
    assert!(composition.session_routing_override().is_none());
    assert_eq!(
        composition.current_routing_profile().unwrap().id.as_str(),
        "persisted"
    );
}
