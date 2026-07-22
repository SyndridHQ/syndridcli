use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::AgentRole;
use crate::PermissionEnvelope;
use crate::StructuredHandoff;
use crate::TaskId;
use crate::WorkAccess;
use crate::WorkflowId;

const MAX_STAGE_ID_BYTES: usize = 128;

/// The bounded stages supported by the sequential coordinator.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequentialStage {
    Planner,
    Executor,
    Verifier,
    RepairExecutor,
    FinalVerifier,
}

/// Opaque identifier for one stage invocation within a workflow.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct StageId(String);

/// Failure returned when a stage identifier is not bounded and non-empty.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum StageIdError {
    #[error("stage identifier must not be empty")]
    Empty,
    #[error("stage identifier exceeds the {MAX_STAGE_ID_BYTES}-byte limit")]
    TooLong,
}

impl StageId {
    pub fn new(value: impl Into<String>) -> Result<Self, StageIdError> {
        let value = value.into();
        if value.is_empty() {
            return Err(StageIdError::Empty);
        }
        if value.len() > MAX_STAGE_ID_BYTES {
            return Err(StageIdError::TooLong);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for StageId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Correlation identity shared by one stage input and output.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct StageCorrelation {
    pub workflow_id: WorkflowId,
    pub task_id: TaskId,
    pub stage_id: StageId,
}

/// Input accepted by exactly one sequential stage execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StageInput {
    pub correlation: StageCorrelation,
    pub role: AgentRole,
    pub access: WorkAccess,
    pub permissions: PermissionEnvelope,
    pub handoff: StructuredHandoff,
}

/// Bounded domain-level failure codes; runtime error details stay outside this crate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageFailureCode {
    InvalidInput,
    PermissionDenied,
    RuntimeUnavailable,
    OutputRejected,
    StageFailed,
    VerificationFailed,
}

/// Result produced by one stage execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StageResult {
    Succeeded { handoff: StructuredHandoff },
    Rejected { handoff: StructuredHandoff },
    Failed { code: StageFailureCode },
}

/// Output returned by exactly one stage execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StageOutput {
    pub correlation: StageCorrelation,
    pub result: StageResult,
}

/// Minimal injected execution boundary for one stage.
pub trait StageExecutor {
    fn execute(&self, input: StageInput) -> StageOutput;
}

/// Explicit state of one stage in the sequential workflow.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageState {
    Pending,
    Active,
    Succeeded,
    Rejected,
    Skipped,
    Failed,
}

/// Explicit state of the sequential workflow.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequentialWorkflowState {
    Ready,
    Running,
    Succeeded,
    Failed,
}

/// A bounded Planner → Executor → Verifier workflow with one optional repair cycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SequentialWorkflow {
    workflow_id: WorkflowId,
    task_id: TaskId,
    permission_ceiling: PermissionEnvelope,
    stages: [(StageId, SequentialStage, StageState); 5],
    active_stage: Option<SequentialStage>,
    state: SequentialWorkflowState,
}

/// Failure returned when a sequential workflow operation violates its invariants.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SequentialWorkflowError {
    #[error("a sequential workflow stage is already active")]
    StageAlreadyActive,
    #[error("the requested stage is not the next pending stage")]
    StageOutOfOrder,
    #[error("the stage input does not match its required role or access")]
    StagePolicyMismatch,
    #[error("the stage handoff correlation does not match the stage input")]
    HandoffCorrelationMismatch,
    #[error("the stage output correlation does not match the active stage")]
    OutputCorrelationMismatch,
    #[error("the successful stage handoff correlation does not match the active workflow")]
    SuccessfulHandoffCorrelationMismatch,
    #[error("the stage permission envelope exceeds the workflow or role ceiling")]
    PermissionCeilingExceeded,
    #[error("the workflow has already completed")]
    WorkflowCompleted,
}

