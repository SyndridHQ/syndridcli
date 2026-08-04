use super::account_pools::AccountPoolMember;
use super::account_pools::AccountPoolProviderFamily;
use super::account_pools::AccountPoolSelectionPolicy;
use super::account_pools::AccountPoolTarget;
use super::account_pools::NamedAccountPool;
use super::account_pools::NamedAccountPoolRegistry;
use super::account_pools::PoolId;
use super::account_pools::PoolMemberId;
use super::codex_accounts::CodexAccountConnectionMetadata;
use super::codex_accounts::CodexAccountProfileId;
use super::codex_accounts::CodexAccountProfileRegistry;
use super::codex_accounts::CodexAccountProfileState;
use super::omniroute::OMNIROUTE_DEFAULT_BASE_URL;
use super::omniroute::OMNIROUTE_PROVIDER_ID;
use super::omniroute::OmniRouteConnectionMetadata;
use super::omniroute::OmniRouteRegistry;
use super::provider_connection::ConnectionValidationResult;
use super::provider_connection::ConnectionValidationStatus;
use super::routing_pool_bindings::RoutingPoolResolutionError;
use super::routing_pool_bindings::resolve_routing_profile;
use super::routing_profiles::RoutingAssignment;
use super::routing_profiles::RoutingProfile;
use super::routing_profiles::RoutingProfileError;
use super::routing_profiles::RoutingProfileId;
use super::routing_profiles::RoutingRole;
use pretty_assertions::assert_eq;
use tempfile::tempdir;

fn accounts() -> CodexAccountProfileRegistry {
    accounts_with(&["account-a1", "account-a2"])
}

fn accounts_with(ids: &[&str]) -> CodexAccountProfileRegistry {
    let mut accounts = CodexAccountProfileRegistry::default();
    for &id in ids {
        let profile_id = if id == "account-a2" { "profile-a2" } else { id };
        accounts
            .insert(CodexAccountConnectionMetadata {
                connection_id: id.to_string(),
                profile_id: CodexAccountProfileId::new(profile_id).unwrap(),
                provider_id: "codex".to_string(),
                label: id.to_string(),
                state: CodexAccountProfileState::Connected,
                account_email: None,
                account_id: None,
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

fn pools() -> NamedAccountPoolRegistry {
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
                        CodexAccountProfileId::new("profile-a2").unwrap(),
                    ),
                },
            ],
            selection_policy: AccountPoolSelectionPolicy::ExplicitMember(
                PoolMemberId::new("member-a2").unwrap(),
            ),
        })
        .unwrap();
    pools
}

fn omni_route() -> OmniRouteRegistry {
    let mut registry = OmniRouteRegistry::default();
    for id in ["connection-c1", "connection-c2"] {
        registry
            .insert(OmniRouteConnectionMetadata {
                connection_id: id.to_string(),
                provider_id: OMNIROUTE_PROVIDER_ID.to_string(),
                label: id.to_string(),
                base_url: OMNIROUTE_DEFAULT_BASE_URL.to_string(),
                credential_reference: format!("credential-{id}"),
                enabled: true,
                validation: ConnectionValidationResult::valid(),
                models: vec!["model".to_string()],
                validated_at: Some(1),
            })
            .unwrap();
    }
    registry
}

fn omni_pool() -> NamedAccountPoolRegistry {
    let mut pools = NamedAccountPoolRegistry::default();
    pools
        .insert(NamedAccountPool {
            id: PoolId::new("omni-primary").unwrap(),
            display_name: "Omni primary".to_string(),
            provider_family: AccountPoolProviderFamily::OmniRoute,
            members: vec![
                AccountPoolMember {
                    id: PoolMemberId::new("member-c1").unwrap(),
                    target: AccountPoolTarget::omniroute("connection-c1").unwrap(),
                },
                AccountPoolMember {
                    id: PoolMemberId::new("member-c2").unwrap(),
                    target: AccountPoolTarget::omniroute("connection-c2").unwrap(),
                },
            ],
            selection_policy: AccountPoolSelectionPolicy::ExplicitMember(
                PoolMemberId::new("member-c1").unwrap(),
            ),
        })
        .unwrap();
    pools
}

fn profile(pool_id: Option<PoolId>, provider_id: &str) -> RoutingProfile {
    let mut profile = RoutingProfile::new(
        RoutingProfileId::new("pool-bound").unwrap(),
        "Pool bound",
        1,
    )
    .unwrap();
    profile
        .assign(
            RoutingRole::Planner,
            RoutingAssignment {
                connection_id: pool_id
                    .is_none()
                    .then(|| "direct-account".to_string())
                    .unwrap_or_default(),
                provider_id: provider_id.to_string(),
                model_id: "planner-model".to_string(),
                enabled: true,
                label: Some("planner-label".to_string()),
                pool_id,
            },
        )
        .unwrap();
    profile
}

#[test]
fn codex_pool_resolution_preserves_exact_identity_and_assignment_metadata() {
    let resolved = resolve_routing_profile(
        &profile(Some(PoolId::new("codex-primary").unwrap()), "codex"),
        &pools(),
        &accounts(),
        &OmniRouteRegistry::default(),
    )
    .unwrap();
    let assignment = resolved.assignments.get(&RoutingRole::Planner).unwrap();
    assert_eq!(assignment.connection_id, "account-a2");
    assert_eq!(assignment.provider_id, "codex");
    assert_eq!(assignment.model_id, "planner-model");
    assert_eq!(assignment.label.as_deref(), Some("planner-label"));
    assert_eq!(assignment.pool_id, None);
}

