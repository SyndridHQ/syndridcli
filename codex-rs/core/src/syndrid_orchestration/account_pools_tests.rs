use super::*;
use crate::syndrid_orchestration::codex_accounts::CodexAccountConnectionMetadata;
use crate::syndrid_orchestration::codex_accounts::CodexAccountProfileState;
use crate::syndrid_orchestration::omniroute::OmniRouteRegistry;
use crate::syndrid_orchestration::provider_connection::ConnectionValidationStatus;
use pretty_assertions::assert_eq;
use tempfile::tempdir;

fn native_id(value: &str) -> crate::syndrid_orchestration::codex_accounts::CodexAccountProfileId {
    crate::syndrid_orchestration::codex_accounts::CodexAccountProfileId::new(value).unwrap()
}

fn native_registry(
    ids: &[&str],
) -> crate::syndrid_orchestration::codex_accounts::CodexAccountProfileRegistry {
    let mut registry =
        crate::syndrid_orchestration::codex_accounts::CodexAccountProfileRegistry::default();
    for id in ids {
        registry
            .insert(CodexAccountConnectionMetadata {
                connection_id: format!("connection-{id}"),
                profile_id: native_id(id),
                provider_id: "codex".to_string(),
                label: format!("Account {id}"),
                state: CodexAccountProfileState::Connected,
                account_email: None,
                account_id: None,
                plan_label: None,
                enabled: true,
                validation: ConnectionValidationStatus::Valid,
                last_authenticated_at: None,
                last_validated_at: None,
                credential_reference: format!("codex-account-connection-{id}"),
                schema_version: 1,
            })
            .unwrap();
    }
    registry
}

fn native_pool(selected: &str) -> NamedAccountPool {
    let members = ["a1", "a2"]
        .into_iter()
        .map(|id| AccountPoolMember {
            id: PoolMemberId::new(id).unwrap(),
            target: AccountPoolTarget::native_codex(native_id(id)),
        })
        .collect();
    NamedAccountPool {
        id: PoolId::new("codex-primary").unwrap(),
        display_name: "Codex primary".to_string(),
        provider_family: AccountPoolProviderFamily::NativeCodex,
        members,
        selection_policy: AccountPoolSelectionPolicy::ExplicitMember(
            PoolMemberId::new(selected).unwrap(),
        ),
    }
}

fn native_pool_with_id(id: &str, selected: &str) -> NamedAccountPool {
    let mut pool = native_pool(selected);
    pool.id = PoolId::new(id).unwrap();
    pool
}

fn round_robin_pool(ids: &[&str]) -> NamedAccountPool {
    NamedAccountPool {
        id: PoolId::new("round-robin").unwrap(),
        display_name: "Round robin".to_string(),
        provider_family: AccountPoolProviderFamily::NativeCodex,
        members: ids
            .iter()
            .map(|id| AccountPoolMember {
                id: PoolMemberId::new(*id).unwrap(),
                target: AccountPoolTarget::native_codex(native_id(id)),
            })
            .collect(),
        selection_policy: AccountPoolSelectionPolicy::RoundRobin,
    }
}

#[test]
fn missing_file_is_an_empty_registry() {
    let directory = tempdir().unwrap();
    let registry =
        NamedAccountPoolRegistry::load(&directory.path().join(ACCOUNT_POOL_FILE)).unwrap();
    assert_eq!(registry.pools().count(), 0);
}

