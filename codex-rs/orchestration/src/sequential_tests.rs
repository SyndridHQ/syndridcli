use super::*;
use pretty_assertions::assert_eq;

fn stage_output(correlation: StageCorrelation, handoff: StructuredHandoff) -> StageOutput {
    StageOutput {
        correlation,
        result: StageResult::Succeeded { handoff },
    }
}

fn stage_failure(correlation: StageCorrelation, code: StageFailureCode) -> StageOutput {
    StageOutput {
        correlation,
        result: StageResult::Failed { code },
    }
}

fn workflow() -> SequentialWorkflow {
    SequentialWorkflow::new(
        WorkflowId::new("workflow-1").expect("workflow id"),
        TaskId::new("task-1").expect("task id"),
        PermissionEnvelope::new(WorkAccess::Writer, WorkAccess::Writer, WorkAccess::Writer)
            .expect("permission ceiling"),
    )
    .expect("static stage ids")
}

fn handoff_for(workflow_id: &str, task_id: &str, stage: AgentRole) -> StructuredHandoff {
    StructuredHandoff::new(
        WorkflowId::new(workflow_id).expect("workflow id"),
        TaskId::new(task_id).expect("task id"),
        AgentId::new("agent-1").expect("agent id"),
        stage,
        BoundedText::new("bounded summary").expect("summary"),
        BoundedText::new("bounded objective").expect("objective"),
        BoundedText::new("bounded scope").expect("scope"),
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
        BoundedText::new("continue").expect("next action"),
        Vec::new(),
        DataQuality::Exact,
    )
}

fn handoff(stage: AgentRole) -> StructuredHandoff {
    handoff_for("workflow-1", "task-1", stage)
}

fn input(stage: SequentialStage, access: WorkAccess, role: AgentRole) -> StageInput {
    let stage_id = match stage {
        SequentialStage::Planner => "planner",
        SequentialStage::Executor => "executor",
        SequentialStage::Verifier => "verifier",
    };
    StageInput {
        correlation: StageCorrelation {
            workflow_id: WorkflowId::new("workflow-1").expect("workflow id"),
            task_id: TaskId::new("task-1").expect("task id"),
            stage_id: StageId::new(stage_id).expect("stage id"),
        },
        role,
        access,
        permissions: PermissionEnvelope::new(WorkAccess::Writer, WorkAccess::Writer, access)
            .expect("permission envelope"),
        handoff: handoff(role),
    }
}

struct EchoExecutor;

impl StageExecutor for EchoExecutor {
    fn execute(&self, input: StageInput) -> StageOutput {
        stage_output(input.correlation, input.handoff)
    }
}

#[test]
fn stages_are_sequential_and_complete_in_order() {
    let mut workflow = workflow();
    let executor = EchoExecutor;
    for (stage, role, access) in [
        (
            SequentialStage::Planner,
            AgentRole::Planner,
            WorkAccess::ReadOnly,
        ),
        (
            SequentialStage::Executor,
            AgentRole::Executor,
            WorkAccess::Writer,
        ),
        (
            SequentialStage::Verifier,
            AgentRole::Verifier,
            WorkAccess::ReadOnly,
        ),
    ] {
        workflow
            .execute_next(input(stage, access, role), &executor)
            .expect("next stage should execute");
    }
    assert_eq!(workflow.state(), SequentialWorkflowState::Succeeded);
    assert_eq!(workflow.active_stage(), None);
}

#[test]
fn all_stage_lookups_are_structural() {
    let workflow = workflow();
    for (stage, id) in [
        (SequentialStage::Planner, "planner"),
        (SequentialStage::Executor, "executor"),
        (SequentialStage::Verifier, "verifier"),
    ] {
        assert_eq!(workflow.stage_id(stage).map(StageId::as_str), Some(id));
        assert_eq!(workflow.stage_state(stage), Some(StageState::Pending));
    }
}

#[test]
fn only_one_stage_can_be_active_and_out_of_order_stages_are_rejected() {
    let mut workflow = workflow();
    let planner = input(
        SequentialStage::Planner,
        WorkAccess::ReadOnly,
        AgentRole::Planner,
    );
    workflow.begin_stage(&planner).expect("planner can start");
    assert_eq!(
        workflow.begin_stage(&planner),
        Err(SequentialWorkflowError::StageAlreadyActive)
    );
    let executor = input(
        SequentialStage::Executor,
        WorkAccess::Writer,
        AgentRole::Executor,
    );
    assert_eq!(
        workflow.complete_stage(&stage_output(
            executor.correlation.clone(),
            executor.handoff.clone()
        )),
        Err(SequentialWorkflowError::OutputCorrelationMismatch)
    );
}

