use super::invocation::ProviderInvocationError;
use super::invocation::ProviderInvocationRequest;
use super::invocation::ProviderInvocationResult;
use super::routing_profiles::RoutingConnectionDirectory;
use super::routing_profiles::RoutingProfileError;
use super::routing_profiles::RoutingProfileRegistry;
use super::routing_profiles::RoutingResolutionStatus;
use super::routing_profiles::RoutingRole;
use serde::Serialize;
use std::fmt;
use std::future::Future;
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
    InternalFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentLifecycle {
    Created,
    Validating,
    Routing,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
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
        let resolution = self
            .directory
            .validate_assignment(assignment)
            .map_err(map_routing_error)?;
        if resolution == RoutingResolutionStatus::ModelUnverified {
            return Err(SubagentError::ModelUnverified);
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
        let prompt = build_prompt(
            request.role,
            &request.instruction,
            request.context.as_deref(),
        );
        let provider_request = ProviderInvocationRequest {
            provider: assignment.provider_id.clone(),
            model: assignment.model_id.clone(),
            system: Some(prompt.0),
            user: prompt.1,
            max_output_tokens: request.max_output_tokens,
        };
        lifecycle.push(SubagentLifecycle::Running);
        let started = Instant::now();
        let result = tokio::select! {
            _ = request.cancellation.cancelled() => Err(SubagentError::InvocationCancelled),
            result = tokio::time::timeout(
                request.timeout,
                self.provider.invoke(provider_request, request.cancellation.clone()),
            ) => match result {
                Ok(Ok(result)) => Ok(result),
                Ok(Err(error)) => Err(map_invocation_error(error)),
                Err(_) => Err(SubagentError::InvocationTimedOut),
            },
        };
        let latency_ms = started.elapsed().as_millis();
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                let status = status_for_error(error);
                lifecycle.push(match status {
                    SubagentStatus::Cancelled => SubagentLifecycle::Cancelled,
                    SubagentStatus::TimedOut => SubagentLifecycle::TimedOut,
                    _ => SubagentLifecycle::Failed,
                });
                return Ok(outcome(
                    &request,
                    profile.id.as_str(),
                    assignment,
                    status,
                    None,
                    None,
                    latency_ms,
                    lifecycle,
                    vec![error.to_string()],
                ));
            }
        };
        if result.provider != assignment.provider_id || result.model != assignment.model_id {
            lifecycle.push(SubagentLifecycle::Failed);
            return Ok(outcome(
                &request,
                profile.id.as_str(),
                assignment,
                SubagentStatus::InvalidResponse,
                None,
                None,
                latency_ms,
                lifecycle,
                vec!["provider response routing metadata did not match the request".to_string()],
            ));
        }
        let output_limit = usize::try_from(request.max_output_tokens)
            .unwrap_or(usize::MAX)
            .saturating_mul(4);
        if result.text.len() > output_limit {
            let mut output = result.text;
            let mut end = output_limit;
            while !output.is_char_boundary(end) {
                end -= 1;
            }
            output.truncate(end);
            lifecycle.push(SubagentLifecycle::Failed);
            return Ok(outcome(
                &request,
                profile.id.as_str(),
                assignment,
                SubagentStatus::OutputLimitReached,
                Some(output),
                result.usage,
                latency_ms,
                lifecycle,
                vec!["provider output exceeded the local byte cap".to_string()],
            ));
        }
        lifecycle.push(SubagentLifecycle::Completed);
        Ok(outcome(
            &request,
            profile.id.as_str(),
            assignment,
            SubagentStatus::Completed,
            Some(result.text),
            result.usage,
            latency_ms,
            lifecycle,
            Vec::new(),
        ))
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
    Ok(())
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
    }
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
