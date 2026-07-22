use std::future::Future;
use std::pin::Pin;

use super::CodexOrchestrationAdapter;
use super::TerminalSnapshot;
use codex_orchestration::AgentId;
use codex_orchestration::AgentRole;
use codex_orchestration::BoundedText;
use codex_orchestration::DataQuality;
use codex_orchestration::EffortRoute;
use codex_orchestration::ForecastConfidence;
use codex_orchestration::ModelRoute;
use codex_orchestration::PermissionEnvelope;
use codex_orchestration::SequentialStage;
use codex_orchestration::SequentialWorkflow;
use codex_orchestration::StageCorrelation;
use codex_orchestration::StageFailureCode;
use codex_orchestration::StageInput;
use codex_orchestration::StageOutput;
use codex_orchestration::StructuredHandoff;
use codex_orchestration::WorkAccess;
use codex_orchestration_adapter::AdapterError;
use codex_orchestration_adapter::RuntimeAgentId;
use codex_orchestration_adapter::SpawnChildRequest;
use codex_orchestration_adapter::SpawnChildResult;
use codex_protocol::protocol::AgentStatus;

pub(super) type RuntimeFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, AdapterError>> + Send + 'a>>;

pub(super) trait SequentialRuntime: Send + Sync {
    fn spawn_child<'a>(&'a self, request: SpawnChildRequest)
    -> RuntimeFuture<'a, SpawnChildResult>;

    fn wait_for_terminal<'a>(
        &'a self,
        runtime_id: RuntimeAgentId,
        workflow_id: &'a codex_orchestration::WorkflowId,
        task_id: &'a codex_orchestration::TaskId,
        agent_id: &'a AgentId,
    ) -> RuntimeFuture<'a, TerminalSnapshot>;
}

impl SequentialRuntime for CodexOrchestrationAdapter {
    fn spawn_child<'a>(
        &'a self,
        request: SpawnChildRequest,
    ) -> RuntimeFuture<'a, SpawnChildResult> {
        Box::pin(async move { self.spawn_child(request).await })
    }

    fn wait_for_terminal<'a>(
        &'a self,
        runtime_id: RuntimeAgentId,
        workflow_id: &'a codex_orchestration::WorkflowId,
        task_id: &'a codex_orchestration::TaskId,
        agent_id: &'a AgentId,
    ) -> RuntimeFuture<'a, TerminalSnapshot> {
        Box::pin(async move {
            self.wait_for_terminal(runtime_id, (workflow_id, task_id, agent_id))
                .await
        })
    }
}

pub(super) struct SequentialRunner<'a, R> {
    runtime: &'a R,
    workflow: SequentialWorkflow,
    active_runtime_id: Option<RuntimeAgentId>,
}

impl<'a, R: SequentialRuntime> SequentialRunner<'a, R> {
    pub(super) fn new(runtime: &'a R, workflow: SequentialWorkflow) -> Self {
        Self {
            runtime,
            workflow,
            active_runtime_id: None,
        }
    }

    pub(super) async fn run(
        &mut self,
        initial_input: StageInput,
        assignments: [StageAssignment; 3],
    ) -> SequentialWorkflow {
        let mut handoff = initial_input.handoff.clone();
        let mut input = initial_input;
        for (index, assignment) in assignments.into_iter().enumerate() {
            if index > 0 {
                input = assignment.input(&self.workflow, &handoff, index);
            }
            let output = self.run_stage(&input, &assignment).await;
            let succeeded = matches!(
                &output.result,
                codex_orchestration::StageResult::Succeeded { .. }
            );
            handoff = match output.result {
                codex_orchestration::StageResult::Succeeded { handoff } => handoff,
                codex_orchestration::StageResult::Failed { .. } => handoff,
            };
            if !succeeded {
                break;
            }
        }
        self.workflow.clone()
    }

