use super::CodexOrchestrationAdapter;
use crate::AgentControl;
use crate::config::Config;
use codex_orchestration::AgentId;
use codex_orchestration::AgentRole;
use codex_orchestration::BoundedText;
use codex_orchestration::DataQuality;
use codex_orchestration::EffortRoute;
use codex_orchestration::ForecastConfidence;
use codex_orchestration::ModelRoute;
use codex_orchestration::PermissionEnvelope;
use codex_orchestration::RouteStatus;
use codex_orchestration::SequentialStage;
use codex_orchestration::SequentialWorkflow;
use codex_orchestration::SequentialWorkflowState;
use codex_orchestration::StageCorrelation;
use codex_orchestration::StageFailureCode;
use codex_orchestration::StageInput;
use codex_orchestration::StructuredHandoff;
use codex_orchestration::TaskId;
use codex_orchestration::WorkAccess;
use codex_orchestration::WorkflowId;
use codex_protocol::ThreadId;
use codex_protocol::protocol::SessionSource;
use serde::Serialize;

pub(super) const MAX_ORCHESTRATED_TASK_BYTES: usize = 16 * 1024;
const MAX_ROUTE_IDENTIFIER_BYTES: usize = 128;

/// Provider-neutral capability declarations supplied by a route selector.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(super) struct RouteCapabilities {
    pub(super) text_generation: bool,
    pub(super) tool_calling: bool,
    pub(super) structured_output: bool,
    pub(super) read_only: bool,
    pub(super) writer: bool,
    pub(super) minimum_context_tokens: Option<u32>,
}

/// Bounded provider/model metadata shared by a role assignment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct ProviderNeutralRoute {
    pub(super) provider: String,
    pub(super) model: ModelRoute,
    pub(super) effort: EffortRoute,
    pub(super) capabilities: RouteCapabilities,
}

#[derive(Clone)]
pub(super) struct ParentExecutionContext {
    pub(super) agent_control: AgentControl,
    pub(super) parent_thread_id: ThreadId,
    pub(super) parent_session_source: SessionSource,
}

