use super::mode_entries;
use super::mode_label;
use super::parse_mode_argument;
use super::routing_role_items;
use super::selectable_provider_setup_items;
use super::strategy_entries;
use super::unavailable_reason;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::legacy_core::AccountPoolProviderFamily;
use crate::legacy_core::ExecutionModeSelection;
use crate::legacy_core::OrchestrationMode;
use crate::legacy_core::OrchestrationStrategyAvailability;
use crate::legacy_core::OrchestrationStrategyUnavailableReason;
use crate::legacy_core::PoolId;
use crate::legacy_core::PoolMemberReadiness;
use crate::legacy_core::PoolReadiness;
use crate::legacy_core::ResolvedOrchestrationPolicy;
use crate::legacy_core::RoutingAssignment;
use crate::legacy_core::RoutingProfile;
use crate::legacy_core::RoutingProfileId;
use crate::legacy_core::RoutingRole;
use crate::legacy_core::SessionExecutionPolicyState;
use crate::legacy_core::SessionPolicySource;
use crate::orchestration_setup::SetupReadinessState;
use crate::pool_setup::PoolSetupSnapshot;
use crate::pool_setup::PoolSummary;
use crate::provider_setup::ProviderSetupItem;
use crate::provider_setup::ProviderSetupSnapshot;
use crate::routing_role_setup::IdentitySourceChoice;
use crate::routing_role_setup::identity_source_items;
use crate::routing_role_setup::pool_selection_items;
use crate::routing_role_setup::set_identity_source;
use crate::routing_role_setup::set_pool_selection;
use tokio::sync::mpsc;

fn ready_item(name: &str, id: &str, provider_id: &str) -> ProviderSetupItem {
    ProviderSetupItem {
        name: name.to_string(),
        detail: "configured".to_string(),
        id: Some(id.to_string()),
        provider_id: Some(provider_id.to_string()),
        models: vec!["model-1".to_string()],
        readiness: SetupReadinessState::Ready,
    }
}

#[test]
fn selector_contains_only_the_five_phase_seven_f_modes() {
    let entries = mode_entries();

    assert_eq!(entries.len(), 5);
    assert_eq!(
        entries.iter().map(|entry| entry.name).collect::<Vec<_>>(),
        ["Fast", "Balanced", "Usage Saver", "Deep", "Custom",]
    );
    assert!(entries[1].selection == Some(ExecutionModeSelection::Balanced));
    assert!(entries[4].selection.is_none());
}

#[test]
fn canonical_labels_are_stable_and_balanced_is_default() {
    assert_eq!(mode_label(&ExecutionModeSelection::Fast), "Fast");
    assert_eq!(mode_label(&ExecutionModeSelection::Balanced), "Balanced");
    assert_eq!(
        mode_label(&ExecutionModeSelection::UsageSaver),
        "Usage Saver"
    );
    assert_eq!(mode_label(&ExecutionModeSelection::Deep), "Deep");
    assert_eq!(
        SessionExecutionPolicyState::new()
            .expect("Balanced is a valid O6E default")
            .selected_mode()
            .expect("new session state is readable"),
        ExecutionModeSelection::Balanced
    );
}

#[test]
fn direct_mode_aliases_map_to_o6e_types() {
    assert_eq!(
        parse_mode_argument("fast"),
        Some(ExecutionModeSelection::Fast)
    );
    assert_eq!(
        parse_mode_argument("balanced"),
        Some(ExecutionModeSelection::Balanced)
    );
    assert_eq!(
        parse_mode_argument("usage-saver"),
        Some(ExecutionModeSelection::UsageSaver)
    );
    assert_eq!(
        parse_mode_argument("usage_saver"),
        Some(ExecutionModeSelection::UsageSaver)
    );
    assert_eq!(
        parse_mode_argument("usagesaver"),
        Some(ExecutionModeSelection::UsageSaver)
    );
    assert_eq!(
        parse_mode_argument("deep"),
        Some(ExecutionModeSelection::Deep)
    );
    assert_eq!(parse_mode_argument("custom"), None);
    assert_eq!(parse_mode_argument("unknown"), None);
    assert_eq!(parse_mode_argument(""), None);
}