#[test]
fn direct_assignment_is_preserved_without_pool_resolution() {
    let source = profile(None, "codex");
    assert_eq!(
        resolve_routing_profile(
            &source,
            &pools(),
            &accounts(),
            &OmniRouteRegistry::default()
        ),
        Ok(source)
    );
}

#[test]
fn omniroute_pool_resolution_preserves_exact_connection() {
    let resolved = resolve_routing_profile(
        &profile(
            Some(PoolId::new("omni-primary").unwrap()),
            OMNIROUTE_PROVIDER_ID,
        ),
        &omni_pool(),
        &CodexAccountProfileRegistry::default(),
        &omni_route(),
    )
    .unwrap();
    let assignment = resolved.assignments.get(&RoutingRole::Planner).unwrap();
    assert_eq!(assignment.connection_id, "connection-c1");
    assert_eq!(assignment.pool_id, None);
}

#[test]
fn selected_stale_member_fails_without_using_another_member() {
    let accounts = accounts_with(&["account-a1"]);
    let error = resolve_routing_profile(
        &profile(Some(PoolId::new("codex-primary").unwrap()), "codex"),
        &pools(),
        &accounts,
        &OmniRouteRegistry::default(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        RoutingPoolResolutionError::Pool {
            pool_id: PoolId::new("codex-primary").unwrap(),
            error: super::account_pools::PoolResolutionError::MissingAccountReference,
        }
    );
}

#[test]
fn nonselected_stale_member_does_not_block_selected_resolution() {
    let accounts = accounts_with(&["account-a2"]);
    let resolved = resolve_routing_profile(
        &profile(Some(PoolId::new("codex-primary").unwrap()), "codex"),
        &pools(),
        &accounts,
        &OmniRouteRegistry::default(),
    )
    .unwrap();
    assert_eq!(
        resolved
            .assignments
            .get(&RoutingRole::Planner)
            .unwrap()
            .connection_id,
        "account-a2"
    );
}

#[test]
fn missing_pool_and_provider_mismatch_are_typed_failures() {
    assert!(matches!(
        resolve_routing_profile(
            &profile(Some(PoolId::new("missing").unwrap()), "codex"),
            &pools(),
            &accounts(),
            &OmniRouteRegistry::default(),
        ),
        Err(RoutingPoolResolutionError::Pool { .. })
    ));
    assert_eq!(
        resolve_routing_profile(
            &profile(Some(PoolId::new("codex-primary").unwrap()), "omniroute"),
            &pools(),
            &accounts(),
            &OmniRouteRegistry::default(),
        ),
        Err(RoutingPoolResolutionError::ProviderFamilyMismatch {
            pool_id: PoolId::new("codex-primary").unwrap(),
        })
    );
}

#[test]
fn direct_and_pool_identity_is_rejected() {
    let assignment = RoutingAssignment {
        connection_id: "direct".to_string(),
        provider_id: "codex".to_string(),
        model_id: "model".to_string(),
        enabled: true,
        label: None,
        pool_id: Some(PoolId::new("codex-primary").unwrap()),
    };
    let mut profile =
        RoutingProfile::new(RoutingProfileId::new("ambiguous").unwrap(), "Ambiguous", 1).unwrap();
    assert_eq!(
        profile.assign(RoutingRole::Planner, assignment),
        Err(RoutingProfileError::InvalidAssignment)
    );
}

#[test]
fn deserialization_rejects_direct_and_pool_identity_together() {
    let source = profile(Some(PoolId::new("codex-primary").unwrap()), "codex");
    let mut registry = super::routing_profiles::RoutingProfileRegistry::default();
    let profile_id = source.id.clone();
    registry.insert(source).unwrap();
    registry.active_profile_id = Some(profile_id);
    let mut raw = serde_json::to_value(&registry).unwrap();
    raw["profiles"]["pool-bound"]["assignments"]["planner"]["connection_id"] =
        serde_json::Value::String("direct-account".to_string());
    let directory = tempdir().unwrap();
    let path = directory.path().join("syndrid-routing-profiles.json");
    std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();

    assert_eq!(
        super::routing_profiles::RoutingProfileRegistry::load(&path),
        Err(RoutingProfileError::InvalidAssignment)
    );
}

#[test]
fn pool_reference_round_trips_without_pool_members_or_resolved_identity() {
    let source = profile(Some(PoolId::new("codex-primary").unwrap()), "codex");
    let mut registry = super::routing_profiles::RoutingProfileRegistry::default();
    let profile_id = source.id.clone();
    registry.insert(source.clone()).unwrap();
    registry.active_profile_id = Some(profile_id);
    let directory = tempdir().unwrap();
    let path = directory.path().join("syndrid-routing-profiles.json");
    registry.save(&path).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("\"pool_id\": \"codex-primary\""));
    assert!(!raw.contains("member-a2"));
    assert!(!raw.contains("account-a2"));
    assert_eq!(
        super::routing_profiles::RoutingProfileRegistry::load(&path).unwrap(),
        registry
    );
}
