use super::ExecutionModeSelection;
use super::RoutingProfileId;
use super::SessionExecutionPolicyState;
use super::SessionExecutionStateError;
use super::SessionExecutionStatus;
use super::SessionPolicySource;
use pretty_assertions::assert_eq;

#[test]
fn default_session_policy_is_balanced_and_idle() {
    let state = SessionExecutionPolicyState::new().expect("default policy");
    assert_eq!(
        state.selected_mode().expect("mode"),
        ExecutionModeSelection::Balanced
    );
    assert_eq!(
        state.policy_source().expect("source"),
        SessionPolicySource::Default
    );
    assert_eq!(
        state.status().expect("status"),
        SessionExecutionStatus::Idle
    );
    assert_eq!(state.routing_profile_id().expect("route"), None);
}

#[test]
fn mode_selection_is_rejected_after_run_starts() {
    let state = SessionExecutionPolicyState::new().expect("default policy");
    state
        .transition(SessionExecutionStatus::Preparing)
        .expect("preparing");
    assert_eq!(
        state.select_mode(
            ExecutionModeSelection::Fast,
            SessionPolicySource::ExplicitUserSelection,
        ),
        Err(SessionExecutionStateError::PolicyMutationWhileActive)
    );
}

#[test]
fn explicit_builtin_modes_replace_the_resolved_policy() {
    for mode in [
        ExecutionModeSelection::Fast,
        ExecutionModeSelection::Balanced,
        ExecutionModeSelection::UsageSaver,
        ExecutionModeSelection::Deep,
    ] {
        let state = SessionExecutionPolicyState::new().expect("default policy");
        state
            .select_mode(mode.clone(), SessionPolicySource::ExplicitUserSelection)
            .expect("select mode");
        assert_eq!(state.selected_mode().expect("mode"), mode);
        assert_eq!(
            state.policy_source().expect("source"),
            SessionPolicySource::ExplicitUserSelection
        );
        assert_eq!(
            state.status().expect("status"),
            SessionExecutionStatus::Idle
        );
    }
}

#[test]
fn routing_selection_is_rejected_while_active() {
    let state = SessionExecutionPolicyState::new().expect("default policy");
    state
        .transition(SessionExecutionStatus::Preparing)
        .expect("preparing");
    assert_eq!(
        state.select_routing_profile(RoutingProfileId::new("profile").expect("profile")),
        Err(SessionExecutionStateError::RoutingMutationWhileActive)
    );
}

#[test]
fn terminal_reset_is_allowed_for_each_terminal_status() {
    for terminal in [
        SessionExecutionStatus::Completed,
        SessionExecutionStatus::Failed,
        SessionExecutionStatus::Cancelled,
        SessionExecutionStatus::TimedOut,
    ] {
        let state = SessionExecutionPolicyState::new().expect("default policy");
        state
            .transition(SessionExecutionStatus::Preparing)
            .expect("preparing");
        state
            .transition(SessionExecutionStatus::Validating)
            .expect("validating");
        if terminal == SessionExecutionStatus::Cancelled {
            state
                .transition(SessionExecutionStatus::Running)
                .expect("running");
            state
                .transition(SessionExecutionStatus::Cancelling)
                .expect("cancelling");
            state
                .transition(SessionExecutionStatus::Cancelled)
                .expect("cancelled");
        } else if terminal == SessionExecutionStatus::TimedOut {
            state
                .transition(SessionExecutionStatus::Running)
                .expect("running");
            state
                .transition(SessionExecutionStatus::TimedOut)
                .expect("timed out");
        } else {
            state
                .transition(SessionExecutionStatus::Failed)
                .expect("failed");
            if terminal == SessionExecutionStatus::Completed {
                let state = SessionExecutionPolicyState::new().expect("default policy");
                state
                    .transition(SessionExecutionStatus::Preparing)
                    .expect("preparing");
                state
                    .transition(SessionExecutionStatus::Validating)
                    .expect("validating");
                state
                    .transition(SessionExecutionStatus::Running)
                    .expect("running");
                state
                    .transition(SessionExecutionStatus::Completed)
                    .expect("completed");
                assert_eq!(state.reset_to_idle(), Ok(()));
                continue;
            }
        }
        assert_eq!(state.status().expect("terminal status"), terminal);
        assert_eq!(state.reset_to_idle(), Ok(()));
        assert_eq!(
            state.status().expect("idle status"),
            SessionExecutionStatus::Idle
        );
    }
}

#[test]
fn lifecycle_accepts_only_declared_transitions() {
    let state = SessionExecutionPolicyState::new().expect("default policy");
    assert_eq!(
        state.transition(SessionExecutionStatus::Running),
        Err(SessionExecutionStateError::InvalidTransition {
            from: SessionExecutionStatus::Idle,
            to: SessionExecutionStatus::Running,
        })
    );
    for status in [
        SessionExecutionStatus::Preparing,
        SessionExecutionStatus::Validating,
        SessionExecutionStatus::Running,
        SessionExecutionStatus::Cancelling,
        SessionExecutionStatus::Cancelled,
    ] {
        state.transition(status).expect("valid transition");
    }
}

#[test]
fn terminal_reset_returns_to_idle_and_active_reset_is_rejected() {
    let state = SessionExecutionPolicyState::new().expect("default policy");
    assert_eq!(state.reset_to_idle(), Ok(()));
    state
        .transition(SessionExecutionStatus::Preparing)
        .expect("preparing");
    assert_eq!(
        state.reset_to_idle(),
        Err(SessionExecutionStateError::ResetWhileActive)
    );
}

#[test]
fn reset_is_rejected_during_cancellation_cleanup_then_succeeds() {
    let state = SessionExecutionPolicyState::new().expect("default policy");
    state
        .transition(SessionExecutionStatus::Preparing)
        .expect("preparing");
    state
        .transition(SessionExecutionStatus::Validating)
        .expect("validating");
    state
        .transition(SessionExecutionStatus::Running)
        .expect("running");
    state
        .transition(SessionExecutionStatus::Cancelling)
        .expect("cancelling");
    assert_eq!(
        state.reset_to_idle(),
        Err(SessionExecutionStateError::ResetWhileActive)
    );
    state
        .transition(SessionExecutionStatus::Cancelled)
        .expect("cancelled after cleanup");
    assert_eq!(state.reset_to_idle(), Ok(()));
    assert_eq!(state.status().expect("idle"), SessionExecutionStatus::Idle);
}
