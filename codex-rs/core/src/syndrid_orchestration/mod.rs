//! Core-owned implementation of the narrow Syndrid O2A runtime boundary.
//!
//! This module delegates child creation and message delivery to the existing `AgentControl`.
//! It owns no thread, graph, persistence, token, or event state of its own.

use crate::AgentControl;
use crate::config::Config;
use codex_orchestration_adapter::AdapterError;
use codex_orchestration_adapter::DeliverHandoffRequest;
use codex_orchestration_adapter::DeliverHandoffResult;
use codex_orchestration_adapter::SpawnChildRequest;
use codex_orchestration_adapter::SpawnChildResult;
use codex_protocol::ThreadId;
use codex_protocol::protocol::SessionSource;

mod codex_accounts;
mod codex_invocation;
mod credential_store;
mod error;
mod handoff;
mod invocation;
mod live;
mod native_credential_store;
mod omniroute;
mod openai_compatible;
mod openrouter_auth;
mod openrouter_callback;
mod openrouter_invocation;
mod openrouter_setup;
mod provider_connection;
mod routing_profiles;
mod spawn;

pub use codex_accounts::CodexAccountConnectionMetadata;
pub use codex_accounts::CodexAccountProfileError;
pub use codex_accounts::CodexAccountProfileId;
pub use codex_accounts::CodexAccountProfileRegistry;
pub use codex_accounts::CodexAccountProfileState;
pub use codex_accounts::CodexAccountStore;
pub use codex_accounts::CodexCredentialEnvelope;
pub use codex_accounts::delete_codex_auth;
pub use codex_accounts::retrieve_codex_envelope;
pub use codex_accounts::store_codex_auth;
pub use codex_invocation::CodexCredentialProvider;
pub use codex_invocation::CodexInvocationAdapter;
pub use codex_invocation::CodexInvocationClient;
pub use codex_invocation::NativeCodexCredentialProvider;
pub use codex_invocation::UnavailableCodexInvocationClient;
pub use invocation::ProviderInvocationRequest;
pub use omniroute::OMNIROUTE_DEFAULT_BASE_URL;
pub use omniroute::OMNIROUTE_PROVIDER_ID;
pub use omniroute::OmniRouteConnectionMetadata;
pub use omniroute::OmniRouteConnectionSetupRequest;
pub use omniroute::OmniRouteRegistry;
pub use omniroute::ProviderSelection;
pub use omniroute::delete_omniroute_credential;
pub use omniroute::invoke_omniroute;
pub use omniroute::list_omniroute_models;
pub use omniroute::setup_omniroute;
pub use openrouter_auth::OpenRouterAuthError;
pub use openrouter_callback::CallbackServerError;
pub use openrouter_setup::BrowserLaunchStatus;
pub use openrouter_setup::OpenRouterSetupError;
pub use openrouter_setup::OpenRouterSetupRequest;
pub use openrouter_setup::OpenRouterSetupStarted;
pub use openrouter_setup::setup_openrouter;
pub use provider_connection::ConnectionValidationStatus;
pub use routing_profiles::RoutingAssignment;
pub use routing_profiles::RoutingConnectionDirectory;
pub use routing_profiles::RoutingConnectionInfo;
pub use routing_profiles::RoutingProfile;
pub use routing_profiles::RoutingProfileError;
pub use routing_profiles::RoutingProfileId;
pub use routing_profiles::RoutingProfileRegistry;
pub use routing_profiles::RoutingProfileStore;
pub use routing_profiles::RoutingResolutionStatus;
pub use routing_profiles::RoutingRole;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "live_tests.rs"]
mod live_tests;

/// Core-owned bridge for the O2A spawn and handoff contracts.
///
/// The caller supplies the current native parent context and effective base configuration. The
/// bridge delegates lifecycle, graph, persistence, permission enforcement, and event work to the
/// existing Codex runtime through `AgentControl`.
pub(crate) struct CodexOrchestrationAdapter {
    agent_control: AgentControl,
    base_config: Config,
    parent_thread_id: ThreadId,
    parent_session_source: SessionSource,
}

