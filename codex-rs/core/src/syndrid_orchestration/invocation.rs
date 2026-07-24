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
use codex_orchestration_adapter::AdapterError;
use codex_orchestration_adapter::AdapterErrorKind;
use codex_orchestration_adapter::Retryability;
use codex_protocol::ThreadId;
use codex_protocol::protocol::SessionSource;
use serde::Serialize;
use std::fmt;
use tokio_util::sync::CancellationToken;

pub(super) const MAX_ORCHESTRATED_TASK_BYTES: usize = 16 * 1024;
const MAX_ROUTE_IDENTIFIER_BYTES: usize = 128;

/// A bounded provider-neutral text invocation passed from an orchestration stage to a provider.
#[derive(Clone, Eq, PartialEq)]
pub struct ProviderInvocationRequest {
    pub provider: String,
    pub model: String,
    pub system: Option<String>,
    pub user: String,
    pub max_output_tokens: u32,
}

impl fmt::Debug for ProviderInvocationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderInvocationRequest")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("has_system", &self.system.is_some())
            .field("system_bytes", &self.system.as_ref().map_or(0, String::len))
            .field("user_input_bytes", &self.user.len())
            .field("max_output_tokens", &self.max_output_tokens)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderInvocationUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ProviderInvocationResult {
    pub provider: String,
    pub model: String,
    pub text: String,
    pub finish_reason: Option<String>,
    pub usage: Option<ProviderInvocationUsage>,
    pub request_id: Option<String>,
}

impl fmt::Debug for ProviderInvocationResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderInvocationResult")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("generated_output_bytes", &self.text.len())
            .field("finish_reason", &self.finish_reason)
            .field("usage", &self.usage)
            .field("request_id", &self.request_id)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderInvocationError {
    InvalidConfiguration,
    UnsupportedProvider,
    UnsupportedAuthenticationMethod,
    ConnectionDisabled,
    ConnectionUnvalidated,
    ReauthenticationRequired,
    MissingCredentialReference,
    CredentialNotFound,
    CredentialStoreUnavailable,
    CredentialStoreRejected,
    InvalidModelId,
    InvalidRequest,
    InputTooLarge,
    OutputLimitInvalid,
    TransportUnavailable,
    RequestTimedOut,
    Cancelled,
    Unauthorized,
    PaymentRequired,
    Forbidden,
    RateLimited,
    ProviderUnavailable,
    ProviderRejected,
    InvalidContentType,
    ResponseTooLarge,
    InvalidResponse,
    MissingOutput,
    OrchestrationConversionFailed,
    LiveCodexInvocationUnavailable,
    ScopedSessionConstructionFailed,
    StreamTerminated,
}

impl fmt::Display for ProviderInvocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidConfiguration => "provider invocation configuration is invalid",
            Self::UnsupportedProvider => "provider invocation is unsupported",
            Self::UnsupportedAuthenticationMethod => {
                "provider authentication method is unsupported"
            }
            Self::ConnectionDisabled => "provider connection is disabled",
            Self::ConnectionUnvalidated => "provider connection is not validated",
            Self::ReauthenticationRequired => "Codex account requires reauthentication",
            Self::MissingCredentialReference => "provider credential reference is missing",
            Self::CredentialNotFound => "provider credential was not found",
            Self::CredentialStoreUnavailable => "provider credential store is unavailable",
            Self::CredentialStoreRejected => "provider credential store rejected the credential",
            Self::InvalidModelId => "provider model ID is invalid",
            Self::InvalidRequest => "provider invocation request is invalid",
            Self::InputTooLarge => "provider invocation input is too large",
            Self::OutputLimitInvalid => "provider invocation output limit is invalid",
            Self::TransportUnavailable => "provider transport is unavailable",
            Self::RequestTimedOut => "provider invocation timed out",
            Self::Cancelled => "provider invocation was cancelled",
            Self::Unauthorized => "provider authorization was rejected",
            Self::PaymentRequired => "provider payment is required",
            Self::Forbidden => "provider request was forbidden",
            Self::RateLimited => "provider rate limit was reached",
            Self::ProviderUnavailable => "provider is unavailable",
            Self::ProviderRejected => "provider rejected the request",
            Self::InvalidContentType => "provider response content type is invalid",
            Self::ResponseTooLarge => "provider response is too large",
            Self::InvalidResponse => "provider response is invalid",
            Self::MissingOutput => "provider response did not contain output",
            Self::OrchestrationConversionFailed => {
                "provider result could not be converted for orchestration"
            }
            Self::LiveCodexInvocationUnavailable => {
                "selected Codex invocation is unavailable in this runtime"
            }
            Self::ScopedSessionConstructionFailed => "scoped Codex session could not be created",
            Self::StreamTerminated => "provider response stream terminated unexpectedly",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProviderInvocationError {}

