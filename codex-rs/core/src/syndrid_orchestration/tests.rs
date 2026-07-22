use super::error::adapter_error;
use super::error::map_native_error;
use super::spawn::apply_permissions;
use super::spawn::apply_routes;
use super::spawn::handoff_message;
use codex_orchestration::AgentRole;
use codex_orchestration::BoundedText;
use codex_orchestration::DataQuality;
use codex_orchestration::EffortRoute;
use codex_orchestration::ForecastConfidence;
use codex_orchestration::ModelRoute;
use codex_orchestration::PermissionEnvelope;
use codex_orchestration::RouteSource;
use codex_orchestration::RouteStatus;
use codex_orchestration::StructuredHandoff;
use codex_orchestration::TaskId;
use codex_orchestration::WorkAccess;
use codex_orchestration::WorkflowId;
use codex_orchestration_adapter::AdapterErrorKind;
use codex_orchestration_adapter::Retryability;
use codex_orchestration_adapter::RuntimeAgentId;
use codex_orchestration_adapter::SpawnChildRequest;
use codex_protocol::error::CodexErr;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;

fn workflow_id() -> WorkflowId {
    WorkflowId::new("workflow-1").expect("workflow id")
}

fn task_id() -> TaskId {
    TaskId::new("task-1").expect("task id")
}

fn agent_id(value: &str) -> codex_orchestration::AgentId {
    codex_orchestration::AgentId::new(value).expect("agent id")
}

fn bounded(value: &str) -> BoundedText {
    BoundedText::new(value).expect("bounded text")
}

fn handoff() -> StructuredHandoff {
    StructuredHandoff::new(
        workflow_id(),
        task_id(),
        agent_id("parent-agent"),
        AgentRole::Executor,
        bounded("bounded child summary"),
        bounded("objective"),
        bounded("scope"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ForecastConfidence::High,
        Vec::new(),
        bounded("continue"),
        Vec::new(),
        DataQuality::Exact,
    )
}

fn request_with_routes(
    access: WorkAccess,
    model_route: ModelRoute,
    effort_route: EffortRoute,
) -> SpawnChildRequest {
    SpawnChildRequest::new(
        workflow_id(),
        task_id(),
        agent_id("child-agent"),
        Some(agent_id("parent-agent")),
        AgentRole::Executor,
        access,
        model_route,
        effort_route,
        PermissionEnvelope::new(WorkAccess::Writer, WorkAccess::Writer, access)
            .expect("permission envelope"),
        handoff(),
    )
    .expect("spawn request")
}

fn request(access: WorkAccess) -> SpawnChildRequest {
    request_with_routes(
        access,
        ModelRoute {
            requested: Some("requested-model".to_string()),
            resolved: Some("resolved-model".to_string()),
            source: RouteSource::Policy,
            status: RouteStatus::Resolved,
            data_quality: DataQuality::Derived,
        },
        EffortRoute {
            requested: Some(ReasoningEffort::Low),
            resolved: Some(ReasoningEffort::High),
            source: RouteSource::Policy,
            status: RouteStatus::Resolved,
            data_quality: DataQuality::Derived,
        },
    )
}

#[tokio::test]
async fn resolved_routes_map_without_fallback_or_lookup() {
    let mut config = crate::config::test_config().await;
    let value = request(WorkAccess::ReadOnly);
    apply_routes(
        &mut config,
        &value,
        (value.workflow_id(), value.task_id(), value.child_agent_id()),
    )
    .expect("resolved routes should map");
    assert_eq!(config.model.as_deref(), Some("resolved-model"));
    assert_eq!(config.model_reasoning_effort, Some(ReasoningEffort::High));
}

#[tokio::test]
async fn unresolved_routes_are_rejected() {
    let mut config = crate::config::test_config().await;
    let value = request_with_routes(
        WorkAccess::ReadOnly,
        ModelRoute {
            requested: Some("requested-model".to_string()),
            resolved: None,
            source: RouteSource::Policy,
            status: RouteStatus::Requested,
            data_quality: DataQuality::Derived,
        },
        EffortRoute {
            requested: Some(ReasoningEffort::Low),
            resolved: Some(ReasoningEffort::High),
            source: RouteSource::Policy,
            status: RouteStatus::Resolved,
            data_quality: DataQuality::Derived,
        },
    );
    let error = apply_routes(
        &mut config,
        &value,
        (value.workflow_id(), value.task_id(), value.child_agent_id()),
    )
    .expect_err("unresolved route must not fall back");
    assert_eq!(error.kind(), AdapterErrorKind::InvalidRequest);
}

#[tokio::test]
async fn readonly_access_narrows_native_permissions() {
    let mut config = crate::config::test_config().await;
    let value = request(WorkAccess::ReadOnly);
    apply_permissions(
        &mut config,
        &value,
        (value.workflow_id(), value.task_id(), value.child_agent_id()),
    )
    .expect("read-only permission should be supported");
    assert_eq!(
        config.permissions.effective_permission_profile(),
        codex_protocol::models::PermissionProfile::read_only()
    );
}

#[test]
fn handoff_translation_is_bounded_and_excludes_transcript_material() {
    let value = handoff();
    assert_eq!(handoff_message(&value), "bounded child summary");
}

#[test]
fn native_identity_is_recorded_without_generation() {
    let native_id = codex_protocol::ThreadId::new();
    let runtime_id = RuntimeAgentId::new(native_id.to_string()).expect("runtime identity");
    assert_eq!(runtime_id.as_str(), native_id.to_string());
}

#[test]
fn native_errors_map_to_bounded_typed_adapter_errors() {
    let workflow_id = workflow_id();
    let task_id = task_id();
    let agent_id = agent_id("child-agent");
    let attribution = (&workflow_id, &task_id, &agent_id);
    let error = map_native_error(CodexErr::AgentLimitReached { max_threads: 2 }, attribution);
    assert_eq!(error.kind(), AdapterErrorKind::CapacityUnavailable);
    assert_eq!(error.retryability(), Retryability::NotRetryable);
    let error = adapter_error(
        AdapterErrorKind::InternalAdapterFailure,
        "x".repeat(codex_orchestration::MAX_HANDOFF_TEXT_BYTES + 100),
        Retryability::NotRetryable,
        attribution,
    );
    assert!(error.message().as_str().len() <= codex_orchestration::MAX_HANDOFF_TEXT_BYTES);
}