impl SequentialWorkflow {
    pub fn new(
        workflow_id: WorkflowId,
        task_id: TaskId,
        permission_ceiling: PermissionEnvelope,
    ) -> Result<Self, StageIdError> {
        Ok(Self {
            workflow_id,
            task_id,
            permission_ceiling,
            stages: [
                (
                    StageId::new("planner")?,
                    SequentialStage::Planner,
                    StageState::Pending,
                ),
                (
                    StageId::new("executor")?,
                    SequentialStage::Executor,
                    StageState::Pending,
                ),
                (
                    StageId::new("verifier")?,
                    SequentialStage::Verifier,
                    StageState::Pending,
                ),
                (
                    StageId::new("repair_executor")?,
                    SequentialStage::RepairExecutor,
                    StageState::Pending,
                ),
                (
                    StageId::new("final_verifier")?,
                    SequentialStage::FinalVerifier,
                    StageState::Pending,
                ),
            ],
            active_stage: None,
            state: SequentialWorkflowState::Ready,
        })
    }

    pub fn workflow_id(&self) -> &WorkflowId {
        &self.workflow_id
    }

    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }

    pub fn state(&self) -> SequentialWorkflowState {
        self.state
    }

    pub fn permission_ceiling(&self) -> PermissionEnvelope {
        self.permission_ceiling
    }

    pub fn stage_state(&self, stage: SequentialStage) -> Option<StageState> {
        self.stages
            .iter()
            .find(|(_, candidate, _)| *candidate == stage)
            .map(|(_, _, state)| *state)
    }

    pub fn stage_id(&self, stage: SequentialStage) -> Option<&StageId> {
        self.stages
            .iter()
            .find(|(_, candidate, _)| *candidate == stage)
            .map(|(stage_id, _, _)| stage_id)
    }

    pub fn active_stage(&self) -> Option<SequentialStage> {
        self.active_stage
    }

    pub fn begin_stage(&mut self, input: &StageInput) -> Result<(), SequentialWorkflowError> {
        if matches!(
            self.state,
            SequentialWorkflowState::Succeeded | SequentialWorkflowState::Failed
        ) {
            return Err(SequentialWorkflowError::WorkflowCompleted);
        }
        if self.active_stage.is_some() {
            return Err(SequentialWorkflowError::StageAlreadyActive);
        }
        self.validate_input(input)?;
        let Some(stage) = self.stage_for_id(input.correlation.stage_id.as_str()) else {
            return Err(SequentialWorkflowError::StageOutOfOrder);
        };
        if self.next_stage() != Some(stage) {
            return Err(SequentialWorkflowError::StageOutOfOrder);
        }
        self.set_stage_state(input.correlation.stage_id.as_str(), StageState::Active);
        self.active_stage = Some(stage);
        self.state = SequentialWorkflowState::Running;
        Ok(())
    }

    pub fn complete_stage(&mut self, output: &StageOutput) -> Result<(), SequentialWorkflowError> {
        let Some(active_stage) = self.active_stage else {
            return Err(SequentialWorkflowError::StageOutOfOrder);
        };
        let Some(stage_id) = self.stage_id(active_stage) else {
            return Err(SequentialWorkflowError::OutputCorrelationMismatch);
        };
        if output.correlation.workflow_id != self.workflow_id
            || output.correlation.task_id != self.task_id
            || &output.correlation.stage_id != stage_id
        {
            return Err(SequentialWorkflowError::OutputCorrelationMismatch);
        }
        if let StageResult::Succeeded { handoff } | StageResult::Rejected { handoff } =
            &output.result
            && (handoff.workflow_id() != &output.correlation.workflow_id
                || handoff.task_id() != &output.correlation.task_id)
        {
            return Err(SequentialWorkflowError::SuccessfulHandoffCorrelationMismatch);
        }
        let next_state = match &output.result {
            StageResult::Succeeded { .. } => StageState::Succeeded,
            StageResult::Rejected { .. } => StageState::Rejected,
            StageResult::Failed { .. } => StageState::Failed,
        };
        if matches!(&output.result, StageResult::Rejected { .. })
            && !matches!(
                active_stage,
                SequentialStage::Verifier | SequentialStage::FinalVerifier
            )
        {
            return Err(SequentialWorkflowError::StagePolicyMismatch);
        }
        if matches!(&output.result, StageResult::Succeeded { .. })
            && matches!(active_stage, SequentialStage::Verifier)
        {
            self.set_stage_state("repair_executor", StageState::Skipped);
            self.set_stage_state("final_verifier", StageState::Skipped);
        }
        self.set_stage_state(output.correlation.stage_id.as_str(), next_state);
        self.active_stage = None;
        self.state = if next_state == StageState::Succeeded
            && self.stages.iter().all(|(_, _, state)| {
                matches!(
                    state,
                    StageState::Succeeded | StageState::Rejected | StageState::Skipped
                )
            }) {
            SequentialWorkflowState::Succeeded
        } else if next_state == StageState::Failed {
            SequentialWorkflowState::Failed
        } else if matches!(active_stage, SequentialStage::FinalVerifier)
            && next_state == StageState::Rejected
        {
            SequentialWorkflowState::Failed
        } else {
            SequentialWorkflowState::Ready
        };
        Ok(())
    }

    pub fn execute_next<E: StageExecutor>(
        &mut self,
        input: StageInput,
        executor: &E,
    ) -> Result<StageOutput, SequentialWorkflowError> {
        self.begin_stage(&input)?;
        let output = executor.execute(input);
        self.complete_stage(&output)?;
        Ok(output)
    }

    fn validate_input(&self, input: &StageInput) -> Result<(), SequentialWorkflowError> {
        if input.correlation.workflow_id != self.workflow_id
            || input.correlation.task_id != self.task_id
            || input.handoff.workflow_id() != &self.workflow_id
            || input.handoff.task_id() != &self.task_id
        {
            return Err(SequentialWorkflowError::HandoffCorrelationMismatch);
        }
        let expected = match input.correlation.stage_id.as_str() {
            "planner" => (
                SequentialStage::Planner,
                AgentRole::Planner,
                WorkAccess::ReadOnly,
            ),
            "executor" => (
                SequentialStage::Executor,
                AgentRole::Executor,
                WorkAccess::Writer,
            ),
            "verifier" => (
                SequentialStage::Verifier,
                AgentRole::Verifier,
                WorkAccess::ReadOnly,
            ),
            "repair_executor" => (
                SequentialStage::RepairExecutor,
                AgentRole::Executor,
                WorkAccess::Writer,
            ),
            "final_verifier" => (
                SequentialStage::FinalVerifier,
                AgentRole::Verifier,
                WorkAccess::ReadOnly,
            ),
            _ => return Err(SequentialWorkflowError::StageOutOfOrder),
        };
        if input.role != expected.1 || input.access != expected.2 {
            return Err(SequentialWorkflowError::StagePolicyMismatch);
        }
        if !permission_envelope_allows(self.permission_ceiling, input.permissions)
            || !expected.2.allows(input.permissions.assignment_access())
        {
            return Err(SequentialWorkflowError::PermissionCeilingExceeded);
        }
        Ok(())
    }

    fn next_stage(&self) -> Option<SequentialStage> {
        self.stages
            .iter()
            .find(|(_, _, state)| *state == StageState::Pending)
            .map(|(_, stage, _)| *stage)
    }

    fn stage_for_id(&self, id: &str) -> Option<SequentialStage> {
        self.stages
            .iter()
            .find(|(stage_id, _, _)| stage_id.as_str() == id)
            .map(|(_, stage, _)| *stage)
    }

    fn set_stage_state(&mut self, id: &str, state: StageState) {
        if let Some((_, _, current)) = self
            .stages
            .iter_mut()
            .find(|(stage_id, _, _)| stage_id.as_str() == id)
        {
            *current = state;
        }
    }
}

fn permission_envelope_allows(ceiling: PermissionEnvelope, requested: PermissionEnvelope) -> bool {
    ceiling
        .workflow_ceiling()
        .allows(requested.workflow_ceiling())
        && ceiling.parent_ceiling().allows(requested.parent_ceiling())
        && ceiling
            .assignment_access()
            .allows(requested.assignment_access())
}
