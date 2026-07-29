use super::*;
use codex_core::LiveOrchestrationTerminal;
use codex_core::OrchestrationEvidence;
use codex_core::OrchestrationOperationalMetadata;
use codex_core::OrchestrationTurnResult;
use codex_core::UserFacingResponse;

fn context() -> OrchestrationTranscriptContext {
    OrchestrationTranscriptContext {
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        assistant_item_id: "item-1".to_string(),
        completed_at_ms: 42,
    }
}

fn metadata() -> OrchestrationOperationalMetadata {
    OrchestrationOperationalMetadata {
        run_id: "run-1".to_string(),
        provider_invocations: 1,
        tool_calls: 0,
        peak_concurrency: 1,
        synthesis_permitted: true,
        cleanup_complete: true,
    }
}

fn evidence() -> OrchestrationEvidence {
    OrchestrationEvidence {
        terminal: LiveOrchestrationTerminal::Completed,
        failure: None,
        role_count: 0,
    }
}

fn completed() -> OrchestrationTurnResult {
    OrchestrationTurnResult::Completed {
        response: UserFacingResponse::new("final answer").unwrap(),
        metadata: metadata(),
        evidence: evidence(),
    }
}

#[test]
fn completed_result_uses_existing_assistant_and_turn_notifications() {
    let notifications = translate_orchestration_result(&completed(), &context());
    assert!(matches!(
        &notifications[0],
        ServerNotification::AgentMessageDelta(AgentMessageDeltaNotification { delta, .. })
            if delta == "final answer"
    ));
    assert!(matches!(
        &notifications[1],
        ServerNotification::ItemCompleted(ItemCompletedNotification {
            item: ThreadItem::AgentMessage { text, .. }, ..
        }) if text == "final answer"
    ));
    assert!(matches!(
        &notifications[2],
        ServerNotification::TurnCompleted(TurnCompletedNotification {
            turn: Turn {
                status: TurnStatus::Completed,
                ..
            },
            ..
        })
    ));
}

#[test]
fn partial_result_is_labeled_without_forwarding_internal_evidence() {
    let result = OrchestrationTurnResult::Partial {
        response: UserFacingResponse::new("useful partial work").unwrap(),
        cause: codex_core::OrchestrationPartialCause::ResponseUnavailable,
        metadata: metadata(),
        evidence: evidence(),
    };
    let notifications = translate_orchestration_result(&result, &context());
    assert!(matches!(
        &notifications[0],
        ServerNotification::AgentMessageDelta(AgentMessageDeltaNotification { delta, .. })
            if delta.starts_with("[Partial orchestration result]")
                && delta.contains("useful partial work")
                && !delta.contains("role_count")
    ));
}

#[test]
fn partial_result_translation_remains_bounded() {
    let result = OrchestrationTurnResult::Partial {
        response: UserFacingResponse::new("é".repeat(16 * 1024)).unwrap(),
        cause: codex_core::OrchestrationPartialCause::ResponseUnavailable,
        metadata: metadata(),
        evidence: evidence(),
    };
    let notifications = translate_orchestration_result(&result, &context());
    let ServerNotification::AgentMessageDelta(notification) = &notifications[0] else {
        panic!("expected assistant delta");
    };
    assert!(notification.delta.len() <= codex_core::MAX_USER_FACING_RESPONSE_BYTES);
    assert!(
        notification
            .delta
            .is_char_boundary(notification.delta.len())
    );
}

#[test]
fn failures_use_error_and_failed_turn_notifications() {
    let result = OrchestrationTurnResult::TimedOut {
        user_message: UserFacingResponse::new("The orchestration timed out.").unwrap(),
        metadata: metadata(),
        evidence: evidence(),
    };
    let notifications = translate_orchestration_result(&result, &context());
    assert!(matches!(
        &notifications[0],
        ServerNotification::Error(ErrorNotification { error, .. })
            if error.message == "The orchestration timed out."
    ));
    assert!(matches!(
        &notifications[1],
        ServerNotification::TurnCompleted(TurnCompletedNotification {
            turn: Turn {
                status: TurnStatus::Failed,
                error: Some(_),
                ..
            },
            ..
        })
    ));
}

#[test]
fn cancellation_uses_existing_interrupted_turn_behavior() {
    let result = OrchestrationTurnResult::Cancelled {
        user_message: UserFacingResponse::new("The orchestration was cancelled.").unwrap(),
        metadata: metadata(),
        evidence: evidence(),
    };
    let notifications = translate_orchestration_result(&result, &context());
    assert!(matches!(
        &notifications[..],
        [ServerNotification::TurnCompleted(
            TurnCompletedNotification {
                turn: Turn {
                    status: TurnStatus::Interrupted,
                    error: None,
                    ..
                },
                ..
            }
        )]
    ));
}
