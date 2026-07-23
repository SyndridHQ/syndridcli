use codex_orchestration::AgentId;
use codex_orchestration::BoundedText;
use codex_orchestration::CancellationState;
use codex_orchestration::DataQuality;
use codex_orchestration::EffortRoute;
use codex_orchestration::ForecastConfidence;
use codex_orchestration::ModelRoute;
use codex_orchestration::PermissionEnvelope;
use codex_orchestration::RouteSource;
use codex_orchestration::RouteStatus;
use codex_orchestration::RunLifecycleState;
use codex_orchestration::StructuredHandoff;
use codex_orchestration::TaskId;
use codex_orchestration::UsageQuantity;
use codex_orchestration::VerificationState;
use codex_orchestration::WaitReason;
use codex_orchestration::WorkAccess;
use codex_orchestration::WorkflowId;
use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;

fn workflow_id(value: &str) -> WorkflowId {
    WorkflowId::new(value).expect("workflow id")
}
fn task_id(value: &str) -> TaskId {
    TaskId::new(value).expect("task id")
}
fn agent_id(value: &str) -> AgentId {
    AgentId::new(value).expect("agent id")
}
fn bounded(value: &str) -> BoundedText {
    BoundedText::new(value).expect("bounded text")
}

fn handoff() -> StructuredHandoff {
    StructuredHandoff::new(
        workflow_id("workflow-1"),
        task_id("task-1"),
        agent_id("parent-agent"),
        codex_orchestration::AgentRole::Executor,
        bounded("summary"),
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
            requested: Some("requested".into()),
            resolved: Some("resolved".into()),
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

fn observation() -> ChildObservation {
    ChildObservation::new(
        workflow_id("workflow-1"),
        task_id("task-1"),
        agent_id("child-agent"),
        RuntimeAgentId::new("runtime-child").expect("runtime id"),
        RunLifecycleState::Waiting,
        Some(WaitReason::Approval),
        CancellationState::NotRequested,
        VerificationState::Pending,
        None,
        None,
        Some(UsageQuantity::new(17)),
        DataQuality::Exact,
        Some(bounded("awaiting approval")),
        Some(12),
    )
}

#[test]
fn cancellation_records_round_trip_without_claiming_completion() {
    let request = CancelChildRequest::new(
        workflow_id("workflow-1"),
        task_id("task-1"),
        agent_id("child-agent"),
        Some(RuntimeAgentId::new("runtime-child").expect("runtime id")),
        bounded("stop"),
        CancellationProvenance::User,
    );
    let result = CancelChildResult::new(
        request.workflow_id().clone(),
        request.task_id().clone(),
        request.agent_id().clone(),
        CancelChildOutcome::Requested,
    );
    assert_eq!(result.outcome(), CancelChildOutcome::Requested);
    assert_eq!(
        serde_json::from_str::<CancelChildRequest>(&serde_json::to_string(&request).unwrap())
            .unwrap(),
        request
    );
    assert_eq!(
        serde_json::from_str::<CancelChildResult>(&serde_json::to_string(&result).unwrap())
            .unwrap(),
        result
    );
    for outcome in [
        CancelChildOutcome::AlreadyTerminal,
        CancelChildOutcome::NotFound,
        CancelChildOutcome::Rejected,
        CancelChildOutcome::Unsupported,
    ] {
        assert_ne!(outcome, CancelChildOutcome::Requested);
    }
}

#[test]
fn observation_keeps_status_dimensions_and_quality_independent() {
    let value = observation();
    let decoded =
        serde_json::from_str::<ChildObservation>(&serde_json::to_string(&value).unwrap()).unwrap();
    assert_eq!(decoded, value);
    assert_eq!(value.lifecycle(), RunLifecycleState::Waiting);
    assert_eq!(value.wait_reason(), Some(WaitReason::Approval));
    assert_eq!(value.cancellation(), CancellationState::NotRequested);
    assert_eq!(value.verification(), VerificationState::Pending);
    assert_eq!(value.data_quality(), DataQuality::Exact);
    let mut malformed = serde_json::to_value(&value).unwrap();
    malformed["status_detail"] = json!("x".repeat(codex_orchestration::MAX_HANDOFF_TEXT_BYTES + 1));
    assert!(serde_json::from_value::<ChildObservation>(malformed).is_err());
}

#[test]
fn capabilities_are_explicit_and_the_integer_ceiling_is_bounded() {
    let value = AdapterCapabilities::new(
        true,
        true,
        true,
        true,
        true,
        false,
        true,
        true,
        Some(2),
        DataQuality::Derived,
    );
    assert_eq!(
        serde_json::from_str::<AdapterCapabilities>(&serde_json::to_string(&value).unwrap())
            .unwrap(),
        value
    );
    assert!(
        serde_json::from_value::<AdapterCapabilities>(json!({
            "supports_child_spawn": true, "supports_handoff_delivery": true,
            "supports_cancellation": true, "supports_observation": true,
            "supports_read_only_children": true, "supports_writer_children": false,
            "supports_model_override": true, "supports_effort_override": true,
            "max_supported_children": 65536, "data_quality": "exact"
        }))
        .is_err()
    );
}

#[test]
fn errors_are_typed_retry_metadata_with_bounded_safe_text() {
    let value = AdapterError::new(
        AdapterErrorKind::RuntimeUnavailable,
        bounded("unavailable"),
        Retryability::Retryable,
        DataQuality::Exact,
    )
    .with_attribution(
        Some(workflow_id("workflow-1")),
        Some(task_id("task-1")),
        Some(agent_id("child-agent")),
    );
    assert_eq!(
        serde_json::from_str::<AdapterError>(&serde_json::to_string(&value).unwrap()).unwrap(),
        value
    );
    assert_eq!(value.kind(), AdapterErrorKind::RuntimeUnavailable);
    assert_eq!(value.retryability(), Retryability::Retryable);
    let mut malformed = serde_json::to_value(&value).unwrap();
    malformed["message"] = json!("x".repeat(codex_orchestration::MAX_HANDOFF_TEXT_BYTES + 1));
    assert!(serde_json::from_value::<AdapterError>(malformed).is_err());
}

#[test]
fn operation_envelopes_round_trip_without_execution_pairing_logic() {
    let (model_route, effort_route) = routes();
    let spawn = SpawnChildRequest::new(
        workflow_id("workflow-1"),
        task_id("task-1"),
        agent_id("child-agent"),
        Some(agent_id("parent-agent")),
        codex_orchestration::AgentRole::Explorer,
        WorkAccess::ReadOnly,
        model_route,
        effort_route,
        PermissionEnvelope::new(WorkAccess::Writer, WorkAccess::Writer, WorkAccess::ReadOnly)
            .unwrap(),
        handoff(),
    )
    .unwrap();
    let request = AdapterRequest::new(7, AdapterRequestKind::SpawnChild(spawn));
    let response = AdapterResponse::new(7, AdapterResponseKind::ObserveChild(observation()));
    assert_eq!(
        serde_json::from_str::<AdapterRequest>(&serde_json::to_string(&request).unwrap()).unwrap(),
        request
    );
    assert_eq!(
        serde_json::from_str::<AdapterResponse>(&serde_json::to_string(&response).unwrap())
            .unwrap(),
        response
    );
}
