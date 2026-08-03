use super::OrchestrationSetupReadiness;
use super::SetupReadinessState;
use super::should_show_first_run_invitation;
use crate::legacy_core::ExecutionModeSelection;
use crate::legacy_core::OrchestrationMode;
use crate::orchestration_profile::OrchestrationProfileSelection;

fn selection(
    strategy: OrchestrationMode,
    preset: ExecutionModeSelection,
) -> OrchestrationProfileSelection {
    OrchestrationProfileSelection { strategy, preset }
}

#[test]
fn single_builtin_presets_are_ready_without_orchestration_runtime() {
    for preset in [
        ExecutionModeSelection::Fast,
        ExecutionModeSelection::Balanced,
        ExecutionModeSelection::UsageSaver,
        ExecutionModeSelection::Deep,
    ] {
        let readiness = OrchestrationSetupReadiness::for_selection(
            &selection(OrchestrationMode::Single, preset),
            false,
        );
        assert!(readiness.can_apply());
        assert_eq!(readiness.routing, SetupReadinessState::NotRequired);
        assert_eq!(readiness.runtime_assembly, SetupReadinessState::NotRequired);
    }
}

#[test]
fn manual_readiness_requires_trusted_runtime_authority() {
    let selection = selection(OrchestrationMode::Manual, ExecutionModeSelection::Fast);
    let blocked = OrchestrationSetupReadiness::for_selection(&selection, false);
    assert!(!blocked.can_apply());
    assert!(matches!(
        blocked.required_roles,
        SetupReadinessState::MissingAuthority(_)
    ));

    let ready = OrchestrationSetupReadiness::for_selection(&selection, true);
    assert!(ready.can_apply());
    assert_eq!(ready.runtime_assembly, SetupReadinessState::Ready);
}

#[test]
fn unfinished_strategies_are_unavailable_without_aliasing() {
    for strategy in [
        OrchestrationMode::Recommended,
        OrchestrationMode::Automatic,
        OrchestrationMode::Adaptive,
    ] {
        let readiness = OrchestrationSetupReadiness::for_selection(
            &selection(strategy, ExecutionModeSelection::Balanced),
            true,
        );
        assert!(!readiness.can_apply());
        assert!(matches!(
            readiness.strategy,
            SetupReadinessState::Unavailable(_)
        ));
    }
}

#[test]
fn first_run_invitation_is_local_once_and_repair_warnings_do_not_duplicate_it() {
    assert!(should_show_first_run_invitation(false, false, true));
    assert!(!should_show_first_run_invitation(true, false, true));
    assert!(!should_show_first_run_invitation(false, true, true));
    assert!(!should_show_first_run_invitation(false, false, false));
}

#[test]
fn candidate_navigation_does_not_mutate_active_policy() {
    let state = crate::legacy_core::SessionExecutionPolicyState::new().expect("default policy");
    let active = selection(
        state.strategy().expect("strategy"),
        state.selected_mode().expect("preset"),
    );
    let candidate = selection(OrchestrationMode::Single, ExecutionModeSelection::Deep);
    assert_ne!(candidate, active);
    assert_eq!(
        state
            .selected_mode()
            .expect("active preset remains unchanged"),
        ExecutionModeSelection::Balanced
    );
    assert_eq!(
        state.strategy().expect("active strategy remains unchanged"),
        OrchestrationMode::Single
    );
}

#[test]
fn setup_readiness_summary_snapshot_is_bounded_and_explainable() {
    let readiness = OrchestrationSetupReadiness::for_selection(
        &selection(OrchestrationMode::Single, ExecutionModeSelection::Balanced),
        false,
    );
    insta::assert_snapshot!(format!(
        "Syndrid Setup\nStrategy             {}\nPreset               {}\nRouting              {}\nRequired roles       {}\nRuntime assembly     {}",
        readiness.strategy.label(),
        readiness.preset.label(),
        readiness.routing.label(),
        readiness.required_roles.label(),
        readiness.runtime_assembly.label(),
    ));
}