pub(super) struct RunOrchestratedTaskRequest {
    pub(super) task: String,
    pub(super) workflow_id: WorkflowId,
    pub(super) task_id: TaskId,
    pub(super) planner_agent_id: AgentId,
    pub(super) executor_agent_id: AgentId,
    pub(super) initial_verifier_agent_id: AgentId,
    pub(super) repair_executor_agent_id: AgentId,
    pub(super) final_verifier_agent_id: AgentId,
    pub(super) parent: ParentExecutionContext,
    pub(super) base_config: Config,
    pub(super) permission_ceiling: PermissionEnvelope,
    pub(super) planner_route: ProviderNeutralRoute,
    pub(super) executor_route: ProviderNeutralRoute,
    pub(super) verifier_route: ProviderNeutralRoute,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(super) enum OrchestratedTaskOutcome {
    SucceededInitialVerification,
    SucceededAfterRepair,
    RejectedAfterRepair,
    InvalidRequest,
    InfrastructureFailure,
    OutputPolicyFailure,
    CorrelationFailure,
    InvariantFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(super) enum TerminalWorkflowStatus {
    Succeeded,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(super) struct ExecutionSummary {
    pub(super) spawn_count: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct RouteMetadata {
    pub(super) provider: String,
    pub(super) model: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct RunOrchestratedTaskResult {
    pub(super) workflow_id: WorkflowId,
    pub(super) task_id: TaskId,
    pub(super) terminal_status: TerminalWorkflowStatus,
    pub(super) outcome: OrchestratedTaskOutcome,
    pub(super) repair_attempted: bool,
    pub(super) final_summary: Option<BoundedText>,
    pub(super) failing_stage: Option<SequentialStage>,
    pub(super) logical_agent_id: Option<AgentId>,
    pub(super) failure_code: Option<StageFailureCode>,
    pub(super) routes: [RouteMetadata; 3],
    pub(super) execution: ExecutionSummary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ValidationError {
    InvalidRequest,
}

#[derive(Clone)]
struct Assignments {
    values: [super::live::StageAssignment; 5],
}

impl RunOrchestratedTaskRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.task.trim().is_empty() || self.task.len() > MAX_ORCHESTRATED_TASK_BYTES {
            return Err(ValidationError::InvalidRequest);
        }
        let agent_ids = [
            &self.planner_agent_id,
            &self.executor_agent_id,
            &self.initial_verifier_agent_id,
            &self.repair_executor_agent_id,
            &self.final_verifier_agent_id,
        ];
        if !valid_identifier(self.workflow_id.as_str())
            || !valid_identifier(self.task_id.as_str())
            || agent_ids
                .iter()
                .any(|agent_id| !valid_identifier(agent_id.as_str()))
            || agent_ids
                .iter()
                .enumerate()
                .any(|(index, agent_id)| agent_ids[..index].contains(agent_id))
        {
            return Err(ValidationError::InvalidRequest);
        }
        if !valid_route(&self.planner_route)
            || !valid_route(&self.executor_route)
            || !valid_route(&self.verifier_route)
            || !valid_permission_ceiling(self.permission_ceiling)
            || !self.executor_route.capabilities.writer
        {
            return Err(ValidationError::InvalidRequest);
        }
        for (route, role) in [
            (&self.planner_route, AgentRole::Planner),
            (&self.executor_route, AgentRole::Executor),
            (&self.verifier_route, AgentRole::Verifier),
        ] {
            if !route_supports_role(route, role) {
                return Err(ValidationError::InvalidRequest);
            }
        }
        if !self
            .permission_ceiling
            .assignment_access()
            .allows(WorkAccess::Writer)
        {
            return Err(ValidationError::InvalidRequest);
        }
        Ok(())
    }

    fn assignments(&self) -> Result<Assignments, ValidationError> {
        self.validate()?;
        let permissions = |access| {
            PermissionEnvelope::new(
                self.permission_ceiling.workflow_ceiling(),
                self.permission_ceiling.parent_ceiling(),
                access,
            )
            .map_err(|_| ValidationError::InvalidRequest)
        };
        Ok(Assignments {
            values: [
                assignment(
                    self.planner_agent_id.clone(),
                    AgentRole::Planner,
                    WorkAccess::ReadOnly,
                    permissions(WorkAccess::ReadOnly)?,
                    &self.planner_route,
                ),
                assignment(
                    self.executor_agent_id.clone(),
                    AgentRole::Executor,
                    WorkAccess::Writer,
                    permissions(WorkAccess::Writer)?,
                    &self.executor_route,
                ),
                assignment(
                    self.initial_verifier_agent_id.clone(),
                    AgentRole::Verifier,
                    WorkAccess::ReadOnly,
                    permissions(WorkAccess::ReadOnly)?,
                    &self.verifier_route,
                ),
                assignment(
                    self.repair_executor_agent_id.clone(),
                    AgentRole::Executor,
                    WorkAccess::Writer,
                    permissions(WorkAccess::Writer)?,
                    &self.executor_route,
                ),
                assignment(
                    self.final_verifier_agent_id.clone(),
                    AgentRole::Verifier,
                    WorkAccess::ReadOnly,
                    permissions(WorkAccess::ReadOnly)?,
                    &self.verifier_route,
                ),
            ],
        })
    }
}

pub(super) async fn run_orchestrated_task(
    request: RunOrchestratedTaskRequest,
) -> RunOrchestratedTaskResult {
    let route_metadata = route_metadata(&request);
    if request.validate().is_err() {
        return invalid_result(&request, route_metadata);
    }
    let Ok(assignments) = request.assignments() else {
        return invalid_result(&request, route_metadata);
    };
    let Ok(workflow) = SequentialWorkflow::new(
        request.workflow_id.clone(),
        request.task_id.clone(),
        request.permission_ceiling,
    ) else {
        return invalid_result(&request, route_metadata);
    };
    let initial_input = initial_input(&request, &assignments.values[0]);
    let adapter = CodexOrchestrationAdapter::new(
        request.parent.agent_control.clone(),
        request.base_config.clone(),
        request.parent.parent_thread_id,
        request.parent.parent_session_source.clone(),
    );
    let workflow = adapter
        .run_sequential_workflow(workflow, initial_input, assignments.values)
        .await;
    map_workflow(&request, workflow, route_metadata)
}

fn assignment(
    agent_id: AgentId,
    role: AgentRole,
    access: WorkAccess,
    permissions: PermissionEnvelope,
    route: &ProviderNeutralRoute,
) -> super::live::StageAssignment {
    super::live::StageAssignment {
        agent_id,
        role,
        provider: route.provider.clone(),
        access,
        permissions,
        model_route: route.model.clone(),
        effort_route: route.effort.clone(),
    }
}

fn initial_input(
    request: &RunOrchestratedTaskRequest,
    planner: &super::live::StageAssignment,
) -> StageInput {
    let task = BoundedText::new(request.task.clone()).expect("validated task is bounded");
    let handoff = StructuredHandoff::new(
        request.workflow_id.clone(),
        request.task_id.clone(),
        planner.agent_id.clone(),
        AgentRole::Planner,
        task.clone(),
        bounded("orchestrated task"),
        bounded("request scope"),
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
        bounded("plan"),
        Vec::new(),
        DataQuality::Exact,
    );
    StageInput {
        correlation: StageCorrelation {
            workflow_id: request.workflow_id.clone(),
            task_id: request.task_id.clone(),
            stage_id: codex_orchestration::StageId::new("planner")
                .expect("static stage id is valid"),
        },
        role: planner.role,
        access: planner.access,
        permissions: planner.permissions,
        handoff,
    }
}

fn map_workflow(
    request: &RunOrchestratedTaskRequest,
    workflow: SequentialWorkflow,
    routes: [RouteMetadata; 3],
) -> RunOrchestratedTaskResult {
    let repair_attempted = matches!(
        workflow.stage_state(SequentialStage::RepairExecutor),
        Some(codex_orchestration::StageState::Succeeded)
            | Some(codex_orchestration::StageState::Rejected)
            | Some(codex_orchestration::StageState::Failed)
    );
    let summary = workflow
        .terminal_handoff()
        .map(|handoff| handoff.task_summary().clone());
    let (outcome, status, failure_code, failing_stage, logical_agent_id) = if workflow.state()
        == SequentialWorkflowState::Succeeded
        && !repair_attempted
    {
        (
            OrchestratedTaskOutcome::SucceededInitialVerification,
            TerminalWorkflowStatus::Succeeded,
            None,
            None,
            None,
        )
    } else if workflow.state() == SequentialWorkflowState::Succeeded {
        (
            OrchestratedTaskOutcome::SucceededAfterRepair,
            TerminalWorkflowStatus::Succeeded,
            None,
            None,
            None,
        )
    } else if workflow.stage_state(SequentialStage::FinalVerifier)
        == Some(codex_orchestration::StageState::Rejected)
    {
        (
            OrchestratedTaskOutcome::RejectedAfterRepair,
            TerminalWorkflowStatus::Failed,
            None,
            Some(SequentialStage::FinalVerifier),
            Some(request.final_verifier_agent_id.clone()),
        )
    } else if let Some((stage, code)) = workflow.terminal_failure() {
        let outcome = match code {
            StageFailureCode::RuntimeUnavailable | StageFailureCode::PermissionDenied => {
                OrchestratedTaskOutcome::InfrastructureFailure
            }
            StageFailureCode::OutputRejected
            | StageFailureCode::StageFailed
            | StageFailureCode::VerificationFailed => OrchestratedTaskOutcome::OutputPolicyFailure,
            StageFailureCode::CorrelationFailure => OrchestratedTaskOutcome::CorrelationFailure,
            StageFailureCode::InvariantFailure | StageFailureCode::InvalidInput => {
                OrchestratedTaskOutcome::InvariantFailure
            }
        };
        (
            outcome,
            TerminalWorkflowStatus::Failed,
            Some(code),
            Some(stage),
            Some(agent_for_stage(request, stage)),
        )
    } else {
        (
            OrchestratedTaskOutcome::InvariantFailure,
            TerminalWorkflowStatus::Failed,
            None,
            None,
            None,
        )
    };
    RunOrchestratedTaskResult {
        workflow_id: request.workflow_id.clone(),
        task_id: request.task_id.clone(),
        terminal_status: status,
        outcome,
        repair_attempted,
        final_summary: summary,
        failing_stage,
        logical_agent_id,
        failure_code,
        routes,
        execution: ExecutionSummary {
            spawn_count: execution_count(&workflow),
        },
    }
}

fn execution_count(workflow: &SequentialWorkflow) -> u8 {
    [
        SequentialStage::Planner,
        SequentialStage::Executor,
        SequentialStage::Verifier,
        SequentialStage::RepairExecutor,
        SequentialStage::FinalVerifier,
    ]
    .into_iter()
    .filter(|stage| workflow.stage_state(*stage) != Some(codex_orchestration::StageState::Pending))
    .count() as u8
}

fn agent_for_stage(request: &RunOrchestratedTaskRequest, stage: SequentialStage) -> AgentId {
    match stage {
        SequentialStage::Planner => request.planner_agent_id.clone(),
        SequentialStage::Executor => request.executor_agent_id.clone(),
        SequentialStage::Verifier => request.initial_verifier_agent_id.clone(),
        SequentialStage::RepairExecutor => request.repair_executor_agent_id.clone(),
        SequentialStage::FinalVerifier => request.final_verifier_agent_id.clone(),
    }
}

fn route_supports_role(route: &ProviderNeutralRoute, role: AgentRole) -> bool {
    let capabilities = route.capabilities;
    if !capabilities.text_generation {
        return false;
    }
    match role {
        AgentRole::Planner => capabilities.read_only,
        AgentRole::Executor => capabilities.tool_calling && capabilities.writer,
        AgentRole::Verifier => capabilities.structured_output && capabilities.read_only,
        AgentRole::Explorer => false,
    }
}

fn valid_route(route: &ProviderNeutralRoute) -> bool {
    valid_identifier(&route.provider)
        && route
            .model
            .resolved
            .as_deref()
            .is_some_and(valid_identifier)
        && route.effort.resolved.is_some()
        && route.effort.status == RouteStatus::Resolved
        && route.model.status == RouteStatus::Resolved
        && route
            .capabilities
            .minimum_context_tokens
            .is_none_or(|tokens| tokens > 0)
}

fn valid_identifier(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_ROUTE_IDENTIFIER_BYTES
}

fn valid_permission_ceiling(permissions: PermissionEnvelope) -> bool {
    permissions
        .workflow_ceiling()
        .allows(permissions.parent_ceiling())
        && permissions
            .workflow_ceiling()
            .allows(permissions.assignment_access())
        && permissions
            .parent_ceiling()
            .allows(permissions.assignment_access())
}

fn route_metadata(request: &RunOrchestratedTaskRequest) -> [RouteMetadata; 3] {
    [
        &request.planner_route,
        &request.executor_route,
        &request.verifier_route,
    ]
    .map(|route| RouteMetadata {
        provider: route.provider.clone(),
        model: route.model.resolved.clone(),
    })
}

fn invalid_result(
    request: &RunOrchestratedTaskRequest,
    routes: [RouteMetadata; 3],
) -> RunOrchestratedTaskResult {
    RunOrchestratedTaskResult {
        workflow_id: request.workflow_id.clone(),
        task_id: request.task_id.clone(),
        terminal_status: TerminalWorkflowStatus::Failed,
        outcome: OrchestratedTaskOutcome::InvalidRequest,
        repair_attempted: false,
        final_summary: None,
        failing_stage: None,
        logical_agent_id: None,
        failure_code: None,
        routes,
        execution: ExecutionSummary { spawn_count: 0 },
    }
}

fn bounded(value: &str) -> BoundedText {
    BoundedText::new(value).expect("static invocation text is bounded")
}

#[cfg(test)]
#[path = "invocation_tests.rs"]
mod invocation_tests;
