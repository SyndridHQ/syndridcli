use codex_orchestration::AgentId;
use codex_orchestration::BoundedText;
use codex_orchestration::CancellationState;
use codex_orchestration::DataQuality;
use codex_orchestration::EffortRoute;
use codex_orchestration::ModelRoute;
use codex_orchestration::RunLifecycleState;
use codex_orchestration::TaskId;
use codex_orchestration::UsageQuantity;
use codex_orchestration::VerificationState;
use codex_orchestration::WaitReason;
use codex_orchestration::WorkflowId;
use serde::Deserialize;
use serde::Serialize;

use crate::RuntimeAgentId;

/// Request record for a bounded child-status observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObserveChildRequest {
    workflow_id: WorkflowId,
    task_id: TaskId,
    agent_id: AgentId,
    runtime_id: Option<RuntimeAgentId>,
}

impl ObserveChildRequest {
    pub fn new(
        workflow_id: WorkflowId,
        task_id: TaskId,
        agent_id: AgentId,
        runtime_id: Option<RuntimeAgentId>,
    ) -> Self {
        Self {
            workflow_id,
            task_id,
            agent_id,
            runtime_id,
        }
    }

    pub fn workflow_id(&self) -> &WorkflowId {
        &self.workflow_id
    }
    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }
    pub fn runtime_id(&self) -> Option<&RuntimeAgentId> {
        self.runtime_id.as_ref()
    }
}

/// Bounded observation supplied by a future runtime adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChildObservation {
    workflow_id: WorkflowId,
    task_id: TaskId,
    agent_id: AgentId,
    runtime_id: RuntimeAgentId,
    lifecycle: RunLifecycleState,
    wait_reason: Option<WaitReason>,
    cancellation: CancellationState,
    verification: VerificationState,
    model_route: Option<ModelRoute>,
    effort_route: Option<EffortRoute>,
    observed_usage: Option<UsageQuantity>,
    data_quality: DataQuality,
    status_detail: Option<BoundedText>,
    sequence: Option<u64>,
}

impl ChildObservation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workflow_id: WorkflowId,
        task_id: TaskId,
        agent_id: AgentId,
        runtime_id: RuntimeAgentId,
        lifecycle: RunLifecycleState,
        wait_reason: Option<WaitReason>,
        cancellation: CancellationState,
        verification: VerificationState,
        model_route: Option<ModelRoute>,
        effort_route: Option<EffortRoute>,
        observed_usage: Option<UsageQuantity>,
        data_quality: DataQuality,
        status_detail: Option<BoundedText>,
        sequence: Option<u64>,
    ) -> Self {
        Self {
            workflow_id,
            task_id,
            agent_id,
            runtime_id,
            lifecycle,
            wait_reason,
            cancellation,
            verification,
            model_route,
            effort_route,
            observed_usage,
            data_quality,
            status_detail,
            sequence,
        }
    }

    pub fn workflow_id(&self) -> &WorkflowId {
        &self.workflow_id
    }
    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }
    pub fn runtime_id(&self) -> &RuntimeAgentId {
        &self.runtime_id
    }
    pub fn lifecycle(&self) -> RunLifecycleState {
        self.lifecycle
    }
    pub fn wait_reason(&self) -> Option<WaitReason> {
        self.wait_reason
    }
    pub fn cancellation(&self) -> CancellationState {
        self.cancellation
    }
    pub fn verification(&self) -> VerificationState {
        self.verification
    }
    pub fn model_route(&self) -> Option<&ModelRoute> {
        self.model_route.as_ref()
    }
    pub fn effort_route(&self) -> Option<&EffortRoute> {
        self.effort_route.as_ref()
    }
    pub fn observed_usage(&self) -> Option<&UsageQuantity> {
        self.observed_usage.as_ref()
    }
    pub fn data_quality(&self) -> DataQuality {
        self.data_quality
    }
    pub fn status_detail(&self) -> Option<&BoundedText> {
        self.status_detail.as_ref()
    }
    pub fn sequence(&self) -> Option<u64> {
        self.sequence
    }
}
