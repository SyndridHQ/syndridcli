use super::*;
use codex_app_server_client::TrustedCompositionSnapshotRequest;
use codex_app_server_client::legacy_core::RoleCapabilityConfiguration;
use codex_app_server_client::legacy_core::RoleCapabilityDeclaration;
use codex_app_server_client::legacy_core::RoleCapabilityValidationContext;
use codex_app_server_client::legacy_core::RoutingRole;
use codex_app_server_client::legacy_core::SessionExecutionPolicyState;
use codex_app_server_client::legacy_core::SubagentToolKind;
use codex_app_server_client::legacy_core::ValidatedRoleCapabilitySet;
use pretty_assertions::assert_eq;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::mpsc;

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
        PathBuf::from("/workspace"),
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