    async fn run_stage(&mut self, input: &StageInput, assignment: &StageAssignment) -> StageOutput {
        let correlation = input.correlation.clone();
        if self.workflow.begin_stage(&input).is_err() {
            return stage_failure(
                &mut self.workflow,
                correlation,
                StageFailureCode::InvalidInput,
            );
        }

        let request = match SpawnChildRequest::new(
            correlation.workflow_id.clone(),
            correlation.task_id.clone(),
            assignment.agent_id.clone(),
            None,
            assignment.role,
            assignment.access,
            assignment.model_route.clone(),
            assignment.effort_route.clone(),
            assignment.permissions,
            input.handoff.clone(),
        ) {
            Ok(request) => request,
            Err(_) => {
                return stage_failure(
                    &mut self.workflow,
                    correlation,
                    StageFailureCode::InvalidInput,
                );
            }
        };
        let spawned = match self.runtime.spawn_child(request).await {
            Ok(result) => result,
            Err(_) => {
                return stage_failure(
                    &mut self.workflow,
                    correlation,
                    StageFailureCode::RuntimeUnavailable,
                );
            }
        };
        if spawned.workflow_id != correlation.workflow_id
            || spawned.task_id != correlation.task_id
            || spawned.agent_id != assignment.agent_id
        {
            return stage_failure(
                &mut self.workflow,
                correlation,
                StageFailureCode::OutputRejected,
            );
        }
        self.active_runtime_id = Some(spawned.runtime_id.clone());
        let snapshot = match self
            .runtime
            .wait_for_terminal(
                spawned.runtime_id.clone(),
                &correlation.workflow_id,
                &correlation.task_id,
                &assignment.agent_id,
            )
            .await
        {
            Ok(snapshot) => snapshot,
            Err(_) => {
                return self.finish_failure(correlation, StageFailureCode::RuntimeUnavailable);
            }
        };
        if self.active_runtime_id.as_ref() != Some(&snapshot.runtime_id)
            || snapshot.runtime_id != spawned.runtime_id
        {
            return self.finish_failure(correlation, StageFailureCode::OutputRejected);
        }
        let output = match bounded_stage_output(snapshot, correlation.clone(), &assignment) {
            Ok(output) => output,
            Err(code) => self.finish_failure(correlation.clone(), code),
        };
        if matches!(
            &output.result,
            codex_orchestration::StageResult::Succeeded { .. }
        ) && self.workflow.complete_stage(&output).is_err()
        {
            return self.finish_failure(correlation, StageFailureCode::OutputRejected);
        }
        self.active_runtime_id = None;
        output
    }

    fn finish_failure(
        &mut self,
        correlation: StageCorrelation,
        code: StageFailureCode,
    ) -> StageOutput {
        let output = StageOutput {
            correlation,
            result: codex_orchestration::StageResult::Failed { code },
        };
        let _ = self.workflow.complete_stage(&output);
        self.active_runtime_id = None;
        output
    }
}

#[derive(Clone)]
pub(super) struct StageAssignment {
    pub(super) agent_id: AgentId,
    pub(super) role: AgentRole,
    pub(super) access: WorkAccess,
    pub(super) permissions: PermissionEnvelope,
    pub(super) model_route: ModelRoute,
    pub(super) effort_route: EffortRoute,
}

impl StageAssignment {
    fn input(
        &self,
        workflow: &SequentialWorkflow,
        handoff: &StructuredHandoff,
        index: usize,
    ) -> StageInput {
        let stage = [
            SequentialStage::Planner,
            SequentialStage::Executor,
            SequentialStage::Verifier,
        ][index];
        StageInput {
            correlation: StageCorrelation {
                workflow_id: workflow.workflow_id().clone(),
                task_id: workflow.task_id().clone(),
                stage_id: workflow.stage_id(stage).expect("fixed stage id").clone(),
            },
            role: self.role,
            access: self.access,
            permissions: self.permissions,
            handoff: handoff.clone(),
        }
    }
}

pub(super) fn bounded_stage_output(
    snapshot: TerminalSnapshot,
    correlation: StageCorrelation,
    assignment: &StageAssignment,
) -> Result<StageOutput, StageFailureCode> {
    let AgentStatus::Completed(Some(message)) = snapshot.status else {
        return Err(StageFailureCode::StageFailed);
    };
    if message.trim().is_empty() {
        return Err(StageFailureCode::OutputRejected);
    }
    let summary = BoundedText::new(message).map_err(|_| StageFailureCode::OutputRejected)?;
    let handoff = StructuredHandoff::new(
        correlation.workflow_id.clone(),
        correlation.task_id.clone(),
        assignment.agent_id.clone(),
        next_role(assignment.role),
        summary,
        bounded("stage result"),
        bounded("stage scope"),
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
    );
    Ok(StageOutput {
        correlation,
        result: codex_orchestration::StageResult::Succeeded { handoff },
    })
}

fn stage_failure(
    workflow: &mut SequentialWorkflow,
    correlation: StageCorrelation,
    code: StageFailureCode,
) -> StageOutput {
    let output = StageOutput {
        correlation,
        result: codex_orchestration::StageResult::Failed { code },
    };
    let _ = workflow.complete_stage(&output);
    output
}

fn bounded(value: &str) -> BoundedText {
    BoundedText::new(value).expect("static stage handoff text is bounded")
}

fn next_role(role: AgentRole) -> AgentRole {
    match role {
        AgentRole::Planner => AgentRole::Executor,
        AgentRole::Executor => AgentRole::Verifier,
        AgentRole::Verifier => AgentRole::Verifier,
        AgentRole::Explorer => AgentRole::Explorer,
    }
}
