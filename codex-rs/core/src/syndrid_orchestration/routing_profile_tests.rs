use super::*;
use crate::CodexAccountConnectionMetadata;
use crate::CodexAccountProfileId;
use crate::CodexAccountProfileRegistry;
use crate::CodexAccountProfileState;
use crate::syndrid_orchestration::provider_connection::ConnectionValidationStatus;
use tempfile::tempdir;

fn assignment(connection_id: &str, provider_id: &str, model_id: &str) -> RoutingAssignment {
    RoutingAssignment {
        connection_id: connection_id.to_string(),
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        enabled: true,
        label: None,
    }
}

fn complete_profile() -> RoutingProfile {
    let mut profile =
        RoutingProfile::new(RoutingProfileId::new("default").expect("id"), "Default", 1)
            .expect("profile");
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
    ] {
        profile
            .assign(
                role,
                assignment("omniroute-local", "omniroute", "provider/model"),
            )
            .expect("assignment");
    }
    profile
}

fn codex_account(
    connection_id: &str,
    state: CodexAccountProfileState,
) -> CodexAccountConnectionMetadata {
    CodexAccountConnectionMetadata {
        connection_id: connection_id.to_string(),
        profile_id: CodexAccountProfileId::new(connection_id).expect("profile id"),
        provider_id: "codex".to_string(),
        label: connection_id.to_string(),
        state,
        account_email: None,
        account_id: None,
        plan_label: None,
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        last_authenticated_at: None,
        last_validated_at: None,
        credential_reference: CodexAccountProfileRegistry::credential_reference_for(connection_id)
            .expect("credential reference"),
        schema_version: 1,
    }
}

#[test]
fn profile_ids_roles_and_required_assignments_are_bounded() {
    for value in ["", "../profile", "profile/name", &"x".repeat(129)] {
        assert_eq!(
            RoutingProfileId::new(value),
            Err(RoutingProfileError::InvalidProfileId)
        );
    }
    assert_eq!(RoutingRole::parse("repair"), Ok(RoutingRole::Repair));
    assert_eq!(
        RoutingRole::parse("unknown"),
        Err(RoutingProfileError::InvalidRole)
    );
    let mut profile =
        RoutingProfile::new(RoutingProfileId::new("default").expect("id"), "Default", 1)
            .expect("profile");
    assert_eq!(
        profile.validate_required_roles(),
        Err(RoutingProfileError::MissingRoleAssignment)
    );
    profile
        .assign(RoutingRole::Main, assignment("c", "p", "m"))
        .expect("assignment");
    assert_eq!(
        profile.assign(RoutingRole::Main, assignment("c", "p", "m")),
        Err(RoutingProfileError::DuplicateRoleAssignment)
    );
}

#[test]
fn each_sequential_role_is_required_but_repair_is_optional() {
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
    ] {
        let mut profile = complete_profile();
        profile.unassign(role).expect("required assignment");
        assert_eq!(
            profile.validate_required_roles(),
            Err(RoutingProfileError::MissingRoleAssignment)
        );
    }

    let profile = complete_profile();
    assert!(!profile.assignments.contains_key(&RoutingRole::Repair));
    assert_eq!(profile.validate_required_roles(), Ok(()));
}

#[test]
fn registry_round_trips_active_profiles_and_rejects_active_deletion() {
    let directory = tempdir().expect("tempdir");
    let path = directory.path().join("routing.json");
    let mut registry = RoutingProfileRegistry::default();
    registry.insert(complete_profile()).expect("insert");
    let id = RoutingProfileId::new("default").expect("id");
    registry.activate(&id).expect("activate");
    registry.save(&path).expect("save");
    let mut loaded = RoutingProfileRegistry::load(&path).expect("load");
    assert_eq!(loaded.active_profile_id, Some(id.clone()));
    assert_eq!(
        loaded.delete(&id),
        Err(RoutingProfileError::ActiveProfileDeletionRejected)
    );
    assert_eq!(
        serde_json::to_vec(&loaded).expect("serialize"),
        serde_json::to_vec(&loaded).expect("serialize")
    );
    loaded.save(&path).expect("atomic replacement");
}