#[test]
fn pending_mode_uses_phase_seven_a_policy_state() {
    let state = SessionExecutionPolicyState::new().expect("Balanced is a valid default");
    state
        .select_mode(
            ExecutionModeSelection::Deep,
            SessionPolicySource::ExplicitUserSelection,
        )
        .expect("idle selection should succeed");
    assert_eq!(
        state.selected_mode().expect("selection is readable"),
        ExecutionModeSelection::Deep
    );

    assert_eq!(
        state.selected_mode().expect("captured mode is readable"),
        ExecutionModeSelection::Deep
    );
    assert_eq!(
        state
            .resolved_policy()
            .expect("policy is readable")
            .selected_mode(),
        &ExecutionModeSelection::Deep
    );
}

#[test]
fn selector_contains_all_canonical_strategy_options() {
    assert_eq!(
        strategy_entries()
            .iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>(),
        ["Single", "Manual", "Recommended", "Automatic", "Adaptive"]
    );
}

#[test]
fn unavailable_strategy_copy_preserves_canonical_reasons() {
    assert_eq!(
        unavailable_reason(OrchestrationStrategyUnavailableReason::AutomaticSelectorUnavailable),
        "Automatic is unavailable because automatic workflow selection is not implemented yet."
    );
    assert_eq!(
        ResolvedOrchestrationPolicy::resolve(
            OrchestrationMode::Adaptive,
            ExecutionModeSelection::Fast,
        )
        .expect("adaptive policy resolves as unavailable")
        .availability(),
        OrchestrationStrategyAvailability::Unavailable(
            OrchestrationStrategyUnavailableReason::AdaptiveUsageAuthorityUnavailable,
        )
    );
}

#[test]
fn strategy_and_preset_updates_share_one_canonical_state() {
    let state = SessionExecutionPolicyState::new().expect("default policy");
    state
        .select_strategy(OrchestrationMode::Manual)
        .expect("manual strategy is available");
    state
        .select_mode(
            ExecutionModeSelection::Fast,
            SessionPolicySource::ExplicitUserSelection,
        )
        .expect("fast preset is available");

    let resolved = state
        .resolved_orchestration_policy()
        .expect("resolved policy");
    assert_eq!(resolved.strategy(), OrchestrationMode::Manual);
    assert_eq!(
        resolved.execution().selected_mode(),
        &ExecutionModeSelection::Fast
    );
}

#[test]
fn setup_rows_expose_exact_selectable_identity_without_publishing_it() {
    let account = ready_item("Account A", "account-a", "codex");
    let invalid_account = ProviderSetupItem {
        readiness: SetupReadinessState::Invalid("authentication is not usable".to_string()),
        ..account.clone()
    };
    let account_rows = selectable_provider_setup_items(
        &[account.clone(), invalid_account],
        RoutingRole::Executor,
        "codex",
    );
    assert!(!account_rows[0].is_disabled);
    assert_eq!(account_rows[0].actions.len(), 1);
    assert!(account_rows[1].is_disabled);
    assert_eq!(account.id.as_deref(), Some("account-a"));

    let connection = ready_item("Connection C", "connection-c", "omniroute");
    let connection_rows = selectable_provider_setup_items(
        std::slice::from_ref(&connection),
        RoutingRole::Planner,
        "connection",
    );
    assert!(!connection_rows[0].is_disabled);
    assert_eq!(connection_rows[0].actions.len(), 1);
    assert_eq!(connection.id.as_deref(), Some("connection-c"));
}

#[test]
fn setup_role_rows_are_candidate_editors_and_not_runtime_state() {
    let profile = RoutingProfile::new(
        RoutingProfileId::new("candidate").expect("profile id"),
        "candidate",
        1,
    )
    .expect("profile");
    let rows = routing_role_items(Some(&profile), RoutingRole::Planner);
    assert_eq!(rows.len(), 5);
    assert!(rows[1].is_current);
    assert_eq!(rows[1].actions.len(), 1);
    assert!(rows.iter().all(|row| !row.is_disabled));

    let provider_setup = ProviderSetupSnapshot {
        connections: vec![ready_item("Connection C", "connection-c", "omniroute")],
        ..ProviderSetupSnapshot::default()
    };
    assert_eq!(provider_setup.connections[0].models, ["model-1"]);
}

