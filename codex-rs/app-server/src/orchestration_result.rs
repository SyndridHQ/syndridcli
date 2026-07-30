use codex_app_server_protocol::AgentMessageDeltaNotification;
use codex_app_server_protocol::ErrorNotification;
use codex_app_server_protocol::ItemCompletedNotification;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnError;
use codex_app_server_protocol::TurnItemsView;
use codex_app_server_protocol::TurnStatus;
use codex_core::OrchestrationTurnResult;

/// Identifies the existing app-server transcript items used for one translated turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestrationTranscriptContext {
    pub thread_id: String,
    pub turn_id: String,
    pub assistant_item_id: String,
    pub completed_at_ms: i64,
}

/// Converts a core orchestration result into existing app-server notifications.
///
/// This is deliberately not wired into turn admission yet. The future production runner will
/// own persistence and submit these notifications through the same event path as Codex turns.
pub fn translate_orchestration_result(
    result: &OrchestrationTurnResult,
    context: &OrchestrationTranscriptContext,
) -> Vec<ServerNotification> {
    match result {
        OrchestrationTurnResult::Completed { response, .. } => {
            assistant_notifications(response.as_str(), context)
        }
        OrchestrationTurnResult::Partial { response, .. } => {
            let response = response
                .with_prefix("[Partial orchestration result]\n\n")
                .unwrap_or_else(|_| {
                    codex_core::UserFacingResponse::new(
                        "[Partial orchestration result]\n\nThe response was too large to display.",
                    )
                    .expect("static partial response must remain bounded")
                });
            assistant_notifications(response.as_str(), context)
        }
        OrchestrationTurnResult::Failed { user_message, .. } => {
            failure_notifications(user_message.as_str(), context)
        }
        OrchestrationTurnResult::Cancelled { .. } => {
            vec![turn_completed_notification(
                context,
                TurnStatus::Interrupted,
                None,
            )]
        }
        OrchestrationTurnResult::TimedOut { user_message, .. } => {
            failure_notifications(user_message.as_str(), context)
        }
        OrchestrationTurnResult::BudgetExhausted { user_message, .. } => {
            failure_notifications(user_message.as_str(), context)
        }
        OrchestrationTurnResult::CleanupIncomplete { user_message, .. } => {
            failure_notifications(user_message.as_str(), context)
        }
    }
}

fn assistant_notifications(
    text: &str,
    context: &OrchestrationTranscriptContext,
) -> Vec<ServerNotification> {
    let item = ThreadItem::AgentMessage {
        id: context.assistant_item_id.clone(),
        text: text.to_string(),
        phase: None,
        memory_citation: None,
    };
    vec![
        ServerNotification::AgentMessageDelta(AgentMessageDeltaNotification {
            thread_id: context.thread_id.clone(),
            turn_id: context.turn_id.clone(),
            item_id: context.assistant_item_id.clone(),
            delta: text.to_string(),
        }),
        ServerNotification::ItemCompleted(ItemCompletedNotification {
            item,
            thread_id: context.thread_id.clone(),
            turn_id: context.turn_id.clone(),
            completed_at_ms: context.completed_at_ms,
        }),
        turn_completed_notification(context, TurnStatus::Completed, None),
    ]
}

fn failure_notifications(
    message: &str,
    context: &OrchestrationTranscriptContext,
) -> Vec<ServerNotification> {
    let error = TurnError {
        message: message.to_string(),
        codex_error_info: None,
        additional_details: None,
    };
    vec![
        ServerNotification::Error(ErrorNotification {
            error: error.clone(),
            will_retry: false,
            thread_id: context.thread_id.clone(),
            turn_id: context.turn_id.clone(),
        }),
        turn_completed_notification(context, TurnStatus::Failed, Some(error)),
    ]
}

fn turn_completed_notification(
    context: &OrchestrationTranscriptContext,
    status: TurnStatus,
    error: Option<TurnError>,
) -> ServerNotification {
    ServerNotification::TurnCompleted(TurnCompletedNotification {
        thread_id: context.thread_id.clone(),
        turn: Turn {
            id: context.turn_id.clone(),
            items: Vec::new(),
            items_view: TurnItemsView::NotLoaded,
            status,
            error,
            started_at: None,
            completed_at: None,
            duration_ms: None,
        },
    })
}

#[cfg(test)]
#[path = "orchestration_result_tests.rs"]
mod tests;