#[test]
fn structural_validation_rejects_duplicates_empty_and_mixed_pools() {
    assert_eq!(PoolId::new(""), Err(AccountPoolError::InvalidPoolId));
    assert_eq!(
        PoolId::new("not valid"),
        Err(AccountPoolError::InvalidPoolId)
    );
    assert_eq!(
        PoolMemberId::new("m".repeat(129)),
        Err(AccountPoolError::InvalidMemberId)
    );

    let mut registry = NamedAccountPoolRegistry::default();
    registry.insert(native_pool("a2")).unwrap();
    assert_eq!(
        registry.insert(native_pool("a2")),
        Err(AccountPoolError::DuplicatePoolId)
    );

    let empty = NamedAccountPool {
        id: PoolId::new("empty").unwrap(),
        display_name: "Empty".to_string(),
        provider_family: AccountPoolProviderFamily::NativeCodex,
        members: Vec::new(),
        selection_policy: AccountPoolSelectionPolicy::ExplicitMember(
            PoolMemberId::new("m").unwrap(),
        ),
    };
    assert_eq!(registry.insert(empty), Err(AccountPoolError::EmptyPool));

    let duplicate_member = NamedAccountPool {
        id: PoolId::new("duplicate-members").unwrap(),
        display_name: "Duplicate members".to_string(),
        provider_family: AccountPoolProviderFamily::NativeCodex,
        members: vec![
            AccountPoolMember {
                id: PoolMemberId::new("same").unwrap(),
                target: AccountPoolTarget::native_codex(native_id("a1")),
            },
            AccountPoolMember {
                id: PoolMemberId::new("same").unwrap(),
                target: AccountPoolTarget::native_codex(native_id("a2")),
            },
        ],
        selection_policy: AccountPoolSelectionPolicy::ExplicitMember(
            PoolMemberId::new("same").unwrap(),
        ),
    };
    assert_eq!(
        registry.insert(duplicate_member),
        Err(AccountPoolError::DuplicateMemberId)
    );

    let mixed = NamedAccountPool {
        id: PoolId::new("mixed").unwrap(),
        display_name: "Mixed".to_string(),
        provider_family: AccountPoolProviderFamily::NativeCodex,
        members: vec![AccountPoolMember {
            id: PoolMemberId::new("m").unwrap(),
            target: AccountPoolTarget::omniroute("connection").unwrap(),
        }],
        selection_policy: AccountPoolSelectionPolicy::ExplicitMember(
            PoolMemberId::new("m").unwrap(),
        ),
    };
    assert_eq!(
        registry.insert(mixed),
        Err(AccountPoolError::ProviderFamilyMismatch)
    );

    let mut missing_selected = native_pool("a2");
    missing_selected.selection_policy =
        AccountPoolSelectionPolicy::ExplicitMember(PoolMemberId::new("missing").unwrap());
    assert_eq!(
        registry.insert(missing_selected),
        Err(AccountPoolError::SelectedMemberNotInPool)
    );

    let mut too_many_members = native_pool("a1");
    too_many_members.members = (0..33)
        .map(|index| AccountPoolMember {
            id: PoolMemberId::new(format!("member-{index}")).unwrap(),
            target: AccountPoolTarget::native_codex(native_id("a1")),
        })
        .collect();
    too_many_members.selection_policy =
        AccountPoolSelectionPolicy::ExplicitMember(PoolMemberId::new("member-0").unwrap());
    assert_eq!(
        registry.insert(too_many_members),
        Err(AccountPoolError::TooManyMembers)
    );

    let mut too_many_pools = NamedAccountPoolRegistry::default();
    for index in 0..32 {
        too_many_pools
            .insert(native_pool_with_id(&format!("pool-{index}"), "a1"))
            .unwrap();
    }
    assert_eq!(
        too_many_pools.insert(native_pool_with_id("pool-32", "a1")),
        Err(AccountPoolError::TooManyPools)
    );
}

#[test]
fn explicit_resolution_returns_exactly_the_selected_account() {
    let mut pools = NamedAccountPoolRegistry::default();
    pools.insert(native_pool("a2")).unwrap();
    let resolved = pools
        .resolve_pool(
            &PoolId::new("codex-primary").unwrap(),
            &native_registry(&["a1", "a2"]),
            &OmniRouteRegistry::default(),
        )
        .unwrap();
    assert_eq!(
        resolved.target,
        AccountPoolTarget::native_codex(native_id("a2"))
    );
}