#[test]
fn successful_output_with_mismatched_workflow_handoff_is_rejected_atomically() {
    let mut workflow = workflow();
    let planner = input(
        SequentialStage::Planner,
        WorkAccess::ReadOnly,
        AgentRole::Planner,
    );
    workflow.begin_stage(&planner).expect("planner can start");
    let output = stage_output(
        planner.correlation.clone(),
        handoff_for("workflow-2", "task-1", AgentRole::Planner),
    );
    assert_eq!(
        workflow.complete_stage(&output),
        Err(SequentialWorkflowError::SuccessfulHandoffCorrelationMismatch)
    );
    assert_eq!(workflow.active_stage(), Some(SequentialStage::Planner));
    assert_eq!(workflow.state(), SequentialWorkflowState::Running);
    assert_eq!(
        workflow.stage_state(SequentialStage::Planner),
        Some(StageState::Active)
    );
}

#[test]
fn successful_output_with_mismatched_task_handoff_is_rejected_atomically() {
    let mut workflow = workflow();
    let planner = input(
        SequentialStage::Planner,
        WorkAccess::ReadOnly,
        AgentRole::Planner,
    );
    workflow.begin_stage(&planner).expect("planner can start");
    let output = stage_output(
        planner.correlation.clone(),
        handoff_for("workflow-1", "task-2", AgentRole::Planner),
    );
    assert_eq!(
        workflow.complete_stage(&output),
        Err(SequentialWorkflowError::SuccessfulHandoffCorrelationMismatch)
    );
    assert_eq!(workflow.active_stage(), Some(SequentialStage::Planner));
    assert_eq!(workflow.state(), SequentialWorkflowState::Running);
    assert_eq!(
        workflow.stage_state(SequentialStage::Planner),
        Some(StageState::Active)
    );
}

#[test]
fn role_access_and_handoff_correlation_are_enforced() {
    let mut workflow = workflow();
    let mut planner = input(
        SequentialStage::Planner,
        WorkAccess::Writer,
        AgentRole::Planner,
    );
    assert_eq!(
        workflow.begin_stage(&planner),
        Err(SequentialWorkflowError::StagePolicyMismatch)
    );
    planner.access = WorkAccess::ReadOnly;
    planner.handoff = handoff_for("workflow-2", "task-1", AgentRole::Planner);
    assert_eq!(
        workflow.begin_stage(&planner),
        Err(SequentialWorkflowError::HandoffCorrelationMismatch)
    );
}

#[test]
fn permissions_cannot_exceed_the_workflow_parent_envelope() {
    let mut workflow = SequentialWorkflow::new(
        WorkflowId::new("workflow-1").expect("workflow id"),
        TaskId::new("task-1").expect("task id"),
        PermissionEnvelope::new(
            WorkAccess::ReadOnly,
            WorkAccess::ReadOnly,
            WorkAccess::ReadOnly,
        )
        .expect("read-only ceiling"),
    )
    .expect("static stage ids");
    let planner = input(
        SequentialStage::Planner,
        WorkAccess::ReadOnly,
        AgentRole::Planner,
    );
    assert_eq!(
        workflow.begin_stage(&planner),
        Err(SequentialWorkflowError::PermissionCeilingExceeded)
    );
    assert_eq!(workflow.active_stage(), None);
    assert_eq!(workflow.state(), SequentialWorkflowState::Ready);
}

#[test]
fn failure_is_bounded_and_stops_the_workflow() {
    let mut workflow = workflow();
    let planner = input(
        SequentialStage::Planner,
        WorkAccess::ReadOnly,
        AgentRole::Planner,
    );
    workflow.begin_stage(&planner).expect("planner can start");
    workflow
        .complete_stage(&stage_failure(
            planner.correlation.clone(),
            StageFailureCode::StageFailed,
        ))
        .expect("failure can complete active stage");
    assert_eq!(workflow.state(), SequentialWorkflowState::Failed);
    assert_eq!(
        workflow.stage_state(SequentialStage::Planner),
        Some(StageState::Failed)
    );
}
