use codex_orchestration::AgentId;
use codex_orchestration::BoundedText;
use codex_orchestration::DataQuality;
use codex_orchestration::TaskId;
use codex_orchestration::WorkflowId;
use codex_orchestration_adapter::AdapterError;
use codex_orchestration_adapter::AdapterErrorKind;
use codex_orchestration_adapter::Retryability;
use codex_protocol::error::CodexErr;

const MAX_ERROR_BYTES: usize = codex_orchestration::MAX_HANDOFF_TEXT_BYTES;

pub(super) fn adapter_error(
    kind: AdapterErrorKind,
    message: impl AsRef<str>,
    retryability: Retryability,
    attribution: (&WorkflowId, &TaskId, &AgentId),
) -> AdapterError {
    AdapterError::new(
        kind,
        bounded_message(message.as_ref()),
        retryability,
        DataQuality::Exact,
    )
    .with_attribution(
        Some(attribution.0.clone()),
        Some(attribution.1.clone()),
        Some(attribution.2.clone()),
    )
}

pub(super) fn map_native_error(
    error: CodexErr,
    attribution: (&WorkflowId, &TaskId, &AgentId),
) -> AdapterError {
    let kind = match &error {
        CodexErr::InvalidRequest(_) => AdapterErrorKind::InvalidRequest,
        CodexErr::UnsupportedOperation(_) => AdapterErrorKind::Unsupported,
        CodexErr::ThreadNotFound(_) => AdapterErrorKind::ChildNotFound,
        CodexErr::AgentLimitReached { .. } | CodexErr::ServerOverloaded => {
            AdapterErrorKind::CapacityUnavailable
        }
        CodexErr::Sandbox(_) | CodexErr::CyberPolicy { .. } => AdapterErrorKind::PermissionDenied,
        CodexErr::InternalAgentDied => AdapterErrorKind::RuntimeUnavailable,
        _ => AdapterErrorKind::InternalAdapterFailure,
    };
    let retryability = if error.is_retryable() {
        Retryability::Retryable
    } else {
        Retryability::NotRetryable
    };
    adapter_error(kind, error.to_string(), retryability, attribution)
}

fn bounded_message(message: &str) -> BoundedText {
    let end = message
        .char_indices()
        .take_while(|(index, character)| *index + character.len_utf8() <= MAX_ERROR_BYTES)
        .last()
        .map_or(0, |(index, character)| index + character.len_utf8());
    BoundedText::new(&message[..end]).expect("bounded error truncation must be valid")
}