#[test]
fn explicit_resolution_returns_exactly_the_selected_omniroute_connection() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("connections.json");
    std::fs::write(
        &path,
        r#"{"connections":{"c1":{"connection_id":"c1","provider_id":"omniroute","label":"One","base_url":"http://localhost:20128","credential_reference":"omni-c1","enabled":true,"validation":{"status":"valid","error":null},"models":["model"],"validated_at":null},"c2":{"connection_id":"c2","provider_id":"omniroute","label":"Two","base_url":"http://localhost:20128","credential_reference":"omni-c2","enabled":true,"validation":{"status":"valid","error":null},"models":["model"],"validated_at":null}}}"#,
    )
    .unwrap();
    let connections = OmniRouteRegistry::load(&path).unwrap();
    let pool = NamedAccountPool {
        id: PoolId::new("omni-primary").unwrap(),
        display_name: "Omni primary".to_string(),
        provider_family: AccountPoolProviderFamily::OmniRoute,
        members: vec![
            AccountPoolMember {
                id: PoolMemberId::new("one").unwrap(),
                target: AccountPoolTarget::omniroute("c1").unwrap(),
            },
            AccountPoolMember {
                id: PoolMemberId::new("two").unwrap(),
                target: AccountPoolTarget::omniroute("c2").unwrap(),
            },
        ],
        selection_policy: AccountPoolSelectionPolicy::ExplicitMember(
            PoolMemberId::new("one").unwrap(),
        ),
    };
    let mut pools = NamedAccountPoolRegistry::default();
    pools.insert(pool).unwrap();
    let resolved = pools
        .resolve_pool(
            &PoolId::new("omni-primary").unwrap(),
            &native_registry(&[]),
            &connections,
        )
        .unwrap();
    assert_eq!(resolved.target, AccountPoolTarget::omniroute("c1").unwrap());
}

#[test]
fn unavailable_selected_omniroute_connection_does_not_fall_back() {
    let pool = NamedAccountPool {
        id: PoolId::new("omni-primary").unwrap(),
        display_name: "Omni primary".to_string(),
        provider_family: AccountPoolProviderFamily::OmniRoute,
        members: vec![AccountPoolMember {
            id: PoolMemberId::new("one").unwrap(),
            target: AccountPoolTarget::omniroute("c1").unwrap(),
        }],
        selection_policy: AccountPoolSelectionPolicy::ExplicitMember(
            PoolMemberId::new("one").unwrap(),
        ),
    };
    let mut pools = NamedAccountPoolRegistry::default();
    pools.insert(pool).unwrap();
    assert_eq!(
        pools.resolve_pool(
            &PoolId::new("omni-primary").unwrap(),
            &native_registry(&[]),
            &OmniRouteRegistry::default(),
        ),
        Err(PoolResolutionError::MissingConnectionReference)
    );
}

#[test]
fn unavailable_selected_account_does_not_fall_back() {
    let mut pools = NamedAccountPoolRegistry::default();
    pools.insert(native_pool("a2")).unwrap();
    let error = pools
        .resolve_pool(
            &PoolId::new("codex-primary").unwrap(),
            &native_registry(&["a1"]),
            &OmniRouteRegistry::default(),
        )
        .unwrap_err();
    assert_eq!(error, PoolResolutionError::MissingAccountReference);
}

#[test]
fn round_robin_pool_is_not_resolved_as_an_explicit_member() {
    let mut pools = NamedAccountPoolRegistry::default();
    let pool = round_robin_pool(&["a1", "a2"]);
    let pool_id = pool.id.clone();
    pools.insert(pool).unwrap();
    assert_eq!(
        pools.resolve_pool(
            &pool_id,
            &native_registry(&["a1", "a2"]),
            &OmniRouteRegistry::default(),
        ),
        Err(PoolResolutionError::RoundRobinRequiresRuntimeSelection)
    );
    assert_eq!(
        pools.readiness(
            &native_registry(&["a1", "a2"]),
            &OmniRouteRegistry::default()
        )[&pool_id],
        PoolReadiness::RotationRequiresRuntimeSelection
    );
}

#[test]
fn serialization_is_deterministic_and_invalid_files_are_untouched() {
    let directory = tempdir().unwrap();
    let path = directory.path().join(ACCOUNT_POOL_FILE);
    let mut registry = NamedAccountPoolRegistry::default();
    registry.insert(native_pool("a2")).unwrap();
    registry.save(&path).unwrap();
    let first = std::fs::read(&path).unwrap();
    let loaded = NamedAccountPoolRegistry::load(&path).unwrap();
    loaded.save(&path).unwrap();
    assert_eq!(first, std::fs::read(&path).unwrap());

    let left_path = directory.path().join("left.json");
    let right_path = directory.path().join("right.json");
    let mut left = NamedAccountPoolRegistry::default();
    left.insert(native_pool_with_id("pool-b", "a2")).unwrap();
    left.insert(native_pool_with_id("pool-a", "a1")).unwrap();
    let mut right = NamedAccountPoolRegistry::default();
    right.insert(native_pool_with_id("pool-a", "a1")).unwrap();
    right.insert(native_pool_with_id("pool-b", "a2")).unwrap();
    left.save(&left_path).unwrap();
    right.save(&right_path).unwrap();
    assert_eq!(
        std::fs::read(left_path).unwrap(),
        std::fs::read(right_path).unwrap()
    );

    std::fs::write(&path, b"not json").unwrap();
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        NamedAccountPoolRegistry::load(&path),
        Err(AccountPoolError::RegistryMalformed)
    );
    assert_eq!(before, std::fs::read(&path).unwrap());
}

