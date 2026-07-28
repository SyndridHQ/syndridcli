use super::mode_entries;
use super::mode_label;
use super::parse_mode_argument;
use crate::legacy_core::ExecutionModeSelection;
use crate::legacy_core::SessionExecutionPolicyState;
use crate::legacy_core::SessionPolicySource;

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