/// Provider adapters implement this seam so orchestration never depends on provider-specific HTTP types.
pub(crate) trait ProviderInvocation: Send + Sync {
    fn invoke(
        &self,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> impl std::future::Future<Output = Result<ProviderInvocationResult, ProviderInvocationError>>
    + Send;
}

/// Runs a provider-neutral invocation through the orchestration adapter boundary.
pub(super) async fn invoke_provider<P: ProviderInvocation>(
    provider: &P,
    request: ProviderInvocationRequest,
    cancellation: CancellationToken,
) -> Result<ProviderInvocationResult, AdapterError> {
    provider
        .invoke(request, cancellation)
        .await
        .map_err(|error| {
            let kind = match error {
                ProviderInvocationError::InvalidRequest
                | ProviderInvocationError::InvalidModelId
                | ProviderInvocationError::InputTooLarge
                | ProviderInvocationError::OutputLimitInvalid => AdapterErrorKind::InvalidRequest,
                ProviderInvocationError::UnsupportedProvider
                | ProviderInvocationError::UnsupportedAuthenticationMethod => {
                    AdapterErrorKind::Unsupported
                }
                ProviderInvocationError::ConnectionDisabled
                | ProviderInvocationError::ConnectionUnvalidated
                | ProviderInvocationError::ReauthenticationRequired
                | ProviderInvocationError::Unauthorized
                | ProviderInvocationError::Forbidden => AdapterErrorKind::PermissionDenied,
                ProviderInvocationError::RequestTimedOut
                | ProviderInvocationError::Cancelled
                | ProviderInvocationError::TransportUnavailable
                | ProviderInvocationError::ProviderUnavailable
                | ProviderInvocationError::CredentialStoreUnavailable => {
                    AdapterErrorKind::RuntimeUnavailable
                }
                ProviderInvocationError::ScopedSessionConstructionFailed
                | ProviderInvocationError::StreamTerminated => AdapterErrorKind::RuntimeUnavailable,
                ProviderInvocationError::PaymentRequired
                | ProviderInvocationError::RateLimited
                | ProviderInvocationError::ProviderRejected
                | ProviderInvocationError::InvalidConfiguration
                | ProviderInvocationError::MissingCredentialReference
                | ProviderInvocationError::CredentialNotFound
                | ProviderInvocationError::CredentialStoreRejected
                | ProviderInvocationError::InvalidContentType
                | ProviderInvocationError::ResponseTooLarge
                | ProviderInvocationError::InvalidResponse
                | ProviderInvocationError::MissingOutput
                | ProviderInvocationError::OrchestrationConversionFailed => {
                    AdapterErrorKind::InternalAdapterFailure
                }
                ProviderInvocationError::LiveCodexInvocationUnavailable => {
                    AdapterErrorKind::Unsupported
                }
            };
            let retryability = match error {
                ProviderInvocationError::RateLimited
                | ProviderInvocationError::ProviderUnavailable
                | ProviderInvocationError::TransportUnavailable => Retryability::Retryable,
                _ => Retryability::NotRetryable,
            };
            AdapterError::new(
                kind,
                BoundedText::new(error.to_string()).expect("static invocation error is bounded"),
                retryability,
                DataQuality::Exact,
            )
        })
}

pub(super) async fn run_provider_sequential_workflow<P: ProviderInvocation>(
    provider: &P,
    mut workflow: SequentialWorkflow,
    initial_input: StageInput,
    assignments: [super::live::StageAssignment; 5],
    cancellation: CancellationToken,
) -> Result<SequentialWorkflow, AdapterError> {
    let mut handoff = initial_input.handoff.clone();
    let mut input = initial_input;
    for (index, assignment) in assignments.into_iter().enumerate() {
        if index > 0 {
            input = assignment.input(&workflow, &handoff, index);
        }
        let correlation = input.correlation.clone();
        workflow.begin_stage(&input).map_err(|_| {
            provider_workflow_error(AdapterErrorKind::InternalAdapterFailure, &correlation)
        })?;
        let model = assignment.model_route.resolved.clone().ok_or_else(|| {
            provider_workflow_error(AdapterErrorKind::InvalidRequest, &correlation)
        })?;
        let request = ProviderInvocationRequest {
            provider: assignment.provider.clone(),
            model,
            system: None,
            user: format!(
                "stage: {}; task: {}",
                input.correlation.stage_id.as_str(),
                input.handoff.task_summary()
            ),
            max_output_tokens: 1_024,
        };
        let result = invoke_provider(provider, request, cancellation.clone()).await?;
        let output = super::live::bounded_stage_output(
            super::TerminalSnapshot {
                runtime_id: codex_orchestration_adapter::RuntimeAgentId::new(format!(
                    "provider-{}",
                    index
                ))
                .expect("static provider runtime ID is valid"),
                status: codex_protocol::protocol::AgentStatus::Completed(Some(result.text)),
            },
            correlation,
            &assignment,
            None,
        )
        .map_err(|_| {
            provider_workflow_error(AdapterErrorKind::InternalAdapterFailure, &input.correlation)
        })?;
        handoff = match &output.result {
            codex_orchestration::StageResult::Succeeded { handoff }
            | codex_orchestration::StageResult::Rejected { handoff } => handoff.clone(),
            codex_orchestration::StageResult::Failed { .. } => {
                return Err(provider_workflow_error(
                    AdapterErrorKind::InternalAdapterFailure,
                    &input.correlation,
                ));
            }
        };
        workflow.complete_stage(&output).map_err(|_| {
            provider_workflow_error(AdapterErrorKind::InternalAdapterFailure, &input.correlation)
        })?;
        if workflow.state() == SequentialWorkflowState::Succeeded {
            break;
        }
    }
    Ok(workflow)
}

fn provider_workflow_error(kind: AdapterErrorKind, correlation: &StageCorrelation) -> AdapterError {
    AdapterError::new(
        kind,
        BoundedText::new("provider-backed workflow failed")
            .expect("static workflow error is bounded"),
        Retryability::NotRetryable,
        DataQuality::Exact,
    )
    .with_attribution(
        Some(correlation.workflow_id.clone()),
        Some(correlation.task_id.clone()),
        None,
    )
}

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

pub(super) async fn run_orchestrated_task_with_provider<P: ProviderInvocation>(
    request: RunOrchestratedTaskRequest,
    provider: &P,
    cancellation: CancellationToken,
) -> Result<RunOrchestratedTaskResult, AdapterError> {
    let route_metadata = route_metadata(&request);
    let assignments = request.assignments().map_err(|_| {
        provider_workflow_error_for_request(&request, AdapterErrorKind::InvalidRequest)
    })?;
    let workflow = SequentialWorkflow::new(
        request.workflow_id.clone(),
        request.task_id.clone(),
        request.permission_ceiling,
    )
    .map_err(|_| provider_workflow_error_for_request(&request, AdapterErrorKind::InvalidRequest))?;
    let initial_input = initial_input(&request, &assignments.values[0]);
    let adapter = CodexOrchestrationAdapter::new(
        request.parent.agent_control.clone(),
        request.base_config.clone(),
        request.parent.parent_thread_id,
        request.parent.parent_session_source.clone(),
    );
    let workflow = adapter
        .run_provider_sequential_workflow(
            provider,
            workflow,
            initial_input,
            assignments.values,
            cancellation,
        )
        .await?;
    Ok(map_workflow(&request, workflow, route_metadata))
}

fn provider_workflow_error_for_request(
    request: &RunOrchestratedTaskRequest,
    kind: AdapterErrorKind,
) -> AdapterError {
    AdapterError::new(
        kind,
        BoundedText::new("provider-backed workflow request is invalid")
            .expect("static workflow error is bounded"),
        Retryability::NotRetryable,
        DataQuality::Exact,
    )
    .with_attribution(
        Some(request.workflow_id.clone()),
        Some(request.task_id.clone()),
        None,
    )
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
