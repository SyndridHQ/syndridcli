use super::InstalledPoolRoutingSnapshot;
use super::InstalledPoolStatus;
use super::PoolSetupSnapshot;
use super::PoolSummary;
use super::member_id_for;
use super::pool_tab;
use super::safe_endpoint;
use crate::chatwidget::tests::make_chatwidget_manual_with_sender;
use crate::cooldown_status::TuiProviderCooldownSnapshot;
use crate::legacy_core::AccountPoolError;
use crate::legacy_core::AccountPoolProviderFamily;
use crate::legacy_core::AccountPoolSelectionPolicy;
use crate::legacy_core::AccountPoolTarget;
use crate::legacy_core::CodexAccountProfileId;
use crate::legacy_core::NamedAccountPool;
use crate::legacy_core::NamedAccountPoolRegistry;
use crate::legacy_core::PoolId;
use crate::legacy_core::PoolMemberId;
use crate::legacy_core::PoolReadiness;
use crate::legacy_core::ProviderCooldownKey;
use crate::legacy_core::ProviderCooldownState;
use crate::legacy_core::ProviderFailureClass;
use crate::legacy_core::RoutingAssignment;
use crate::legacy_core::RoutingProfile;
use crate::legacy_core::RoutingProfileId;
use crate::legacy_core::RoutingRole;
use crate::pool_authority::PoolRegistryWriter;
use crate::pool_authority::TuiPoolAuthority;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

fn second_member_registry() -> NamedAccountPoolRegistry {
    let mut registry = sample_registry();
    let mut pool = registry
        .remove(&PoolId::new("codex-primary").unwrap())
        .unwrap();
    pool.members.push(super::AccountPoolMember {
        id: PoolMemberId::new("personal-backup").unwrap(),
        target: AccountPoolTarget::native_codex(CodexAccountProfileId::new("account-b").unwrap()),
    });
    registry.insert(pool).unwrap();
    registry
}

#[derive(Default)]
struct TestWriter {
    fail: AtomicBool,
    bytes: Mutex<Vec<u8>>,
}

impl PoolRegistryWriter for TestWriter {
    fn save(&self, _registry: &NamedAccountPoolRegistry) -> Result<(), AccountPoolError> {
        if self.fail.load(Ordering::Relaxed) {
            Err(AccountPoolError::AtomicWriteFailed)
        } else {
            *self.bytes.lock().unwrap() = b"candidate".to_vec();
            Ok(())
        }
    }
}

fn sample_registry() -> NamedAccountPoolRegistry {
    let mut registry = NamedAccountPoolRegistry::default();
    registry
        .insert(NamedAccountPool {
            id: PoolId::new("codex-primary").unwrap(),
            display_name: "Codex Primary".to_string(),
            provider_family: AccountPoolProviderFamily::NativeCodex,
            members: vec![super::AccountPoolMember {
                id: PoolMemberId::new("personal-main").unwrap(),
                target: AccountPoolTarget::native_codex(
                    CodexAccountProfileId::new("account-a").unwrap(),
                ),
            }],
            selection_policy: AccountPoolSelectionPolicy::ExplicitMember(
                PoolMemberId::new("personal-main").unwrap(),
            ),
        })
        .unwrap();
    registry
}

#[test]
fn pool_summary_preserves_exact_id_provider_selection_and_readiness() {
    let registry = sample_registry();
    let accounts = crate::legacy_core::CodexAccountProfileRegistry::default();
    let connections = crate::legacy_core::OmniRouteRegistry::default();
    let snapshot = PoolSetupSnapshot::from_registry(&registry, Some(&accounts), Some(&connections));
    assert_eq!(
        snapshot.summaries,
        vec![PoolSummary {
            id: PoolId::new("codex-primary").unwrap(),
            display_name: "Codex Primary".to_string(),
            provider: AccountPoolProviderFamily::NativeCodex,
            member_count: 1,
            selected: "personal-main".to_string(),
            is_round_robin: false,
            readiness: PoolReadiness::MissingAccountReference,
            available_target_count: 1,
            cooling_target_count: 0,
            earliest_recovery: None,
        }]
    );
}

