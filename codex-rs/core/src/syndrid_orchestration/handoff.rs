use super::CodexOrchestrationAdapter;
use super::error::adapter_error;
use super::error::map_native_error;
use super::spawn::handoff_message;
use crate::agent::AgentStatus;
use crate::agent_communication::AgentCommunicationContext;
use crate::agent_communication::AgentCommunicationKind;
use codex_orchestration_adapter::AdapterError;
use codex_orchestration_adapter::AdapterErrorKind;
use codex_orchestration_adapter::DeliverHandoffRequest;
use codex_orchestration_adapter::DeliverHandoffResult;
use codex_orchestration_adapter::HandoffDeliveryOutcome;
use codex_orchestration_adapter::Retryability;
use codex_protocol::AgentPath;
use codex_protocol::ThreadId;
use codex_protocol::protocol::InterAgentCommunication;

pub(super) async fn deliver_handoff(
    adapter: &CodexOrchestrationAdapter,
    request: DeliverHandoffRequest,
) -> Result<DeliverHandoffResult, AdapterError> {
    let attribution = (&request.workflow_id, &request.task_id, &request.agent_id);
    let runtime_id = ThreadId::try_from(request.runtime_id.as_str()).map_err(|error| {
        adapter_error(
            AdapterErrorKind::InvalidRequest,
            error.to_string(),
            Retryability::NotRetryable,
            attribution,
        )
    })?;
    let metadata = adapter
        .agent_control
        .ensure_agent_known(runtime_id)
        .map_err(|error| map_native_error(error, attribution))?;
    let target_path = metadata.agent_path.ok_or_else(|| {
        adapter_error(
            AdapterErrorKind::InternalAdapterFailure,
            "native child is missing its agent path",
            Retryability::NotRetryable,
            attribution,
        )
    })?;
    if matches!(
        adapter.agent_control.get_status(runtime_id).await,
        AgentStatus::Completed(_) | AgentStatus::Errored(_) | AgentStatus::Shutdown
    ) {
        return Ok(DeliverHandoffResult {
            workflow_id: request.workflow_id,
            task_id: request.task_id,
            agent_id: request.agent_id,
            runtime_id: request.runtime_id,
            outcome: HandoffDeliveryOutcome::TargetCompleted,
        });
    }
    adapter
        .agent_control
        .ensure_v2_agent_loaded(adapter.base_config.clone(), runtime_id)
        .await
        .map_err(|error| map_native_error(error, attribution))?;
    let author_path = adapter
        .parent_session_source
        .get_agent_path()
        .unwrap_or_else(AgentPath::root);
    let communication = InterAgentCommunication::new_encrypted(
        author_path,
        target_path,
        Vec::new(),
        handoff_message(&request.handoff),
        true,
    );
    let context =
        AgentCommunicationContext::new(AgentCommunicationKind::Message, adapter.parent_thread_id);
    adapter
        .agent_control
        .send_inter_agent_communication(runtime_id, communication, context)
        .await
        .map_err(|error| map_native_error(error, attribution))?;
    Ok(DeliverHandoffResult {
        workflow_id: request.workflow_id,
        task_id: request.task_id,
        agent_id: request.agent_id,
        runtime_id: request.runtime_id,
        outcome: HandoffDeliveryOutcome::Accepted,
    })
}
