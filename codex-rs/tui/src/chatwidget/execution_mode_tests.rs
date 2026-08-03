use super::mode_entries;
use super::mode_label;
use super::parse_mode_argument;
use super::routing_role_items;
use super::selectable_provider_setup_items;
use super::strategy_entries;
use super::unavailable_reason;
use crate::legacy_core::ExecutionModeSelection;
use crate::legacy_core::OrchestrationMode;
use crate::legacy_core::OrchestrationStrategyAvailability;
use crate::legacy_core::OrchestrationStrategyUnavailableReason;
use crate::legacy_core::ResolvedOrchestrationPolicy;
use crate::legacy_core::RoutingProfile;
use crate::legacy_core::RoutingProfileId;
use crate::legacy_core::RoutingRole;
use crate::legacy_core::SessionExecutionPolicyState;
use crate::legacy_core::SessionPolicySource;
use crate::orchestration_setup::SetupReadinessState;
use crate::provider_setup::ProviderSetupItem;
use crate::provider_setup::ProviderSetupSnapshot;

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
