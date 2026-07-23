use super::*;
use pretty_assertions::assert_eq;

fn ids() -> (WorkflowId, TaskId, AgentId) {
    (
        WorkflowId::new("workflow-1").expect("valid workflow id"),
        TaskId::new("task-1").expect("valid task id"),
        AgentId::new("agent-1").expect("valid agent id"),
    )
}

#[test]
fn multiplier_presets_are_exact_and_formatted() {
    assert_eq!(UsageBudgetMultiplier::SINGLE.basis_points(), 10_000);
    assert_eq!(
        UsageBudgetMultiplier::LIGHT_ACCELERATION.basis_points(),
        11_000
    );
    assert_eq!(UsageBudgetMultiplier::BALANCED.basis_points(), 12_500);
    assert_eq!(UsageBudgetMultiplier::AGGRESSIVE.basis_points(), 15_000);
    assert_eq!(UsageBudgetMultiplier::MAXIMUM.basis_points(), 20_000);
    assert_eq!(UsageBudgetMultiplier::BALANCED.to_string(), "1.25×");
}

#[test]
fn multiplier_rejects_values_outside_initial_range() {
    assert_eq!(
        UsageBudgetMultiplier::new_basis_points(9_999),
        Err(MultiplierError::BelowMinimum)
    );
    assert_eq!(
        UsageBudgetMultiplier::new_basis_points(20_001),
        Err(MultiplierError::AboveMaximum)
    );
}

#[test]
fn multiplier_deserialization_reuses_range_validation() {
    assert_eq!(
        serde_json::from_str::<UsageBudgetMultiplier>(r#"{"basis_points":9999}"#)
            .expect_err("values below the minimum must fail")
            .to_string(),
        MultiplierError::BelowMinimum.to_string()
    );
    assert_eq!(
        serde_json::from_str::<UsageBudgetMultiplier>(r#"{"basis_points":20001}"#)
            .expect_err("values above the maximum must fail")
            .to_string(),
        MultiplierError::AboveMaximum.to_string()
    );

    let encoded = serde_json::to_string(&UsageBudgetMultiplier::BALANCED)
        .expect("valid multiplier should serialize");
    assert_eq!(
        serde_json::from_str::<UsageBudgetMultiplier>(&encoded)
            .expect("valid multiplier should deserialize"),
        UsageBudgetMultiplier::BALANCED
    );
}

#[test]
fn workflow_keeps_state_dimensions_independent_and_single_mode_empty() {
    let (workflow_id, _, _) = ids();
    let workflow = WorkflowRun {
        workflow_id,
        mode: OrchestrationMode::Single,
        lifecycle: RunLifecycleState::Waiting,
        stage: WorkflowStage::Verifying,
        wait_reason: Some(WaitReason::Approval),
        cancellation: CancellationState::NotRequested,
        verification: VerificationState::Pending,
        max_concurrency: WorkflowRun::INITIAL_MAX_CONCURRENCY,
        max_writers: WorkflowRun::INITIAL_MAX_WRITERS,
        assignments: Vec::new(),
        budget: None,
    };

    assert_eq!(workflow.mode, OrchestrationMode::Single);
    assert!(workflow.assignments.is_empty());
    assert_eq!(workflow.lifecycle, RunLifecycleState::Waiting);
    assert_eq!(workflow.stage, WorkflowStage::Verifying);
    assert_eq!(workflow.wait_reason, Some(WaitReason::Approval));
    assert_eq!(workflow.verification, VerificationState::Pending);
}

#[test]
fn access_and_permission_envelope_never_widen_parent() {
    assert!(WorkAccess::Writer.allows(WorkAccess::ReadOnly));
    assert!(!WorkAccess::ReadOnly.allows(WorkAccess::Writer));
    assert_eq!(
        PermissionEnvelope::new(WorkAccess::Writer, WorkAccess::ReadOnly, WorkAccess::Writer),
        Err(PermissionEnvelopeError::AssignmentExceedsParent)
    );
    let envelope =
        PermissionEnvelope::new(WorkAccess::Writer, WorkAccess::Writer, WorkAccess::ReadOnly)
            .expect("valid permission envelope");
    assert_eq!(envelope.workflow_ceiling(), WorkAccess::Writer);
    assert_eq!(envelope.parent_ceiling(), WorkAccess::Writer);
    assert_eq!(envelope.assignment_access(), WorkAccess::ReadOnly);
}

#[test]
fn permission_envelope_deserialization_reuses_non_widening_validation() {
    let forbidden = r#"{
        "workflow_ceiling":"writer",
        "parent_ceiling":"read_only",
        "assignment_access":"writer"
    }"#;
    assert!(serde_json::from_str::<PermissionEnvelope>(forbidden).is_err());

    let envelope =
        PermissionEnvelope::new(WorkAccess::Writer, WorkAccess::Writer, WorkAccess::ReadOnly)
            .expect("valid permission envelope");
    let encoded = serde_json::to_string(&envelope).expect("valid envelope should serialize");
    assert_eq!(
        serde_json::from_str::<PermissionEnvelope>(&encoded)
            .expect("valid envelope should deserialize"),
        envelope
    );
}

#[test]
fn routing_separates_requested_and_resolved_values() {
    let route = ModelRoute {
        requested: Some("model-a".to_string()),
        resolved: Some("model-b".to_string()),
        source: RouteSource::Fallback,
        status: RouteStatus::Rerouted,
        data_quality: DataQuality::Exact,
    };
    assert_ne!(route.requested, route.resolved);
    assert_eq!(route.status, RouteStatus::Rerouted);
}

#[test]
fn data_quality_round_trips() {
    let quality = DataQuality::Derived;
    let encoded = serde_json::to_string(&quality).expect("quality should serialize");
    assert_eq!(
        serde_json::from_str::<DataQuality>(&encoded).expect("quality should deserialize"),
        quality
    );
}