#[test]
fn round_robin_policy_round_trips_and_legacy_files_are_not_rewritten_on_load() {
    let directory = tempdir().unwrap();
    let path = directory.path().join(ACCOUNT_POOL_FILE);
    let mut registry = NamedAccountPoolRegistry::default();
    registry.insert(round_robin_pool(&["b", "a"])).unwrap();
    registry.save(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("round_robin"));
    assert_eq!(NamedAccountPoolRegistry::load(&path).unwrap(), registry);

    let legacy = br#"{"schema_version":1,"pools":[{"id":"legacy","display_name":"Legacy","provider_family":"native_codex","members":[{"id":"a","target":{"kind":"native_codex_account","account_profile_id":"a"}}],"selection_policy":{"kind":"explicit_member","member_id":"a"}}]}"#;
    std::fs::write(&path, legacy).unwrap();
    assert_eq!(
        NamedAccountPoolRegistry::load(&path)
            .unwrap()
            .pools()
            .count(),
        1
    );
    assert_eq!(std::fs::read(&path).unwrap(), legacy);
}

#[test]
#[cfg(unix)]
fn save_failure_preserves_previous_bytes_and_candidate_is_not_authoritative() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let path = directory.path().join(ACCOUNT_POOL_FILE);
    let mut original = NamedAccountPoolRegistry::default();
    original.insert(native_pool("a1")).unwrap();
    original.save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();

    let mut candidate = NamedAccountPoolRegistry::default();
    candidate.insert(native_pool("a2")).unwrap();
    let original_permissions = std::fs::metadata(directory.path()).unwrap().permissions();
    let mut read_only = original_permissions.clone();
    read_only.set_mode(0o555);
    std::fs::set_permissions(directory.path(), read_only).unwrap();
    let result = candidate.save(&path);
    std::fs::set_permissions(directory.path(), original_permissions).unwrap();

    assert_eq!(result, Err(AccountPoolError::AtomicWriteFailed));
    assert_eq!(before, std::fs::read(&path).unwrap());
    assert_eq!(original, NamedAccountPoolRegistry::load(&path).unwrap());
}

#[test]
fn oversized_and_unsupported_files_are_rejected_without_rewriting() {
    let directory = tempdir().unwrap();
    let path = directory.path().join(ACCOUNT_POOL_FILE);
    std::fs::write(&path, "{\"schema_version\":99,\"pools\":[]}").unwrap();
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        NamedAccountPoolRegistry::load(&path),
        Err(AccountPoolError::UnsupportedSchemaVersion)
    );
    assert_eq!(before, std::fs::read(&path).unwrap());

    let oversized = vec![b'x'; MAX_ACCOUNT_POOL_FILE_BYTES + 1];
    std::fs::write(&path, &oversized).unwrap();
    assert_eq!(
        NamedAccountPoolRegistry::load(&path),
        Err(AccountPoolError::RegistryTooLarge)
    );
    assert_eq!(oversized, std::fs::read(&path).unwrap());
}

#[test]
fn stale_nonselected_account_does_not_block_selected_resolution() {
    let mut pools = NamedAccountPoolRegistry::default();
    pools.insert(native_pool("a1")).unwrap();
    let accounts = native_registry(&["a1"]);
    let pool_id = PoolId::new("codex-primary").unwrap();
    assert_eq!(
        pools.readiness(&accounts, &OmniRouteRegistry::default())[&pool_id],
        PoolReadiness::Ready
    );
    let members = pools
        .member_readiness(&pool_id, &accounts, &OmniRouteRegistry::default())
        .unwrap();
    assert_eq!(
        members[&PoolMemberId::new("a1").unwrap()],
        PoolMemberReadiness::Ready
    );
    assert_eq!(
        members[&PoolMemberId::new("a2").unwrap()],
        PoolMemberReadiness::MissingAccountReference
    );
}
