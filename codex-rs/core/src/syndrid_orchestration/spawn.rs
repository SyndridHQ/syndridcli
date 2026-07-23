use super::CodexOrchestrationAdapter;
use super::error::adapter_error;
use super::error::map_native_error;
use crate::agent::control::SpawnAgentOptions;
use crate::agent::next_thread_spawn_depth;
use crate::agent_communication::AgentCommunicationContext;
use crate::agent_communication::AgentCommunicationKind;
use codex_orchestration::AgentRole;
use codex_orchestration::WorkAccess;
use codex_orchestration_adapter::AdapterError;
use codex_orchestration_adapter::AdapterErrorKind;
use codex_orchestration_adapter::Retryability;
use codex_orchestration_adapter::SpawnChildRequest;
use codex_orchestration_adapter::SpawnChildResult;
use codex_protocol::AgentPath;
use codex_protocol::protocol::InterAgentCommunication;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;

pub(super) async fn spawn_child(
    adapter: &CodexOrchestrationAdapter,
    request: SpawnChildRequest,
) -> Result<SpawnChildResult, AdapterError> {
    let attribution = (
        request.workflow_id(),
        request.task_id(),
        request.child_agent_id(),
    );
    let mut config = adapter.base_config.clone();
    apply_routes(&mut config, &request, attribution)?;
    apply_permissions(&mut config, &request, attribution)?;

    let parent_path = adapter
        .parent_session_source
        .get_agent_path()
        .unwrap_or_else(AgentPath::root);
    let child_path = parent_path
        .join(request.task_id().as_str())
        .map_err(|message| {
            adapter_error(
                AdapterErrorKind::InvalidRequest,
                message,
                Retryability::NotRetryable,
                attribution,
            )
        })?;
    let session_source = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id: adapter.parent_thread_id,
        depth: next_thread_spawn_depth(&adapter.parent_session_source),
        agent_path: Some(child_path.clone()),
        agent_nickname: None,
        agent_role: Some(role_name(request.role()).to_string()),
    });
    let communication = InterAgentCommunication::new_encrypted(
        parent_path,
        child_path,
        Vec::new(),
        handoff_message(request.handoff()),
        true,
    );
    let context =
        AgentCommunicationContext::new(AgentCommunicationKind::Spawn, adapter.parent_thread_id);
    let spawned = adapter
        .agent_control
        .spawn_agent_with_communication(
            config,
            communication,
            context,
            Some(session_source),
            SpawnAgentOptions {
                parent_thread_id: Some(adapter.parent_thread_id),
                ..SpawnAgentOptions::default()
            },
        )
        .await
        .map_err(|error| map_native_error(error, attribution))?;
    let runtime_id = codex_orchestration_adapter::RuntimeAgentId::new(
        spawned.thread_id.to_string(),
    )
    .map_err(|error| {
        adapter_error(
            AdapterErrorKind::InternalAdapterFailure,
            error.to_string(),
            Retryability::NotRetryable,
            attribution,
        )
    })?;
    Ok(SpawnChildResult {
        workflow_id: request.workflow_id().clone(),
        task_id: request.task_id().clone(),
        agent_id: request.child_agent_id().clone(),
        runtime_id,
    })
}

pub(super) fn apply_routes(
    config: &mut crate::config::Config,
    request: &SpawnChildRequest,
    attribution: (
        &codex_orchestration::WorkflowId,
        &codex_orchestration::TaskId,
        &codex_orchestration::AgentId,
    ),
) -> Result<(), AdapterError> {
    let model = request.model_route().resolved.as_deref().ok_or_else(|| {
        adapter_error(
            AdapterErrorKind::InvalidRequest,
            "spawn model route has no resolved model",
            Retryability::NotRetryable,
            attribution,
        )
    })?;
    if model.trim().is_empty() {
        return Err(adapter_error(
            AdapterErrorKind::InvalidRequest,
            "spawn model route resolved model is empty",
            Retryability::NotRetryable,
            attribution,
        ));
    }
    let effort = request.effort_route().resolved.clone().ok_or_else(|| {
        adapter_error(
            AdapterErrorKind::InvalidRequest,
            "spawn effort route has no resolved effort",
            Retryability::NotRetryable,
            attribution,
        )
    })?;
    config.model = Some(model.to_string());
    config.model_reasoning_effort = Some(effort);
    Ok(())
}

pub(super) fn apply_permissions(
    config: &mut crate::config::Config,
    request: &SpawnChildRequest,
    attribution: (
        &codex_orchestration::WorkflowId,
        &codex_orchestration::TaskId,
        &codex_orchestration::AgentId,
    ),
) -> Result<(), AdapterError> {
    let read_only = matches!(request.access(), WorkAccess::ReadOnly)
        || matches!(
            request.permissions().assignment_access(),
            WorkAccess::ReadOnly
        );
    if read_only {
        config
            .permissions
            .set_permission_profile(codex_protocol::models::PermissionProfile::read_only())
            .map_err(|error| {
                adapter_error(
                    AdapterErrorKind::PermissionDenied,
                    error.to_string(),
                    Retryability::NotRetryable,
                    attribution,
                )
            })?;
        return Ok(());
    }
    config
        .permissions
        .can_set_permission_profile(&codex_protocol::models::PermissionProfile::workspace_write())
        .map_err(|error| {
            adapter_error(
                AdapterErrorKind::PermissionDenied,
                error.to_string(),
                Retryability::NotRetryable,
                attribution,
            )
        })?;
    Ok(())
}

fn role_name(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "planner",
        AgentRole::Explorer => "explorer",
        AgentRole::Executor => "executor",
        AgentRole::Verifier => "verifier",
    }
}

pub(super) fn handoff_message(handoff: &codex_orchestration::StructuredHandoff) -> String {
    handoff.task_summary().as_str().to_string()
}