fn captured_pool_snapshot(registry: &NamedAccountPoolRegistry) -> InstalledPoolRoutingSnapshot {
    let profile_id = RoutingProfileId::new("installed").unwrap();
    let mut profile = RoutingProfile::new(profile_id, "Installed", 1).unwrap();
    profile
        .assign(
            RoutingRole::Planner,
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
    InstalledPoolRoutingSnapshot::from_captured_routing(&profile, registry, 7)
}

#[test]
fn installed_pool_status_uses_canonical_fingerprint_and_ignores_display_name() {
    let registry = sample_registry();
    let installed = captured_pool_snapshot(&registry);
    assert_eq!(installed.generation, 7);
    assert_eq!(
        installed.status_for_pool(&PoolId::new("codex-primary").unwrap(), &registry),
        InstalledPoolStatus::Current
    );

    let mut renamed = registry.clone();
    let pool_id = PoolId::new("codex-primary").unwrap();
    let mut pool = renamed.remove(&pool_id).unwrap();
    pool.display_name = "Renamed only".to_string();
    renamed.insert(pool).unwrap();
    assert_eq!(
        installed.status_for_pool(&pool_id, &renamed),
        InstalledPoolStatus::Current
    );

    let mut changed = renamed.clone();
    let mut pool = changed.remove(&pool_id).unwrap();
    pool.selection_policy = AccountPoolSelectionPolicy::RoundRobin;
    changed.insert(pool).unwrap();
    assert_eq!(
        installed.status_for_pool(&pool_id, &changed),
        InstalledPoolStatus::ReapplyRouting
    );

    let mut added = registry.clone();
    let mut pool = added.remove(&pool_id).unwrap();
    pool.members.push(super::AccountPoolMember {
        id: PoolMemberId::new("personal-backup").unwrap(),
        target: AccountPoolTarget::native_codex(CodexAccountProfileId::new("account-b").unwrap()),
    });
    added.insert(pool).unwrap();
    assert_eq!(
        installed.status_for_pool(&pool_id, &added),
        InstalledPoolStatus::ReapplyRouting
    );

    let mut retargeted = registry;
    let mut pool = retargeted.remove(&pool_id).unwrap();
    pool.members[0].target =
        AccountPoolTarget::native_codex(CodexAccountProfileId::new("account-b").unwrap());
    retargeted.insert(pool).unwrap();
    assert_eq!(
        installed.status_for_pool(&pool_id, &retargeted),
        InstalledPoolStatus::ReapplyRouting
    );

    let selected_member_change = second_member_registry();
    let installed_with_two_members = captured_pool_snapshot(&selected_member_change);
    let mut selected_member_changed = selected_member_change;
    let mut pool = selected_member_changed.remove(&pool_id).unwrap();
    pool.selection_policy =
        AccountPoolSelectionPolicy::ExplicitMember(PoolMemberId::new("personal-backup").unwrap());
    selected_member_changed.insert(pool).unwrap();
    assert_eq!(
        installed_with_two_members.status_for_pool(&pool_id, &selected_member_changed),
        InstalledPoolStatus::ReapplyRouting
    );
}

#[test]
fn installed_pool_status_distinguishes_unrouted_and_missing_saved_pools() {
    let registry = sample_registry();
    let installed = captured_pool_snapshot(&registry);
    let other_id = PoolId::new("other").unwrap();
    assert_eq!(
        installed.status_for_pool(&other_id, &registry),
        InstalledPoolStatus::NotCurrentlyRouted
    );

    let empty = NamedAccountPoolRegistry::default();
    assert_eq!(
        installed.status_for_pool(&PoolId::new("codex-primary").unwrap(), &empty),
        InstalledPoolStatus::InstalledSnapshotMissingFromRegistry
    );
}

#[test]
fn pool_list_snapshot_is_bounded_and_explicit() {
    let snapshot = PoolSetupSnapshot::from_registry(
        &sample_registry(),
        Some(&crate::legacy_core::CodexAccountProfileRegistry::default()),
        Some(&crate::legacy_core::OmniRouteRegistry::default()),
    );
    let (_, items) = pool_tab(&snapshot);
    let summary = items
        .iter()
        .map(|item| {
            format!(
                "{} — {}",
                item.name,
                item.description.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(summary);
    assert!(summary.contains("Explicit member"));
}

#[test]
fn pool_list_reports_current_cooldown_eligibility_without_cursor_data() {
    let registry = second_member_registry();
    let target = AccountPoolTarget::native_codex(CodexAccountProfileId::new("account-a").unwrap());
    let now = Instant::now();
    let mut state = ProviderCooldownState::new();
    state
        .record_cooldown(
            ProviderCooldownKey::new(target),
            ProviderFailureClass::RateLimited,
            Duration::from_secs(28),
            now,
        )
        .unwrap();
    let cooldowns = TuiProviderCooldownSnapshot::from_state(&state, now);
    let snapshot = PoolSetupSnapshot::from_registry_with_cooldowns(
        &registry,
        Some(&crate::legacy_core::CodexAccountProfileRegistry::default()),
        Some(&crate::legacy_core::OmniRouteRegistry::default()),
        &cooldowns,
    );
    let (_, items) = pool_tab(&snapshot);
    let rendered = items
        .iter()
        .map(|item| item.description.as_deref().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(
        rendered,
        @r###"Native Codex · 2 members · Explicit member · ID codex-primary · Runtime Not currently routed · 1 available · 1 cooling
Choose a stable pool ID, provider, members, and explicit selection.
Discard all pool edits."###
    );
    assert!(rendered.contains("1 available · 1 cooling"));
    assert!(!rendered.contains("cursor"));
    assert!(!rendered.contains("next"));
}

#[test]
fn pool_list_reports_earliest_recovery_when_all_unique_targets_are_cooling() {
    let registry = second_member_registry();
    let now = Instant::now();
    let mut state = ProviderCooldownState::new();
    for (id, seconds) in [("account-a", 28), ("account-b", 8)] {
        let target = AccountPoolTarget::native_codex(CodexAccountProfileId::new(id).unwrap());
        state
            .record_cooldown(
                ProviderCooldownKey::new(target),
                ProviderFailureClass::ProviderUnavailable,
                Duration::from_secs(seconds),
                now,
            )
            .unwrap();
    }
    let cooldowns = TuiProviderCooldownSnapshot::from_state(&state, now);
    let snapshot = PoolSetupSnapshot::from_registry_with_cooldowns(
        &registry,
        Some(&crate::legacy_core::CodexAccountProfileRegistry::default()),
        Some(&crate::legacy_core::OmniRouteRegistry::default()),
        &cooldowns,
    );
    let (_, items) = pool_tab(&snapshot);
    let rendered = items[0].description.as_deref().unwrap_or_default();
    assert!(rendered.contains("All targets cooling · Earliest recovery 8s"));
}

#[test]
fn round_robin_pool_is_displayed_as_active_and_not_flattened() {
    let mut registry = sample_registry();
    let pool_id = PoolId::new("codex-primary").unwrap();
    let mut pool = registry.remove(&pool_id).unwrap();
    pool.selection_policy = AccountPoolSelectionPolicy::RoundRobin;
    registry.insert(pool).unwrap();
    let snapshot = PoolSetupSnapshot::from_registry(
        &registry,
        Some(&crate::legacy_core::CodexAccountProfileRegistry::default()),
        Some(&crate::legacy_core::OmniRouteRegistry::default()),
    );
    assert_eq!(snapshot.summaries[0].selected, "Round robin");
    assert_eq!(
        snapshot.summaries[0].readiness,
        crate::legacy_core::PoolReadiness::MissingAccountReference
    );
    let (_, items) = pool_tab(&snapshot);
    let rendered = items
        .iter()
        .map(|item| item.name.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Needs attention"));
    assert!(!rendered.contains("selected personal-main"));
}

#[test]
fn proposed_member_ids_are_stable_and_redacted() {
    let registry = sample_registry();
    let target = AccountPoolTarget::native_codex(CodexAccountProfileId::new("account-a").unwrap());
    assert_eq!(
        member_id_for(&target, &registry).as_str(),
        "account-account-a"
    );
    assert_eq!(
        safe_endpoint("https://example.test/v1?api_key=secret"),
        "https://example.test/v1"
    );
}

#[tokio::test]
async fn candidate_pool_edits_preserve_exact_targets_and_explicit_selection() {
    let (mut widget, _, _, _) = make_chatwidget_manual_with_sender().await;
    let pool_id = PoolId::new("candidate").unwrap();
    widget.set_pool_setup_candidate(NamedAccountPoolRegistry::default());
    widget.set_pool_creation_id(pool_id.clone());
    widget.set_pool_creation_name("Candidate".to_string());
    widget.set_pool_creation_provider(AccountPoolProviderFamily::NativeCodex);
    let account_a = AccountPoolTarget::native_codex(CodexAccountProfileId::new("a").unwrap());
    let account_b = AccountPoolTarget::native_codex(CodexAccountProfileId::new("b").unwrap());
    widget
        .add_pool_member_candidate(&pool_id, account_a.clone())
        .unwrap();
    widget
        .add_pool_member_candidate(&pool_id, account_b.clone())
        .unwrap();
    let member_a = PoolMemberId::new("account-a").unwrap();
    let member_b = PoolMemberId::new("account-b").unwrap();
    widget
        .select_pool_member_candidate(&pool_id, &member_b)
        .unwrap();
    widget
        .rename_pool_candidate(&pool_id, "Renamed".to_string())
        .unwrap();
    let candidate = widget.pool_setup_candidate().unwrap();
    let pool = candidate.get(&pool_id).unwrap();
    assert_eq!(pool.display_name, "Renamed");
    assert_eq!(pool.members[0].target, account_a);
    assert_eq!(pool.members[1].target, account_b);
    assert_eq!(
        pool.selection_policy,
        AccountPoolSelectionPolicy::ExplicitMember(member_b.clone())
    );
    assert!(
        widget
            .remove_pool_member_candidate(&pool_id, &member_b)
            .is_err()
    );
    widget
        .remove_pool_member_candidate(&pool_id, &member_a)
        .unwrap();
    let pool = widget
        .pool_setup_candidate()
        .unwrap()
        .get(&pool_id)
        .unwrap()
        .clone();
    assert_eq!(pool.members.len(), 1);
    assert_eq!(
        pool.selection_policy,
        AccountPoolSelectionPolicy::ExplicitMember(member_b)
    );
}

#[tokio::test]
async fn policy_transitions_require_explicit_member_confirmation() {
    let (mut widget, _, _, _) = make_chatwidget_manual_with_sender().await;
    let pool_id = PoolId::new("codex-primary").unwrap();
    widget.set_pool_setup_candidate(sample_registry());

    widget
        .select_pool_policy_candidate(&pool_id, AccountPoolSelectionPolicy::RoundRobin)
        .unwrap();
    let round_robin = widget
        .pool_setup_candidate()
        .unwrap()
        .get(&pool_id)
        .unwrap()
        .clone();
    assert_eq!(
        round_robin.selection_policy,
        AccountPoolSelectionPolicy::RoundRobin
    );
    assert!(!widget.pool_policy_needs_member(&pool_id));

    widget
        .select_pool_policy_candidate(
            &pool_id,
            AccountPoolSelectionPolicy::ExplicitMember(PoolMemberId::new("ignored").unwrap()),
        )
        .unwrap();
    assert!(widget.pool_policy_needs_member(&pool_id));
    assert_eq!(
        widget
            .pool_setup_candidate()
            .unwrap()
            .get(&pool_id)
            .unwrap()
            .selection_policy,
        AccountPoolSelectionPolicy::RoundRobin
    );

    let member_id = PoolMemberId::new("personal-main").unwrap();
    widget
        .select_pool_member_candidate(&pool_id, &member_id)
        .unwrap();
    assert!(!widget.pool_policy_needs_member(&pool_id));
    assert_eq!(
        widget
            .pool_setup_candidate()
            .unwrap()
            .get(&pool_id)
            .unwrap()
            .selection_policy,
        AccountPoolSelectionPolicy::ExplicitMember(member_id)
    );
}

#[tokio::test]
async fn round_robin_member_management_preserves_nonempty_pool() {
    let (mut widget, _, _, _) = make_chatwidget_manual_with_sender().await;
    let pool_id = PoolId::new("codex-primary").unwrap();
    widget.set_pool_setup_candidate(second_member_registry());
    widget
        .select_pool_policy_candidate(&pool_id, AccountPoolSelectionPolicy::RoundRobin)
        .unwrap();

    let first = PoolMemberId::new("personal-main").unwrap();
    let second = PoolMemberId::new("personal-backup").unwrap();
    widget
        .remove_pool_member_candidate(&pool_id, &first)
        .unwrap();
    assert!(
        widget
            .remove_pool_member_candidate(&pool_id, &second)
            .is_err()
    );
    assert_eq!(
        widget
            .pool_setup_candidate()
            .unwrap()
            .get(&pool_id)
            .unwrap()
            .members
            .len(),
        1
    );
}

#[tokio::test]
async fn candidate_cancel_does_not_change_active_registry() {
    let (mut widget, _, _, _) = make_chatwidget_manual_with_sender().await;
    let active = sample_registry();
    widget.set_pool_setup_candidate(active.clone());
    widget
        .rename_pool_candidate(
            &PoolId::new("codex-primary").unwrap(),
            "Changed".to_string(),
        )
        .unwrap();
    assert_ne!(widget.pool_setup_candidate().unwrap(), active);
    widget.clear_pool_setup_candidate();
    assert_eq!(widget.pool_setup_candidate(), None);
    assert_eq!(
        active
            .get(&PoolId::new("codex-primary").unwrap())
            .unwrap()
            .display_name,
        "Codex Primary"
    );
}

#[test]
fn save_failure_does_not_publish_candidate_registry() {
    let writer = Arc::new(TestWriter {
        fail: AtomicBool::new(true),
        bytes: Mutex::new(b"previous bytes".to_vec()),
    });
    let authority = TuiPoolAuthority::for_test(sample_registry(), writer.clone());
    let mut candidate = second_member_registry();
    assert_eq!(
        authority.save(&candidate),
        Err(AccountPoolError::AtomicWriteFailed)
    );
    assert_eq!(*writer.bytes.lock().unwrap(), b"previous bytes");
    assert_eq!(authority.candidate(), Some(sample_registry()));
    candidate.remove(&PoolId::new("codex-primary").unwrap());
    assert_eq!(
        authority.save(&candidate),
        Err(AccountPoolError::AtomicWriteFailed)
    );
    assert_eq!(*writer.bytes.lock().unwrap(), b"previous bytes");
    assert_eq!(authority.candidate(), Some(sample_registry()));
}

#[test]
fn successful_save_publishes_complete_candidate_registry() {
    let authority = TuiPoolAuthority::for_test(sample_registry(), Arc::new(TestWriter::default()));
    let candidate = second_member_registry();
    authority.save(&candidate).unwrap();
    assert_eq!(authority.candidate(), Some(candidate));
}

#[test]
fn invalid_pool_file_is_preserved_until_explicit_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(crate::legacy_core::ACCOUNT_POOL_FILE);
    std::fs::write(&path, b"not json").unwrap();
    let authority = TuiPoolAuthority::load(directory.path(), None, None);
    assert!(authority.load_error().is_some());
    assert_eq!(std::fs::read(&path).unwrap(), b"not json");
    assert!(
        authority
            .save(&NamedAccountPoolRegistry::default())
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"not json");
}

#[tokio::test]
async fn connection_members_are_exact_and_mixed_provider_additions_are_rejected() {
    let (mut widget, _, _, _) = make_chatwidget_manual_with_sender().await;
    let pool_id = PoolId::new("omni").unwrap();
    widget.set_pool_setup_candidate(NamedAccountPoolRegistry::default());
    widget.set_pool_creation_id(pool_id.clone());
    widget.set_pool_creation_name("Omni".to_string());
    widget.set_pool_creation_provider(AccountPoolProviderFamily::OmniRoute);
    let connection = AccountPoolTarget::omniroute("connection-a".to_string()).unwrap();
    widget
        .add_pool_member_candidate(&pool_id, connection.clone())
        .unwrap();
    let pool = widget
        .pool_setup_candidate()
        .unwrap()
        .get(&pool_id)
        .unwrap()
        .clone();
    assert_eq!(pool.members[0].target, connection);
    assert!(
        widget
            .add_pool_member_candidate(
                &pool_id,
                AccountPoolTarget::native_codex(CodexAccountProfileId::new("account-a").unwrap())
            )
            .is_err()
    );
}

#[tokio::test]
async fn deleting_a_candidate_pool_leaves_other_pools_unchanged() {
    let (mut widget, _, _, _) = make_chatwidget_manual_with_sender().await;
    let mut second = second_member_registry();
    let other_id = PoolId::new("other").unwrap();
    let mut other = second
        .get(&PoolId::new("codex-primary").unwrap())
        .unwrap()
        .clone();
    other.id = other_id.clone();
    second.insert(other).unwrap();
    widget.set_pool_setup_candidate(second.clone());
    widget
        .delete_pool_candidate(&PoolId::new("codex-primary").unwrap())
        .unwrap();
    let remaining = widget.pool_setup_candidate().unwrap();
    assert_eq!(remaining.pools().count(), 1);
    assert_eq!(remaining.get(&other_id), second.get(&other_id));
}
