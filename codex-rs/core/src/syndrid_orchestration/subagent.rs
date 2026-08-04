use super::execution_budget::BudgetExhaustion;
use super::execution_budget::ExecutionBudgetLedger;
use super::invocation::ProviderInvocationError;
use super::invocation::ProviderInvocationRequest;
use super::invocation::ProviderInvocationResult;
use super::invocation::ProviderInvocationToolDefinition;
use super::invocation::ProviderInvocationToolResult;
use super::orchestration_cleanup::CleanupChildKind;
use super::orchestration_cleanup::OrchestrationCleanup;
use super::routing_profiles::RoutingConnectionDirectory;
use super::routing_profiles::RoutingProfileError;
use super::routing_profiles::RoutingProfileRegistry;
use super::routing_profiles::RoutingResolutionStatus;
use super::routing_profiles::RoutingRole;
use super::subagent_tools::SubagentToolCallRecord;
use super::subagent_tools::SubagentToolError;
use super::subagent_tools::SubagentToolKind;
use super::subagent_tools::SubagentToolPolicy;
use super::subagent_tools::execute_tool;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

pub const SUBAGENT_DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
pub const SUBAGENT_MIN_TIMEOUT: Duration = Duration::from_secs(1);
pub const SUBAGENT_MAX_TIMEOUT: Duration = Duration::from_secs(15 * 60);
pub const SUBAGENT_DEFAULT_OUTPUT_TOKENS: u32 = 2_000;
pub const SUBAGENT_MIN_OUTPUT_TOKENS: u32 = 1;
pub const SUBAGENT_MAX_OUTPUT_TOKENS: u32 = 16_000;
pub const SUBAGENT_MAX_TASK_ID_BYTES: usize = 128;
pub const SUBAGENT_MAX_INSTRUCTION_BYTES: usize = 32 * 1024;
pub const SUBAGENT_MAX_CONTEXT_BYTES: usize = 128 * 1024;