#[test]
fn profile_validation_is_local_and_distinguishes_unverified_models() {
    let mut directory = RoutingConnectionDirectory::default();
    directory.insert(RoutingConnectionInfo {
        connection_id: "local".to_string(),
        provider_id: "omniroute".to_string(),
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        authentication_supported: true,
        models: None,
    });
    let status = directory
        .validate_assignment(&assignment("local", "omniroute", "unknown/model"))
        .expect("unverified");
    assert_eq!(status, RoutingResolutionStatus::ModelUnverified);
    directory.insert(RoutingConnectionInfo {
        connection_id: "known".to_string(),
        provider_id: "omniroute".to_string(),
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        authentication_supported: true,
        models: Some(vec!["known/model".to_string()]),
    });
    assert_eq!(
        directory.validate_assignment(&assignment("known", "wrong", "known/model")),
        Err(RoutingProfileError::ProviderMismatch)
    );
    assert_eq!(
        directory.validate_assignment(&assignment("known", "omniroute", "missing")),
        Err(RoutingProfileError::ModelNotFound)
    );
}

#[test]
fn active_role_resolves_to_provider_selection_without_credentials() {
    let mut registry = RoutingProfileRegistry::default();
    let profile = complete_profile();
    registry.insert(profile).expect("insert");
    let id = RoutingProfileId::new("default").expect("id");
    registry.activate(&id).expect("activate");
    let selection = registry
        .active()
        .expect("active")
        .resolve_role(RoutingRole::Planner)
        .expect("selection");
    assert_eq!(selection.connection_id, "omniroute-local");
    assert_eq!(selection.provider_id, "omniroute");
    assert_eq!(selection.model_id, "provider/model");
    assert_eq!(
        registry
            .active()
            .expect("active")
            .resolve_required_sequential_selections()
            .expect("required selections")
            .len(),
        4
    );
}

#[test]
fn codex_routing_requires_connected_exact_named_accounts() {
    let mut connected = CodexAccountProfileRegistry::default();
    connected
        .insert(codex_account(
            "codex-personal",
            CodexAccountProfileState::Connected,
        ))
        .expect("personal");
    connected
        .insert(codex_account(
            "codex-secondary",
            CodexAccountProfileState::Connected,
        ))
        .expect("secondary");
    let mut directory = RoutingConnectionDirectory::default();
    directory.add_codex(&connected);
    assert_eq!(
        directory.validate_assignment(&assignment("codex-personal", "codex", "model")),
        Ok(RoutingResolutionStatus::ModelUnverified)
    );
    assert_eq!(
        directory.validate_assignment(&assignment("codex-secondary", "codex", "model")),
        Ok(RoutingResolutionStatus::ModelUnverified)
    );
    assert_eq!(
        directory.validate_assignment(&assignment("codex-missing", "codex", "model")),
        Err(RoutingProfileError::UnknownConnection)
    );

    for state in [
        CodexAccountProfileState::Unconfigured,
        CodexAccountProfileState::Disabled,
        CodexAccountProfileState::ReauthenticationRequired,
    ] {
        let mut registry = CodexAccountProfileRegistry::default();
        registry
            .insert(codex_account("codex-account", state))
            .expect("account");
        let mut directory = RoutingConnectionDirectory::default();
        directory.add_codex(&registry);
        assert_eq!(
            directory.validate_assignment(&assignment("codex-account", "codex", "model")),
            Err(RoutingProfileError::UnvalidatedConnection)
        );
    }
}

#[test]
fn secret_sentinels_never_enter_profile_serialization_or_debug() {
    let mut profile = complete_profile();
    profile.description = Some("credential-sentinel prompt-sentinel output-sentinel".to_string());
    let debug = format!("{profile:?}");
    let serialized = serde_json::to_string(&profile).expect("serialize");
    for sentinel in [
        "api-key-sentinel",
        "access-token-sentinel",
        "refresh-token-sentinel",
    ] {
        assert!(!debug.contains(sentinel));
        assert!(!serialized.contains(sentinel));
    }
}