fn role_profile(pool_id: Option<&str>) -> RoutingProfile {
    let mut profile = RoutingProfile::new(
        RoutingProfileId::new("candidate").expect("profile id"),
        "candidate",
        1,
    )
    .expect("profile");
    profile
        .assign(
            RoutingRole::Planner,
            RoutingAssignment {
                connection_id: pool_id
                    .is_none()
                    .then(|| "account-a".to_string())
                    .unwrap_or_default(),
                provider_id: "codex".to_string(),
                model_id: "planner-model".to_string(),
                enabled: true,
                label: Some("planner".to_string()),
                pool_id: pool_id.map(|id| PoolId::new(id).expect("pool id")),
            },
        )
        .expect("assignment");
    profile
}

fn pool_snapshot() -> PoolSetupSnapshot {
    let pool_id = PoolId::new("codex-primary").expect("pool id");
    let member_id = crate::legacy_core::PoolMemberId::new("member-main").expect("member id");
    PoolSetupSnapshot {
        summaries: vec![
            PoolSummary {
                id: pool_id.clone(),
                display_name: "Codex primary".to_string(),
                provider: AccountPoolProviderFamily::NativeCodex,
                member_count: 2,
                selected: member_id.to_string(),
                readiness: PoolReadiness::Ready,
            },
            PoolSummary {
                id: PoolId::new("omni-primary").expect("pool id"),
                display_name: "Omni primary".to_string(),
                provider: AccountPoolProviderFamily::OmniRoute,
                member_count: 1,
                selected: "connection-main".to_string(),
                readiness: PoolReadiness::Ready,
            },
        ],
        member_labels: [((pool_id, member_id), "personal-main".to_string())]
            .into_iter()
            .collect(),
        ..Default::default()
    }
}

#[test]
fn identity_source_selector_has_only_direct_and_named_pool() {
    let rows = identity_source_items(Some(&role_profile(None)), RoutingRole::Planner, None);
    assert_eq!(
        rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
        ["Direct", "Named pool"]
    );
    assert!(rows[0].is_current);
    assert!(!rows[1].is_current);

    let rows = identity_source_items(
        Some(&role_profile(None)),
        RoutingRole::Planner,
        Some(IdentitySourceChoice::NamedPool),
    );
    assert!(!rows[0].is_current);
    assert!(rows[1].is_current);
}

#[test]
fn identity_source_action_emits_the_exact_role_event() {
    let rows = identity_source_items(Some(&role_profile(None)), RoutingRole::Planner, None);
    let (sender, mut receiver) = mpsc::unbounded_channel();
    (rows[1].actions[0])(&AppEventSender::new(sender));
    assert!(matches!(
        receiver.try_recv().expect("identity source event"),
        AppEvent::UpdateOrchestrationSetupIdentitySource {
            role: RoutingRole::Planner,
            source: IdentitySourceChoice::NamedPool,
        }
    ));
}

#[test]
fn pool_picker_filters_provider_and_preserves_exact_pool_id() {
    let rows = pool_selection_items(
        Some(&role_profile(None)),
        RoutingRole::Planner,
        &pool_snapshot(),
    );
    assert_eq!(rows.len(), 1);
    assert!(rows[0].name.contains("codex-primary"));
    assert!(!rows[0].is_disabled);
    assert!(
        rows[0]
            .description
            .as_deref()
            .unwrap()
            .contains("personal-main")
    );

    let mut omni_profile = role_profile(None);
    let omni_assignment = omni_profile
        .assignments
        .get_mut(&RoutingRole::Planner)
        .expect("planner assignment");
    omni_assignment.provider_id = "omniroute".to_string();
    omni_assignment.connection_id = "connection-main".to_string();
    let rows = pool_selection_items(Some(&omni_profile), RoutingRole::Planner, &pool_snapshot());
    assert_eq!(rows.len(), 1);
    assert!(rows[0].name.starts_with("omni-primary"));
}

#[test]
fn pool_picker_action_emits_the_exact_pool_id_event() {
    let rows = pool_selection_items(
        Some(&role_profile(None)),
        RoutingRole::Planner,
        &pool_snapshot(),
    );
    let (sender, mut receiver) = mpsc::unbounded_channel();
    (rows[0].actions[0])(&AppEventSender::new(sender));
    assert!(matches!(
        receiver.try_recv().expect("pool selection event"),
        AppEvent::UpdateOrchestrationSetupPool { role: RoutingRole::Planner, pool_id }
            if pool_id.as_str() == "codex-primary"
    ));
}