#[derive(Clone, Debug)]
pub(super) struct TerminalSnapshot {
    pub(super) runtime_id: codex_orchestration_adapter::RuntimeAgentId,
    pub(super) status: codex_protocol::protocol::AgentStatus,
}

impl CodexOrchestrationAdapter {
    pub(crate) fn new(
        agent_control: AgentControl,
        base_config: Config,
        parent_thread_id: ThreadId,
        parent_session_source: SessionSource,
    ) -> Self {
        Self {
            agent_control,
            base_config,
            parent_thread_id,
            parent_session_source,
        }
    }

    pub(crate) async fn spawn_child(
        &self,
        request: SpawnChildRequest,
    ) -> Result<SpawnChildResult, AdapterError> {
        spawn::spawn_child(self, request).await
    }

    pub(crate) async fn deliver_handoff(
        &self,
        request: DeliverHandoffRequest,
    ) -> Result<DeliverHandoffResult, AdapterError> {
        handoff::deliver_handoff(self, request).await
    }

    pub(super) async fn invoke_provider<P: invocation::ProviderInvocation>(
        &self,
        provider: &P,
        request: invocation::ProviderInvocationRequest,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<invocation::ProviderInvocationResult, AdapterError> {
        invocation::invoke_provider(provider, request, cancellation).await
    }

    async fn run_sequential_workflow(
        &self,
        workflow: codex_orchestration::SequentialWorkflow,
        initial_input: codex_orchestration::StageInput,
        assignments: [live::StageAssignment; 5],
    ) -> codex_orchestration::SequentialWorkflow {
        let mut runner = live::SequentialRunner::new(self, workflow);
        runner.run(initial_input, assignments).await
    }

    pub(super) async fn run_provider_sequential_workflow<P: invocation::ProviderInvocation>(
        &self,
        provider: &P,
        workflow: codex_orchestration::SequentialWorkflow,
        initial_input: codex_orchestration::StageInput,
        assignments: [live::StageAssignment; 5],
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<codex_orchestration::SequentialWorkflow, AdapterError> {
        invocation::run_provider_sequential_workflow(
            provider,
            workflow,
            initial_input,
            assignments,
            cancellation,
        )
        .await
    }

    pub(super) async fn wait_for_terminal(
        &self,
        runtime_id: codex_orchestration_adapter::RuntimeAgentId,
        attribution: (
            &codex_orchestration::WorkflowId,
            &codex_orchestration::TaskId,
            &codex_orchestration::AgentId,
        ),
    ) -> Result<TerminalSnapshot, AdapterError> {
        let thread_id =
            codex_protocol::ThreadId::try_from(runtime_id.as_str()).map_err(|error| {
                error::adapter_error(
                    codex_orchestration_adapter::AdapterErrorKind::InvalidRequest,
                    error.to_string(),
                    codex_orchestration_adapter::Retryability::NotRetryable,
                    attribution,
                )
            })?;
        let mut status = self
            .agent_control
            .subscribe_status(thread_id)
            .await
            .map_err(|error| error::map_native_error(error, attribution))?;
        loop {
            let current = status.borrow().clone();
            if !matches!(
                current,
                codex_protocol::protocol::AgentStatus::PendingInit
                    | codex_protocol::protocol::AgentStatus::Running
            ) {
                return Ok(TerminalSnapshot {
                    runtime_id,
                    status: current,
                });
            }
            status.changed().await.map_err(|_| {
                error::adapter_error(
                    codex_orchestration_adapter::AdapterErrorKind::RuntimeUnavailable,
                    "native child status stream closed before terminal state",
                    codex_orchestration_adapter::Retryability::NotRetryable,
                    attribution,
                )
            })?;
        }
    }
}
