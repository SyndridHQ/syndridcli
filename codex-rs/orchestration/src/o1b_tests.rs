use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;

fn ids() -> (WorkflowId, TaskId, AgentId) {
    (
        WorkflowId::new("workflow-1").expect("valid workflow id"),
        TaskId::new("task-1").expect("valid task id"),
        AgentId::new("agent-1").expect("valid agent id"),
    )
}

fn text(value: &str) -> BoundedText {
    BoundedText::new(value).expect("bounded test text")
}

fn handoff() -> StructuredHandoff {
    let (workflow_id, task_id, source_agent_id) = ids();
    StructuredHandoff::new(
        workflow_id,
        task_id,
        source_agent_id,
        AgentRole::Verifier,
        text("summary"),
        text("objective"),
        text("scope"),
        vec![text("finding")],
        vec![text("src/lib.rs")],
        vec![text("src/lib.rs")],
        vec![text("just test")],
        vec![text("passed")],
        vec![text("test-run-1")],
        vec![text("none")],
        vec![text("none")],
        ForecastConfidence::High,
        vec![text("none")],
        text("review evidence"),
        vec![text("run:test-1")],
        DataQuality::Derived,
    )
}

#[test]
fn structured_handoff_round_trips_and_preserves_attribution() {
    let value = handoff();
    let encoded = serde_json::to_string(&value).expect("handoff should serialize");
    let decoded =
        serde_json::from_str::<StructuredHandoff>(&encoded).expect("handoff should deserialize");

    assert_eq!(decoded, value);
    assert_eq!(decoded.workflow_id().as_str(), "workflow-1");
    assert_eq!(decoded.task_id().as_str(), "task-1");
    assert_eq!(decoded.source_agent_id().as_str(), "agent-1");
    assert_eq!(decoded.destination_role(), AgentRole::Verifier);
}

#[test]
fn malformed_handoff_text_cannot_bypass_bound() {
    let mut value = serde_json::to_value(handoff()).expect("handoff should serialize");
    value["task_summary"] = json!("x".repeat(MAX_HANDOFF_TEXT_BYTES + 1));
    assert!(serde_json::from_value::<StructuredHandoff>(value).is_err());
}

#[test]
fn recommendation_round_trip_keeps_requested_and_resolved_routes_distinct() {
    let requested = ModelRoute {
        requested: Some("requested-model".to_string()),
        resolved: None,
        source: RouteSource::User,
        status: RouteStatus::Requested,
        data_quality: DataQuality::Exact,
    };
    let resolved = ModelRoute {
        requested: Some("requested-model".to_string()),
        resolved: Some("resolved-model".to_string()),
        source: RouteSource::Fallback,
        status: RouteStatus::Rerouted,
        data_quality: DataQuality::Derived,
    };
    let forecast = Forecast {
        predicted_usage: Some(UsageQuantity::new(100)),
        predicted_orchestration_overhead: Some(UsageQuantity::new(5)),
        predicted_completion_time_ms: Some(1_000),
        predicted_latency_ms: Some(100),
        confidence: ForecastConfidence::Medium,
        data_quality: DataQuality::Estimated,
    };
    let value = Recommendation::new(
        OrchestrationMode::Recommended,
        vec![AgentProfile {
            role: AgentRole::Executor,
            requested_model: Some(requested.clone()),
            requested_effort: None,
            access: WorkAccess::Writer,
            role_label: Some("executor".to_string()),
        }],
        vec![resolved.clone()],
        Vec::new(),
        2,
        UsageBudgetMultiplier::BALANCED,
        forecast.clone(),
        vec![text("resolved after policy")],
        vec![text("provider availability")],
    )
    .expect("valid recommendation");

    let decoded = serde_json::from_str::<Recommendation>(
        &serde_json::to_string(&value).expect("recommendation should serialize"),
    )
    .expect("recommendation should deserialize");
    assert_eq!(decoded, value);
    assert_eq!(
        decoded.proposed_profiles()[0].requested_model,
        Some(requested)
    );
    assert_eq!(decoded.resolved_model_routes(), &[resolved]);
    assert_eq!(decoded.forecast(), &forecast);
}

#[test]
fn recommendation_notes_are_bounded_and_records_have_no_runtime_handle() {
    let forecast = Forecast {
        predicted_usage: None,
        predicted_orchestration_overhead: None,
        predicted_completion_time_ms: None,
        predicted_latency_ms: None,
        confidence: ForecastConfidence::Low,
        data_quality: DataQuality::Unavailable,
    };
    let result = Recommendation::new(
        OrchestrationMode::Single,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        1,
        UsageBudgetMultiplier::SINGLE,
        forecast,
        vec![text("x"); MAX_RECOMMENDATION_NOTES + 1],
        Vec::new(),
    );
    assert_eq!(result, Err(RecommendationError::TooManyNotes));
}

#[test]
fn verification_requirement_and_evidence_are_distinct_and_round_trip() {
    let requirement = VerificationRequirement::new(EvidenceKind::Test, true, text("run tests"));
    let evidence = VerificationEvidence::new(
        EvidenceKind::Test,
        text("test-run-1"),
        EvidenceResult::Passed,
        DataQuality::Exact,
    );
    let requirement_json = serde_json::to_string(&requirement).expect("requirement serializes");
    let evidence_json = serde_json::to_string(&evidence).expect("evidence serializes");
    assert_eq!(
        serde_json::from_str::<VerificationRequirement>(&requirement_json)
            .expect("requirement deserializes"),
        requirement
    );
    assert_eq!(
        serde_json::from_str::<VerificationEvidence>(&evidence_json)
            .expect("evidence deserializes"),
        evidence
    );
    assert_eq!(requirement.kind(), EvidenceKind::Test);
    assert_eq!(evidence.observed_result(), EvidenceResult::Passed);
    assert_ne!(
        requirement.description().as_str(),
        evidence.source_reference().as_str()
    );
}

#[test]
fn malformed_verification_evidence_cannot_bypass_text_bound() {
    let mut value = serde_json::to_value(VerificationEvidence::new(
        EvidenceKind::Manual,
        text("reference"),
        EvidenceResult::Inconclusive,
        DataQuality::Estimated,
    ))
    .expect("evidence serializes");
    value["source_reference"] = json!("x".repeat(MAX_HANDOFF_TEXT_BYTES + 1));
    assert!(serde_json::from_value::<VerificationEvidence>(value).is_err());
}

#[test]
fn workflow_event_round_trips_with_workflow_task_agent_attribution() {
    let (workflow_id, task_id, agent_id) = ids();
    let value = WorkflowEvent {
        workflow_id,
        task_id: Some(task_id),
        agent_id: Some(agent_id),
        sequence: 7,
        causation: Some(EventReference(3)),
        correlation: Some(EventReference(4)),
        kind: WorkflowEventKind::VerificationRecorded {
            state: VerificationState::Pending,
        },
    };
    let decoded = serde_json::from_str::<WorkflowEvent>(
        &serde_json::to_string(&value).expect("event should serialize"),
    )
    .expect("event should deserialize");
    assert_eq!(decoded, value);
    assert_eq!(decoded.workflow_id.as_str(), "workflow-1");
    assert_eq!(
        decoded.task_id.expect("task attribution").as_str(),
        "task-1"
    );
    assert_eq!(
        decoded.agent_id.expect("agent attribution").as_str(),
        "agent-1"
    );
}
