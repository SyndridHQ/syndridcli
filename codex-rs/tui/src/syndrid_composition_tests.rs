use super::*;
use codex_app_server_client::TrustedCompositionSnapshotRequest;
use codex_app_server_client::legacy_core::SessionExecutionPolicyState;
use pretty_assertions::assert_eq;
use std::path::PathBuf;
use std::sync::Arc;
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
        TrustedCompositionSnapshotError::ToolAuthorityUnavailable
    );
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
