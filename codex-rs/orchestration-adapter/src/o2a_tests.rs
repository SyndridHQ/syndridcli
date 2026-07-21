use codex_orchestration::AgentRole;
use codex_orchestration::BoundedText;
use codex_orchestration::DataQuality;
use codex_orchestration::EffortRoute;
use codex_orchestration::ForecastConfidence;
use codex_orchestration::ModelRoute;
use codex_orchestration::PermissionEnvelope;
use codex_orchestration::RouteSource;
use codex_orchestration::RouteStatus;
use codex_orchestration::TaskId;
use codex_orchestration::WorkAccess;
use codex_orchestration::WorkflowId;
use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;

fn workflow_id(value: &str) -> WorkflowId {
    WorkflowId::new(value).expect("valid workflow id")
}
fn task_id(value: &str) -> TaskId {
    TaskId::new(value).expect("valid task id")
}
fn agent_id(value: &str) -> codex_orchestration::AgentId {
    codex_orchestration::AgentId::new(value).expect("valid agent id")
}
fn bounded(value: &str) -> BoundedText {
    BoundedText::new(value).expect("bounded text")
}

fn handoff() -> codex_orchestration::StructuredHandoff {
    codex_orchestration::StructuredHandoff::new(
        workflow_id("workflow-1"),
        task_id("task-1"),
        agent_id("parent-agent"),
        AgentRole::Executor,
        bounded("bounded summary"),
        bounded("objective"),
        bounded("scope"),
        vec![bounded("finding")],
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

fn routes() -> (ModelRoute, EffortRoute) {
    (
        ModelRoute {
            requested: Some("requested-model".to_string()),
            resolved: Some("resolved-model".to_string()),
            source: RouteSource::Policy,
            status: RouteStatus::Resolved,
            data_quality: DataQuality::Derived,
        },
        EffortRoute {
            requested: None,
            resolved: None,
            source: RouteSource::Inherited,
            status: RouteStatus::Requested,
            data_quality: DataQuality::Unavailable,
        },
    )
}

fn spawn_request() -> SpawnChildRequest {
    let (model_route, effort_route) = routes();
    SpawnChildRequest::new(
        workflow_id("workflow-1"),
        task_id("task-1"),
        agent_id("child-agent"),
        Some(agent_id("parent-agent")),
        AgentRole::Explorer,
        WorkAccess::ReadOnly,
        model_route,
        effort_route,
        PermissionEnvelope::new(WorkAccess::Writer, WorkAccess::Writer, WorkAccess::ReadOnly)
            .expect("valid permission envelope"),
        handoff(),
    )
    .expect("valid spawn request")
}

#[test]
fn runtime_identity_round_trips_and_rejects_invalid_values() {
    let value = RuntimeAgentId::new("runtime-child").expect("runtime id");
    assert_eq!(
        serde_json::from_str::<RuntimeAgentId>(
            &serde_json::to_string(&value).expect("identity serializes"),
        )
        .expect("identity deserializes"),
        value
    );
    assert_eq!(RuntimeAgentId::new(""), Err(RuntimeIdentityError::Empty));
    assert_eq!(
        RuntimeAgentId::new("x".repeat(MAX_RUNTIME_ID_BYTES + 1)),
        Err(RuntimeIdentityError::TooLong)
    );
    assert!(
        serde_json::from_value::<RuntimeAgentId>(json!("x".repeat(MAX_RUNTIME_ID_BYTES + 1)))
            .is_err()
    );
}

#[test]
fn spawn_request_round_trips_and_preserves_attribution() {
    let value = spawn_request();
    let decoded = serde_json::from_str::<SpawnChildRequest>(
        &serde_json::to_string(&value).expect("spawn serializes"),
    )
    .expect("spawn deserializes");
    assert_eq!(decoded, value);
    assert_ne!(
        decoded.child_agent_id(),
        decoded.parent_agent_id().expect("parent")
    );
    assert_eq!(decoded.workflow_id().as_str(), "workflow-1");
    assert_eq!(decoded.task_id().as_str(), "task-1");
}

#[test]
fn spawn_rejects_self_parent_and_permission_conflict() {
    let request = spawn_request();
    let self_parent = SpawnChildRequest::new(
        request.workflow_id().clone(),
        request.task_id().clone(),
        request.child_agent_id().clone(),
        Some(request.child_agent_id().clone()),
        request.role(),
        request.access(),
        request.model_route().clone(),
        request.effort_route().clone(),
        request.permissions(),
        request.handoff().clone(),
    );
    assert_eq!(self_parent, Err(SpawnRequestError::SelfParent));
    let conflict = SpawnChildRequest::new(
        request.workflow_id().clone(),
        request.task_id().clone(),
        request.child_agent_id().clone(),
        request.parent_agent_id().cloned(),
        request.role(),
        WorkAccess::ReadOnly,
        request.model_route().clone(),
        request.effort_route().clone(),
        PermissionEnvelope::new(WorkAccess::Writer, WorkAccess::Writer, WorkAccess::Writer)
            .expect("envelope itself is valid"),
        request.handoff().clone(),
    );
    assert_eq!(conflict, Err(SpawnRequestError::AccessPermissionConflict));
}

#[test]
fn malformed_spawn_cannot_bypass_permission_or_handoff_bounds() {
    let mut value = serde_json::to_value(spawn_request()).expect("spawn serializes");
    value["access"] = json!("read_only");
    value["permissions"]["assignment_access"] = json!("writer");
    assert!(serde_json::from_value::<SpawnChildRequest>(value).is_err());
    let mut value = serde_json::to_value(spawn_request()).expect("spawn serializes");
    value["handoff"]["task_summary"] =
        json!("x".repeat(codex_orchestration::MAX_HANDOFF_TEXT_BYTES + 1));
    assert!(serde_json::from_value::<SpawnChildRequest>(value).is_err());
}

#[test]
fn spawn_result_preserves_orchestration_and_runtime_identity() {
    let value = SpawnChildResult {
        workflow_id: workflow_id("workflow-1"),
        task_id: task_id("task-1"),
        agent_id: agent_id("child-agent"),
        runtime_id: RuntimeAgentId::new("runtime-child").expect("runtime id"),
    };
    assert_eq!(
        serde_json::from_str::<SpawnChildResult>(
            &serde_json::to_string(&value).expect("result serializes"),
        )
        .expect("result deserializes"),
        value
    );
}

#[test]
fn handoff_request_and_result_round_trip_with_closed_outcome() {
    let request = DeliverHandoffRequest {
        workflow_id: workflow_id("workflow-1"),
        task_id: task_id("task-1"),
        agent_id: agent_id("child-agent"),
        runtime_id: RuntimeAgentId::new("runtime-child").expect("runtime id"),
        handoff: handoff(),
    };
    assert_eq!(
        serde_json::from_str::<DeliverHandoffRequest>(
            &serde_json::to_string(&request).expect("request serializes"),
        )
        .expect("request deserializes"),
        request
    );
    let result = DeliverHandoffResult {
        workflow_id: request.workflow_id,
        task_id: request.task_id,
        agent_id: request.agent_id,
        runtime_id: request.runtime_id,
        outcome: HandoffDeliveryOutcome::Accepted,
    };
    assert_eq!(
        serde_json::from_str::<DeliverHandoffResult>(
            &serde_json::to_string(&result).expect("result serializes"),
        )
        .expect("result deserializes"),
        result
    );
}