/// A provider-neutral dispatcher used by the bounded runtime.
pub trait SubagentProvider: Send + Sync {
    fn invoke(
        &self,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> impl Future<Output = Result<ProviderInvocationResult, ProviderInvocationError>> + Send;

    fn invoke_role(
        &self,
        _role: RoutingRole,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> impl Future<Output = Result<ProviderInvocationResult, ProviderInvocationError>> + Send
    {
        self.invoke(request, cancellation)
    }
}

impl<P: super::invocation::ProviderInvocation> SubagentProvider for P {
    fn invoke(
        &self,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> impl Future<Output = Result<ProviderInvocationResult, ProviderInvocationError>> + Send
    {
        super::invocation::ProviderInvocation::invoke(self, request, cancellation)
    }
}

#[derive(Clone)]
pub struct SubagentRequest {
    pub task_id: String,
    pub parent_id: Option<String>,
    pub role: RoutingRole,
    pub instruction: String,
    pub context: Option<String>,
    pub timeout: Duration,
    pub max_output_tokens: u32,
    pub cancellation: CancellationToken,
    pub depth: u8,
    pub tool_policy: SubagentToolPolicy,
    pub budget: Option<Arc<ExecutionBudgetLedger>>,
    pub(crate) cleanup: Option<Arc<OrchestrationCleanup>>,
}

impl fmt::Debug for SubagentRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubagentRequest")
            .field("task_id_bytes", &self.task_id.len())
            .field("has_parent_id", &self.parent_id.is_some())
            .field("role", &self.role)
            .field("instruction_bytes", &self.instruction.len())
            .field(
                "context_bytes",
                &self.context.as_ref().map_or(0, String::len),
            )
            .field("timeout", &self.timeout)
            .field("max_output_tokens", &self.max_output_tokens)
            .field("depth", &self.depth)
            .field("tool_policy", &self.tool_policy)
            .field("has_budget", &self.budget.is_some())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    Completed,
    CompletedWithTruncation,
    Cancelled,
    TimedOut,
    RoutingFailed,
    AuthenticationFailed,
    ProviderRejected,
    TransportFailed,
    InvalidResponse,
    OutputLimitReached,
    InvalidToolPolicy,
    ToolPolicyRejected,
    ToolExecutionFailed,
    BudgetExhausted,
    InternalFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentLifecycle {
    Created,
    Validating,
    Routing,
    Running,
    InvokingProvider,
    AwaitingToolExecution,
    ExecutingTool,
    ReturningToolResult,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    BudgetExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentDataQuality {
    Exact,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SubagentUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub quality: SubagentDataQuality,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct SubagentOutcome {
    pub task_id: String,
    pub parent_id: Option<String>,
    pub role: RoutingRole,
    pub profile_id: String,
    pub provider_id: String,
    pub connection_id: String,
    pub model_id: String,
    pub status: SubagentStatus,
    pub output: Option<String>,
    pub usage: Option<SubagentUsage>,
    pub latency_ms: u128,
    pub lifecycle: Vec<SubagentLifecycle>,
    pub warnings: Vec<String>,
    pub provider_turns: usize,
    pub tool_calls: usize,
    pub tool_call_counts: BTreeMap<SubagentToolKind, usize>,
    pub tool_audit: Vec<SubagentToolCallRecord>,
    pub output_truncated: bool,
    pub budget_exhausted: bool,
}

impl fmt::Debug for SubagentOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubagentOutcome")
            .field("task_id_bytes", &self.task_id.len())
            .field("has_parent_id", &self.parent_id.is_some())
            .field("role", &self.role)
            .field("profile_id_bytes", &self.profile_id.len())
            .field("provider_id_bytes", &self.provider_id.len())
            .field("connection_id_bytes", &self.connection_id.len())
            .field("model_id_bytes", &self.model_id.len())
            .field("status", &self.status)
            .field("output_bytes", &self.output.as_ref().map_or(0, String::len))
            .field("usage", &self.usage)
            .field("latency_ms", &self.latency_ms)
            .field("lifecycle", &self.lifecycle)
            .field("warning_count", &self.warnings.len())
            .field("provider_turns", &self.provider_turns)
            .field("tool_calls", &self.tool_calls)
            .field("tool_call_counts", &self.tool_call_counts)
            .field("tool_audit_count", &self.tool_audit.len())
            .field("output_truncated", &self.output_truncated)
            .field("budget_exhausted", &self.budget_exhausted)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubagentError {
    InvalidTaskId,
    EmptyInstruction,
    InstructionTooLarge,
    ContextTooLarge,
    InvalidTimeout,
    InvalidOutputLimit,
    RecursionNotAllowed,
    CancelledBeforeStart,
    NoActiveProfile,
    UnknownActiveProfile,
    MissingRoleAssignment,
    UnknownProvider,
    UnknownConnection,
    ConnectionDisabled,
    ConnectionUnconfigured,
    ReauthenticationRequired,
    InvalidModel,
    ModelUnverified,
    UnsupportedProvider,
    InvocationCancelled,
    InvocationTimedOut,
    AuthenticationFailed,
    ProviderRejected,
    TransportFailed,
    InvalidResponse,
    OutputLimitReached,
    InvalidToolPolicy,
    JoinFailure,
    InternalFailure,
}

impl fmt::Display for SubagentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidTaskId => "subagent task ID is invalid",
            Self::EmptyInstruction => "subagent instruction is empty",
            Self::InstructionTooLarge => "subagent instruction is too large",
            Self::ContextTooLarge => "subagent context is too large",
            Self::InvalidTimeout => "subagent timeout is invalid",
            Self::InvalidOutputLimit => "subagent output limit is invalid",
            Self::RecursionNotAllowed => "subagent recursion is not allowed",
            Self::CancelledBeforeStart => "subagent was cancelled before start",
            Self::NoActiveProfile => "no active routing profile is configured",
            Self::UnknownActiveProfile => "active routing profile was not found",
            Self::MissingRoleAssignment => "requested subagent role is not assigned",
            Self::UnknownProvider => "subagent provider is unknown",
            Self::UnknownConnection => "subagent connection is unknown",
            Self::ConnectionDisabled => "subagent connection is disabled",
            Self::ConnectionUnconfigured => "subagent connection is unconfigured",
            Self::ReauthenticationRequired => "subagent connection requires reauthentication",
            Self::InvalidModel => "subagent model is invalid",
            Self::ModelUnverified => "subagent model is unverified",
            Self::UnsupportedProvider => "subagent provider is unsupported",
            Self::InvocationCancelled => "subagent invocation was cancelled",
            Self::InvocationTimedOut => "subagent invocation timed out",
            Self::AuthenticationFailed => "subagent authentication failed",
            Self::ProviderRejected => "subagent provider rejected the request",
            Self::TransportFailed => "subagent transport failed",
            Self::InvalidResponse => "subagent response is invalid",
            Self::OutputLimitReached => "subagent output limit was reached",
            Self::InvalidToolPolicy => "subagent tool policy is invalid",
            Self::JoinFailure => "subagent child task join failed",
            Self::InternalFailure => "subagent runtime failed internally",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for SubagentError {}

pub struct SubagentRuntime<P> {
    provider: P,
    profiles: RoutingProfileRegistry,
    directory: RoutingConnectionDirectory,
}

impl<P> SubagentRuntime<P> {
    pub fn new(
        provider: P,
        profiles: RoutingProfileRegistry,
        directory: RoutingConnectionDirectory,
    ) -> Self {
        Self {
            provider,
            profiles,
            directory,
        }
    }
}

impl<P: SubagentProvider> SubagentRuntime<P> {
    pub(crate) fn resolved_route(
        &self,
        request: &SubagentRequest,
    ) -> Result<(String, String, String, String, RoutingRole), SubagentError> {
        validate_request(request)?;
        let profile = self.profiles.active().map_err(|error| match error {
            RoutingProfileError::MissingActiveProfile => SubagentError::NoActiveProfile,
            RoutingProfileError::UnknownProfile => SubagentError::UnknownActiveProfile,
            _ => SubagentError::UnknownActiveProfile,
        })?;
        let assignment = profile
            .assignments
            .get(&request.role)
            .ok_or(SubagentError::MissingRoleAssignment)?;
        if assignment.pool_id.is_none() {
            let resolution = self
                .directory
                .validate_assignment(assignment)
                .map_err(map_routing_error)?;
            if resolution == RoutingResolutionStatus::ModelUnverified {
                return Err(SubagentError::ModelUnverified);
            }
        }
        if !matches!(
            assignment.provider_id.as_str(),
            "codex" | "openrouter" | "omniroute"
        ) {
            return Err(SubagentError::UnsupportedProvider);
        }
        Ok((
            profile.id.as_str().to_string(),
            assignment.provider_id.clone(),
            assignment.connection_id.clone(),
            assignment.model_id.clone(),
            request.role,
        ))
    }

    pub(crate) fn validate_for_batch(
        &self,
        request: &SubagentRequest,
    ) -> Result<String, SubagentError> {
        Ok(self.resolved_route(request)?.0)
    }

    pub async fn run_subagent(
        &self,
        request: SubagentRequest,
    ) -> Result<SubagentOutcome, SubagentError> {
        validate_request(&request)?;
        let mut lifecycle = vec![SubagentLifecycle::Created, SubagentLifecycle::Validating];
        if request.cancellation.is_cancelled() {
            return Err(SubagentError::CancelledBeforeStart);
        }
        let profile = self.profiles.active().map_err(|error| match error {
            RoutingProfileError::MissingActiveProfile => SubagentError::NoActiveProfile,
            RoutingProfileError::UnknownProfile => SubagentError::UnknownActiveProfile,
            _ => SubagentError::UnknownActiveProfile,
        })?;
        let assignment = profile
            .assignments
            .get(&request.role)
            .ok_or(SubagentError::MissingRoleAssignment)?;
        if assignment.pool_id.is_none() {
            let resolution = self
                .directory
                .validate_assignment(assignment)
                .map_err(map_routing_error)?;
            if resolution == RoutingResolutionStatus::ModelUnverified {
                return Err(SubagentError::ModelUnverified);
            }
        }
        if !matches!(
            assignment.provider_id.as_str(),
            "codex" | "openrouter" | "omniroute"
        ) {
            return Err(SubagentError::UnsupportedProvider);
        }
        if request.cancellation.is_cancelled() {
            return Err(SubagentError::CancelledBeforeStart);
        }
        lifecycle.push(SubagentLifecycle::Routing);
        let profile_id = profile.id.as_str().to_string();
        let assignment = assignment.clone();
        let started = Instant::now();
        let session_timeout = request
            .timeout
            .min(request.tool_policy.budget().session_timeout);
        let result = tokio::time::timeout(
            session_timeout,
            self.run_tool_session(&request, &profile_id, &assignment, lifecycle),
        )
        .await;
        match result {
            Ok(result) => result,
            Err(_) => {
                let mut lifecycle = vec![
                    SubagentLifecycle::Created,
                    SubagentLifecycle::Validating,
                    SubagentLifecycle::Routing,
                    SubagentLifecycle::TimedOut,
                ];
                if request.cancellation.is_cancelled() {
                    lifecycle.pop();
                    lifecycle.push(SubagentLifecycle::Cancelled);
                }
                Ok(outcome(
                    &request,
                    &profile_id,
                    &assignment,
                    if request.cancellation.is_cancelled() {
                        SubagentStatus::Cancelled
                    } else {
                        SubagentStatus::TimedOut
                    },
                    None,
                    None,
                    started.elapsed().as_millis(),
                    lifecycle,
                    vec!["subagent session reached its time limit".to_string()],
                    &SessionMetrics::default(),
                    false,
                    false,
                ))
            }
        }
    }

    async fn run_tool_session(
        &self,
        request: &SubagentRequest,
        profile_id: &str,
        assignment: &super::routing_profiles::RoutingAssignment,
        mut lifecycle: Vec<SubagentLifecycle>,
    ) -> Result<SubagentOutcome, SubagentError> {
        let prompt = build_prompt(
            request.role,
            &request.instruction,
            request.context.as_deref(),
        );
        let tools = request
            .tool_policy
            .approved_tools()
            .iter()
            .map(|tool| ProviderInvocationToolDefinition {
                name: tool.provider_name().to_string(),
            })
            .collect::<Vec<_>>();
        let mut tool_results = Vec::new();
        let mut seen_call_ids = HashSet::new();
        let mut metrics = SessionMetrics::default();
        let mut usage = None;
        let started = Instant::now();

        loop {
            if request.cancellation.is_cancelled() {
                lifecycle.push(SubagentLifecycle::Cancelled);
                return Ok(outcome(
                    request,
                    profile_id,
                    assignment,
                    SubagentStatus::Cancelled,
                    None,
                    usage,
                    started.elapsed().as_millis(),
                    lifecycle,
                    vec!["subagent session was cancelled".to_string()],
                    &metrics,
                    false,
                    false,
                ));
            }
            if metrics.provider_turns >= request.tool_policy.budget().max_provider_turns {
                lifecycle.push(SubagentLifecycle::BudgetExhausted);
                return Ok(outcome(
                    request,
                    profile_id,
                    assignment,
                    SubagentStatus::BudgetExhausted,
                    None,
                    usage,
                    started.elapsed().as_millis(),
                    lifecycle,
                    vec!["maximum provider turns reached".to_string()],
                    &metrics,
                    false,
                    true,
                ));
            }
            lifecycle.push(SubagentLifecycle::InvokingProvider);
            let provider_ownership = request
                .cleanup
                .as_ref()
                .map(|cleanup| {
                    cleanup.register_provider_reservation(
                        request
                            .budget
                            .as_ref()
                            .map_or(0, |budget| budget.generation()),
                    )
                })
                .transpose()
                .map_err(|_| SubagentError::InternalFailure)?;
            let provider_reservation = match request
                .budget
                .as_ref()
                .map(|budget| budget.reserve_provider(request.role))
                .transpose()
            {
                Ok(reservation) => reservation,
                Err(error) => {
                    if let (Some(cleanup), Some(handle)) =
                        (request.cleanup.as_ref(), provider_ownership)
                    {
                        cleanup
                            .resolve_provider_reservation(
                                request
                                    .budget
                                    .as_ref()
                                    .map_or(0, |budget| budget.generation()),
                                handle,
                            )
                            .map_err(|_| SubagentError::InternalFailure)?;
                    }
                    lifecycle.push(SubagentLifecycle::BudgetExhausted);
                    return Ok(outcome(
                        request,
                        profile_id,
                        assignment,
                        SubagentStatus::BudgetExhausted,
                        None,
                        usage,
                        started.elapsed().as_millis(),
                        lifecycle,
                        vec![safe_budget_message(error)],
                        &metrics,
                        false,
                        true,
                    ));
                }
            };
            let provider_child = match request
                .cleanup
                .as_ref()
                .map(|cleanup| {
                    cleanup.register_child(
                        request
                            .budget
                            .as_ref()
                            .map_or(0, |budget| budget.generation()),
                        CleanupChildKind::Provider,
                    )
                })
                .transpose()
            {
                Ok(child) => child,
                Err(_) => {
                    if let (Some(cleanup), Some(handle)) =
                        (request.cleanup.as_ref(), provider_ownership)
                    {
                        cleanup
                            .resolve_provider_reservation(
                                request
                                    .budget
                                    .as_ref()
                                    .map_or(0, |budget| budget.generation()),
                                handle,
                            )
                            .map_err(|_| SubagentError::InternalFailure)?;
                    }
                    return Err(SubagentError::InternalFailure);
                }
            };
            if let Some(reservation) = provider_reservation {
                if reservation.commit().is_err() {
                    if let (Some(cleanup), Some(handle)) =
                        (request.cleanup.as_ref(), provider_ownership)
                    {
                        cleanup
                            .resolve_provider_reservation(
                                request
                                    .budget
                                    .as_ref()
                                    .map_or(0, |budget| budget.generation()),
                                handle,
                            )
                            .map_err(|_| SubagentError::InternalFailure)?;
                    }
                    return Err(SubagentError::InternalFailure);
                }
            }
            if let (Some(cleanup), Some(handle)) = (request.cleanup.as_ref(), provider_ownership) {
                cleanup
                    .resolve_provider_reservation(
                        request
                            .budget
                            .as_ref()
                            .map_or(0, |budget| budget.generation()),
                        handle,
                    )
                    .map_err(|_| SubagentError::InternalFailure)?;
            }
            let provider_request = ProviderInvocationRequest {
                provider: assignment.provider_id.clone(),
                model: assignment.model_id.clone(),
                system: Some(prompt.0.clone()),
                user: prompt.1.clone(),
                max_output_tokens: request.max_output_tokens,
                tools: tools.clone(),
                tool_results: std::mem::take(&mut tool_results),
            };
            let provider_future = self.provider.invoke_role(
                request.role,
                provider_request,
                request.cancellation.clone(),
            );
            tokio::pin!(provider_future);
            let result = tokio::select! {
                _ = request.cancellation.cancelled() => {
                    let _ = provider_future.await;
                    Err(SubagentError::InvocationCancelled)
                }
                result = &mut provider_future => result.map_err(map_invocation_error),
            };
            if let (Some(cleanup), Some(handle)) = (request.cleanup.as_ref(), provider_child) {
                cleanup
                    .complete_child(
                        request
                            .budget
                            .as_ref()
                            .map_or(0, |budget| budget.generation()),
                        handle,
                    )
                    .map_err(|_| SubagentError::InternalFailure)?;
            }
            if let Some(budget) = request.budget.as_ref() {
                if matches!(&result, Err(SubagentError::InvocationCancelled)) {
                    budget.record_provider_cancelled();
                } else if result.is_err() {
                    budget.record_provider_rejected();
                } else {
                    budget.record_provider_completed();
                }
            }
            metrics.provider_turns += 1;
            let result = match result {
                Ok(result) => result,
                Err(error) => {
                    lifecycle.push(match error {
                        SubagentError::InvocationCancelled => SubagentLifecycle::Cancelled,
                        _ => SubagentLifecycle::Failed,
                    });
                    return Ok(outcome(
                        request,
                        profile_id,
                        assignment,
                        status_for_error(error),
                        None,
                        usage,
                        started.elapsed().as_millis(),
                        lifecycle,
                        vec![error.to_string()],
                        &metrics,
                        false,
                        false,
                    ));
                }
            };
            if let Some(budget) = request.budget.as_ref() {
                if let Some(provider_usage) = result.usage.as_ref() {
                    let output_tokens = provider_usage.output_tokens;
                    if let Some(output_tokens) = output_tokens {
                        if let Err(error) = budget.record_output_tokens(output_tokens) {
                            lifecycle.push(SubagentLifecycle::BudgetExhausted);
                            return Ok(outcome(
                                request,
                                profile_id,
                                assignment,
                                SubagentStatus::BudgetExhausted,
                                None,
                                result.usage.clone(),
                                started.elapsed().as_millis(),
                                lifecycle,
                                vec![safe_budget_message(error)],
                                &metrics,
                                false,
                                true,
                            ));
                        }
                    }
                }
            }
            usage = result.usage.clone();
            if result.provider != assignment.provider_id || result.model != assignment.model_id {
                lifecycle.push(SubagentLifecycle::Failed);
                return Ok(outcome(
                    request,
                    profile_id,
                    assignment,
                    SubagentStatus::InvalidResponse,
                    None,
                    usage,
                    started.elapsed().as_millis(),
                    lifecycle,
                    vec![
                        "provider response routing metadata did not match the request".to_string(),
                    ],
                    &metrics,
                    false,
                    false,
                ));
            }
            let Some(tool_call) = result.tool_call else {
                let (output, output_truncated) =
                    bound_provider_output(result.text, request.max_output_tokens);
                lifecycle.push(if output_truncated {
                    SubagentLifecycle::Failed
                } else {
                    SubagentLifecycle::Completed
                });
                return Ok(outcome(
                    request,
                    profile_id,
                    assignment,
                    if output_truncated {
                        SubagentStatus::OutputLimitReached
                    } else {
                        SubagentStatus::Completed
                    },
                    Some(output),
                    usage,
                    started.elapsed().as_millis(),
                    lifecycle,
                    if output_truncated {
                        vec!["provider output exceeded the local byte cap".to_string()]
                    } else {
                        Vec::new()
                    },
                    &metrics,
                    output_truncated,
                    false,
                ));
            };
            lifecycle.push(SubagentLifecycle::AwaitingToolExecution);
            if metrics.tool_calls >= request.tool_policy.budget().max_tool_calls {
                lifecycle.push(SubagentLifecycle::BudgetExhausted);
                return Ok(outcome(
                    request,
                    profile_id,
                    assignment,
                    SubagentStatus::BudgetExhausted,
                    None,
                    usage,
                    started.elapsed().as_millis(),
                    lifecycle,
                    vec!["maximum tool calls reached".to_string()],
                    &metrics,
                    false,
                    true,
                ));
            }
            let tool = parse_tool_kind(&tool_call.name);
            let duplicate = !seen_call_ids.insert(tool_call.id.clone());
            metrics.tool_calls += 1;
            let call_started = Instant::now();
            lifecycle.push(SubagentLifecycle::ExecutingTool);
            let execution = if duplicate {
                Err(SubagentToolError::InvalidCallId)
            } else if tool.is_none() {
                Err(SubagentToolError::ToolNotApproved)
            } else if tool.is_some_and(|tool| {
                let count = metrics.tool_call_counts.entry(tool).or_default();
                *count += 1;
                *count > request.tool_policy.budget().max_calls_per_tool
            }) {
                Err(SubagentToolError::ToolNotApproved)
            } else if let Some(tool) = tool {
                let tool_ownership = request
                    .cleanup
                    .as_ref()
                    .map(|cleanup| {
                        cleanup.register_tool_reservation(
                            request
                                .budget
                                .as_ref()
                                .map_or(0, |budget| budget.generation()),
                        )
                    })
                    .transpose()
                    .map_err(|_| SubagentError::InternalFailure)?;
                let tool_reservation = match request
                    .budget
                    .as_ref()
                    .map(|budget| budget.reserve_tool(request.role))
                    .transpose()
                {
                    Ok(reservation) => reservation,
                    Err(error) => {
                        if let (Some(cleanup), Some(handle)) =
                            (request.cleanup.as_ref(), tool_ownership)
                        {
                            cleanup
                                .resolve_tool_reservation(
                                    request
                                        .budget
                                        .as_ref()
                                        .map_or(0, |budget| budget.generation()),
                                    handle,
                                )
                                .map_err(|_| SubagentError::InternalFailure)?;
                        }
                        lifecycle.push(SubagentLifecycle::BudgetExhausted);
                        return Ok(outcome(
                            request,
                            profile_id,
                            assignment,
                            SubagentStatus::BudgetExhausted,
                            None,
                            usage,
                            started.elapsed().as_millis(),
                            lifecycle,
                            vec![safe_budget_message(error)],
                            &metrics,
                            false,
                            true,
                        ));
                    }
                };
                let tool_child = match request
                    .cleanup
                    .as_ref()
                    .map(|cleanup| {
                        cleanup.register_child(
                            request
                                .budget
                                .as_ref()
                                .map_or(0, |budget| budget.generation()),
                            CleanupChildKind::Tool,
                        )
                    })
                    .transpose()
                {
                    Ok(child) => child,
                    Err(_) => {
                        if let (Some(cleanup), Some(handle)) =
                            (request.cleanup.as_ref(), tool_ownership)
                        {
                            cleanup
                                .resolve_tool_reservation(
                                    request
                                        .budget
                                        .as_ref()
                                        .map_or(0, |budget| budget.generation()),
                                    handle,
                                )
                                .map_err(|_| SubagentError::InternalFailure)?;
                        }
                        return Err(SubagentError::InternalFailure);
                    }
                };
                if let Some(reservation) = tool_reservation {
                    if reservation.commit().is_err() {
                        if let (Some(cleanup), Some(handle)) =
                            (request.cleanup.as_ref(), tool_ownership)
                        {
                            cleanup
                                .resolve_tool_reservation(
                                    request
                                        .budget
                                        .as_ref()
                                        .map_or(0, |budget| budget.generation()),
                                    handle,
                                )
                                .map_err(|_| SubagentError::InternalFailure)?;
                        }
                        return Err(SubagentError::InternalFailure);
                    }
                }
                if let (Some(cleanup), Some(handle)) = (request.cleanup.as_ref(), tool_ownership) {
                    cleanup
                        .resolve_tool_reservation(
                            request
                                .budget
                                .as_ref()
                                .map_or(0, |budget| budget.generation()),
                            handle,
                        )
                        .map_err(|_| SubagentError::InternalFailure)?;
                }
                let execution = tokio::select! {
                    _ = request.cancellation.cancelled() => Err(SubagentToolError::Cancelled),
                    result = tokio::time::timeout(
                        request.tool_policy.budget().per_tool_timeout,
                        execute_tool(
                            &request.tool_policy,
                            tool,
                            &tool_call.id,
                            &tool_call.arguments,
                            &request.cancellation,
                        ),
                    ) => result.unwrap_or(Err(SubagentToolError::Cancelled)),
                };
                if let (Some(cleanup), Some(handle)) = (request.cleanup.as_ref(), tool_child) {
                    cleanup
                        .complete_child(
                            request
                                .budget
                                .as_ref()
                                .map_or(0, |budget| budget.generation()),
                            handle,
                        )
                        .map_err(|_| SubagentError::InternalFailure)?;
                }
                execution
            } else {
                Err(SubagentToolError::ToolNotApproved)
            };
            if matches!(execution, Err(SubagentToolError::Cancelled)) {
                lifecycle.push(SubagentLifecycle::Cancelled);
                return Ok(outcome(
                    request,
                    profile_id,
                    assignment,
                    SubagentStatus::Cancelled,
                    None,
                    usage,
                    started.elapsed().as_millis(),
                    lifecycle,
                    vec!["subagent tool execution was cancelled".to_string()],
                    &metrics,
                    false,
                    false,
                ));
            }
            let (content, is_error, truncated, succeeded) = match execution {
                Ok(execution) => (execution.content, false, execution.truncated, true),
                Err(error) => (format!("tool error: {error}"), true, false, false),
            };
            let remaining = request
                .tool_policy
                .budget()
                .max_aggregate_tool_output_bytes
                .saturating_sub(metrics.aggregate_tool_output_bytes);
            let (content, aggregate_truncated) = bound_text(content, remaining);
            let truncated = truncated || aggregate_truncated;
            if let Some(budget) = request.budget.as_ref() {
                if let Err(error) = budget.record_tool_output(content.len()) {
                    lifecycle.push(SubagentLifecycle::BudgetExhausted);
                    return Ok(outcome(
                        request,
                        profile_id,
                        assignment,
                        SubagentStatus::BudgetExhausted,
                        None,
                        usage,
                        started.elapsed().as_millis(),
                        lifecycle,
                        vec![safe_budget_message(error)],
                        &metrics,
                        false,
                        true,
                    ));
                }
                budget.record_tool_completed();
            }
            metrics.aggregate_tool_output_bytes = metrics
                .aggregate_tool_output_bytes
                .saturating_add(content.len());
            metrics.tool_audit.push(SubagentToolCallRecord {
                tool: tool.unwrap_or(SubagentToolKind::GitStatus),
                call_id: tool_call.id.clone(),
                descriptor: tool
                    .map(|tool| tool.provider_name().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                succeeded,
                duration_ms: call_started.elapsed().as_millis(),
                input_bytes: tool_call.arguments.len(),
                output_bytes: content.len(),
                truncated,
            });
            lifecycle.push(SubagentLifecycle::ReturningToolResult);
            tool_results.push(ProviderInvocationToolResult {
                id: tool_call.id,
                content,
                is_error,
            });
        }
    }
}

fn validate_request(request: &SubagentRequest) -> Result<(), SubagentError> {
    if request.task_id.trim().is_empty() || request.task_id.len() > SUBAGENT_MAX_TASK_ID_BYTES {
        return Err(SubagentError::InvalidTaskId);
    }
    if request.instruction.trim().is_empty() {
        return Err(SubagentError::EmptyInstruction);
    }
    if request.instruction.len() > SUBAGENT_MAX_INSTRUCTION_BYTES {
        return Err(SubagentError::InstructionTooLarge);
    }
    if request
        .context
        .as_ref()
        .is_some_and(|context| context.len() > SUBAGENT_MAX_CONTEXT_BYTES)
    {
        return Err(SubagentError::ContextTooLarge);
    }
    if !(SUBAGENT_MIN_TIMEOUT..=SUBAGENT_MAX_TIMEOUT).contains(&request.timeout) {
        return Err(SubagentError::InvalidTimeout);
    }
    if !(SUBAGENT_MIN_OUTPUT_TOKENS..=SUBAGENT_MAX_OUTPUT_TOKENS)
        .contains(&request.max_output_tokens)
    {
        return Err(SubagentError::InvalidOutputLimit);
    }
    if request.depth != 1 {
        return Err(SubagentError::RecursionNotAllowed);
    }
    let budget = request.tool_policy.budget();
    if budget.max_provider_turns == 0
        || budget.max_tool_calls == 0
        || budget.max_calls_per_tool == 0
        || budget.max_tool_input_bytes == 0
        || budget.max_tool_output_bytes == 0
        || budget.max_aggregate_tool_output_bytes == 0
        || budget.max_file_bytes == 0
        || budget.max_file_read_bytes == 0
        || budget.max_file_read_lines == 0
        || budget.max_search_results == 0
        || budget.max_search_files == 0
        || budget.max_search_bytes == 0
        || budget.max_git_status_entries == 0
        || budget.max_git_output_bytes == 0
        || budget.session_timeout.is_zero()
        || budget.per_tool_timeout.is_zero()
    {
        return Err(SubagentError::InvalidToolPolicy);
    }
    if request.tool_policy.requires_workspace() && request.tool_policy.workspace_root().is_none() {
        return Err(SubagentError::InvalidToolPolicy);
    }
    Ok(())
}

fn safe_budget_message(error: BudgetExhaustion) -> String {
    error.to_string()
}

fn build_prompt(role: RoutingRole, instruction: &str, context: Option<&str>) -> (String, String) {
    let system = format!(
        "You are the Syndrid {role} subagent. You have no tools. Do not claim to have modified files or run commands. Return only a bounded result, concise summary, assumptions, unresolved issues, and verification status."
    );
    let user = match context {
        Some(context) => format!("Task:\n{instruction}\n\nBounded context:\n{context}"),
        None => format!("Task:\n{instruction}"),
    };
    (system, user)
}

fn outcome(
    request: &SubagentRequest,
    profile_id: &str,
    assignment: &super::routing_profiles::RoutingAssignment,
    status: SubagentStatus,
    output: Option<String>,
    usage: Option<super::invocation::ProviderInvocationUsage>,
    latency_ms: u128,
    lifecycle: Vec<SubagentLifecycle>,
    warnings: Vec<String>,
    metrics: &SessionMetrics,
    output_truncated: bool,
    budget_exhausted: bool,
) -> SubagentOutcome {
    SubagentOutcome {
        task_id: request.task_id.clone(),
        parent_id: request.parent_id.clone(),
        role: request.role,
        profile_id: profile_id.to_string(),
        provider_id: assignment.provider_id.clone(),
        connection_id: assignment.connection_id.clone(),
        model_id: assignment.model_id.clone(),
        status,
        output,
        usage: usage.map(|usage| SubagentUsage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
            quality: SubagentDataQuality::Exact,
        }),
        latency_ms,
        lifecycle,
        warnings,
        provider_turns: metrics.provider_turns,
        tool_calls: metrics.tool_calls,
        tool_call_counts: metrics.tool_call_counts.clone(),
        tool_audit: metrics.tool_audit.clone(),
        output_truncated,
        budget_exhausted,
    }
}

#[derive(Default)]
struct SessionMetrics {
    provider_turns: usize,
    tool_calls: usize,
    tool_call_counts: BTreeMap<SubagentToolKind, usize>,
    tool_audit: Vec<SubagentToolCallRecord>,
    aggregate_tool_output_bytes: usize,
}

fn parse_tool_kind(name: &str) -> Option<SubagentToolKind> {
    match name {
        "read_file" => Some(SubagentToolKind::ReadFile),
        "search_text" => Some(SubagentToolKind::SearchText),
        "git_status" => Some(SubagentToolKind::GitStatus),
        _ => None,
    }
}

fn bound_provider_output(value: String, max_output_tokens: u32) -> (String, bool) {
    let limit = usize::try_from(max_output_tokens)
        .unwrap_or(usize::MAX)
        .saturating_mul(4);
    bound_text(value, limit)
}

fn bound_text(value: String, limit: usize) -> (String, bool) {
    if value.len() <= limit {
        return (value, false);
    }
    let mut end = limit;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_string(), true)
}

fn map_routing_error(error: RoutingProfileError) -> SubagentError {
    match error {
        RoutingProfileError::UnknownConnection => SubagentError::UnknownConnection,
        RoutingProfileError::DisabledConnection => SubagentError::ConnectionDisabled,
        RoutingProfileError::UnvalidatedConnection => SubagentError::ConnectionUnconfigured,
        RoutingProfileError::ModelNotFound | RoutingProfileError::InvalidModelId => {
            SubagentError::InvalidModel
        }
        RoutingProfileError::UnsupportedAuthenticationMethod => SubagentError::UnsupportedProvider,
        RoutingProfileError::ProviderMismatch => SubagentError::UnknownProvider,
        RoutingProfileError::MissingRoleAssignment => SubagentError::MissingRoleAssignment,
        _ => SubagentError::InternalFailure,
    }
}

fn map_invocation_error(error: ProviderInvocationError) -> SubagentError {
    match error {
        ProviderInvocationError::Cancelled => SubagentError::InvocationCancelled,
        ProviderInvocationError::RequestTimedOut => SubagentError::InvocationTimedOut,
        ProviderInvocationError::Unauthorized
        | ProviderInvocationError::Forbidden
        | ProviderInvocationError::MissingCredentialReference
        | ProviderInvocationError::CredentialNotFound
        | ProviderInvocationError::CredentialStoreUnavailable
        | ProviderInvocationError::CredentialStoreRejected
        | ProviderInvocationError::ReauthenticationRequired => SubagentError::AuthenticationFailed,
        ProviderInvocationError::ProviderRejected
        | ProviderInvocationError::PaymentRequired
        | ProviderInvocationError::RateLimited => SubagentError::ProviderRejected,
        ProviderInvocationError::TransportUnavailable
        | ProviderInvocationError::ProviderUnavailable
        | ProviderInvocationError::StreamTerminated => SubagentError::TransportFailed,
        ProviderInvocationError::InvalidResponse
        | ProviderInvocationError::InvalidContentType
        | ProviderInvocationError::MissingOutput
        | ProviderInvocationError::ResponseTooLarge => SubagentError::InvalidResponse,
        ProviderInvocationError::InvalidConfiguration
        | ProviderInvocationError::UnsupportedProvider
        | ProviderInvocationError::UnsupportedAuthenticationMethod
        | ProviderInvocationError::ConnectionDisabled
        | ProviderInvocationError::ConnectionUnvalidated
        | ProviderInvocationError::InvalidModelId
        | ProviderInvocationError::InvalidRequest
        | ProviderInvocationError::InputTooLarge
        | ProviderInvocationError::OutputLimitInvalid
        | ProviderInvocationError::OrchestrationConversionFailed
        | ProviderInvocationError::LiveCodexInvocationUnavailable
        | ProviderInvocationError::ScopedSessionConstructionFailed => {
            SubagentError::InternalFailure
        }
    }
}

fn status_for_error(error: SubagentError) -> SubagentStatus {
    match error {
        SubagentError::InvocationCancelled => SubagentStatus::Cancelled,
        SubagentError::InvocationTimedOut => SubagentStatus::TimedOut,
        SubagentError::AuthenticationFailed => SubagentStatus::AuthenticationFailed,
        SubagentError::ProviderRejected => SubagentStatus::ProviderRejected,
        SubagentError::TransportFailed => SubagentStatus::TransportFailed,
        SubagentError::InvalidResponse => SubagentStatus::InvalidResponse,
        SubagentError::OutputLimitReached => SubagentStatus::OutputLimitReached,
        _ => SubagentStatus::InternalFailure,
    }
}

#[cfg(test)]
#[path = "subagent_tests.rs"]
mod tests;