#[test]
fn pool_picker_rows_are_bounded_and_redacted() {
    let rows = pool_selection_items(
        Some(&role_profile(None)),
        RoutingRole::Planner,
        &pool_snapshot(),
    );
    let rendered = rows
        .iter()
        .map(|row| {
            format!(
                "{} | {}",
                row.name,
                row.description.as_deref().unwrap_or("no description")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(rendered, @r###"codex-primary · Ready | Codex primary · 2 members · explicit member member-main (personal-main) · Ready"###);
}

#[test]
fn degraded_pool_remains_selectable_without_fallback() {
    let mut snapshot = pool_snapshot();
    snapshot.member_statuses.insert(
        (
            PoolId::new("codex-primary").expect("pool id"),
            crate::legacy_core::PoolMemberId::new("member-stale").expect("member id"),
        ),
        PoolMemberReadiness::MissingAccountReference,
    );
    let rows = pool_selection_items(Some(&role_profile(None)), RoutingRole::Planner, &snapshot);
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].is_disabled);
    assert!(
        rows[0]
            .description
            .as_deref()
            .unwrap()
            .contains("degraded: 1")
    );
}

#[test]
fn identity_source_transitions_clear_incompatible_candidate_fields() {
    let mut profile = role_profile(Some("codex-primary"));
    set_identity_source(
        &mut profile,
        RoutingRole::Planner,
        IdentitySourceChoice::Direct,
    )
    .expect("direct transition");
    let assignment = profile.assignments.get(&RoutingRole::Planner).unwrap();
    assert!(assignment.connection_id.is_empty());
    assert!(assignment.pool_id.is_none());

    set_pool_selection(
        &mut profile,
        RoutingRole::Planner,
        PoolId::new("codex-primary").expect("pool id"),
    )
    .expect("pool selection");
    let assignment = profile.assignments.get(&RoutingRole::Planner).unwrap();
    assert!(assignment.connection_id.is_empty());
    assert_eq!(
        assignment.pool_id.as_ref().unwrap().as_str(),
        "codex-primary"
    );
    assert_eq!(assignment.model_id, "planner-model");

    set_identity_source(
        &mut profile,
        RoutingRole::Planner,
        IdentitySourceChoice::NamedPool,
    )
    .expect("named-pool transition");
    let assignment = profile.assignments.get(&RoutingRole::Planner).unwrap();
    assert!(assignment.connection_id.is_empty());
    assert!(assignment.pool_id.is_none());
}

#[test]
fn missing_pool_remains_visible_and_unselectable() {
    let rows = pool_selection_items(
        Some(&role_profile(Some("missing-pool"))),
        RoutingRole::Planner,
        &PoolSetupSnapshot::default(),
    );
    assert_eq!(rows.len(), 1);
    assert!(rows[0].name.contains("missing-pool"));
    assert!(rows[0].is_disabled);
    assert!(rows[0].is_current);
}

#[test]
fn incompatible_existing_pool_remains_visible_and_unselectable() {
    let rows = pool_selection_items(
        Some(&role_profile(Some("omni-primary"))),
        RoutingRole::Planner,
        &pool_snapshot(),
    );
    assert_eq!(rows.len(), 2);
    let incompatible = rows
        .iter()
        .find(|row| row.name.starts_with("omni-primary"))
        .expect("incompatible pool row");
    assert!(incompatible.is_current);
    assert!(incompatible.is_disabled);
    assert!(incompatible.actions.is_empty());
}

#[test]
fn unavailable_selected_pool_is_not_selectable() {
    let mut snapshot = pool_snapshot();
    snapshot.summaries.push(PoolSummary {
        id: PoolId::new("codex-stale").expect("pool id"),
        display_name: "Codex stale".to_string(),
        provider: AccountPoolProviderFamily::NativeCodex,
        member_count: 1,
        selected: "missing-member".to_string(),
        readiness: PoolReadiness::MissingAccountReference,
    });
    let rows = pool_selection_items(Some(&role_profile(None)), RoutingRole::Planner, &snapshot);
    assert_eq!(rows.len(), 2);
    let stale = rows
        .iter()
        .find(|row| row.name.starts_with("codex-stale"))
        .expect("stale pool row");
    assert!(stale.is_disabled);
    assert!(stale.actions.is_empty());
}
