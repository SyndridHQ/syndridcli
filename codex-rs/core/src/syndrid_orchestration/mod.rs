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

mod account_pools;
mod codex_accounts;
mod codex_invocation;
mod cooldown_state;
mod credential_store;
mod error;
mod execution_budget;
mod execution_budget_accounting;
mod execution_modes;
mod final_deliverable;
mod handoff;
mod invocation;
mod live;
mod live_coordinator;
mod live_coordinator_mapping;
mod live_coordinator_stages;
mod live_coordinator_types;
mod live_coordinator_validation;
mod native_credential_store;
mod observation_delivery;
mod omniroute;
mod openai_compatible;
mod openrouter_auth;
mod openrouter_callback;
mod openrouter_invocation;
mod openrouter_setup;
mod orchestration_cleanup;
mod orchestration_failure;
mod orchestration_observability;
mod orchestration_observability_runtime;
mod orchestration_policy;
mod production_dispatch;
mod production_lifecycle;
mod production_request;
mod provider_connection;
mod provider_construction;
mod provider_failure;
mod role_capabilities;
mod role_capability_config;
mod rotation_state;
mod routing_pool_bindings;
mod routing_profiles;
mod scoped_codex_session;
mod session_execution;
mod spawn;
mod subagent;
mod subagent_batch;
mod subagent_repair;
mod subagent_tools;
mod turn_result;

pub use account_pools::ACCOUNT_POOL_FILE;
pub use account_pools::AccountPoolError;
pub use account_pools::AccountPoolMember;
pub use account_pools::AccountPoolProviderFamily;
pub use account_pools::AccountPoolSelectionPolicy;
pub use account_pools::AccountPoolTarget;
pub use account_pools::MAX_ACCOUNT_POOL_FILE_BYTES;
pub use account_pools::NamedAccountPool;
pub use account_pools::NamedAccountPoolRegistry;
pub use account_pools::NamedAccountPoolStore;
pub use account_pools::PoolId;
pub use account_pools::PoolMemberId;
pub use account_pools::PoolMemberReadiness;
pub use account_pools::PoolReadiness;
pub use account_pools::PoolResolutionError;
pub use account_pools::ResolvedPoolMember;
pub use codex_accounts::CodexAccountConnectionMetadata;
pub use codex_accounts::CodexAccountProfileError;
pub use codex_accounts::CodexAccountProfileId;
pub use codex_accounts::CodexAccountProfileRegistry;
pub use codex_accounts::CodexAccountProfileState;
pub use codex_accounts::CodexAccountStore;
pub use codex_accounts::CodexCredentialEnvelope;
pub use codex_accounts::codex_auth_exists;
pub use codex_accounts::delete_codex_auth;
pub use codex_accounts::retrieve_codex_envelope;
pub use codex_accounts::store_codex_auth;
pub use codex_invocation::CODEX_PROVIDER_ID;
pub use codex_invocation::CodexCredentialProvider;
pub use codex_invocation::CodexInvocationAdapter;
pub use codex_invocation::CodexInvocationClient;
pub use codex_invocation::NativeCodexCredentialProvider;
pub use codex_invocation::UnavailableCodexInvocationClient;
pub use codex_invocation::invoke_codex;
pub use codex_orchestration::OrchestrationMode;
pub use cooldown_state::ProviderCooldownError;
pub use cooldown_state::ProviderCooldownKey;
pub use cooldown_state::ProviderCooldownState;
pub use cooldown_state::ProviderCooldownStatus;
pub use execution_budget::BudgetExhaustion;
pub use execution_budget::BudgetExhaustionCategory;
pub use execution_budget::ExecutionBudgetLedger;
pub use execution_budget::ExecutionBudgetLimits;
pub use execution_budget::ExecutionBudgetSnapshot;
pub use execution_modes::BuiltInExecutionMode;
pub use execution_modes::ExecutionModeSelection;
pub use execution_modes::ExecutionPolicy;
pub use execution_modes::ExecutionPolicyError;
pub use execution_modes::ExecutionShape;
pub use execution_modes::PolicySource;
pub use execution_modes::RepairPolicyDecision;
pub use execution_modes::ResolvedExecutionPolicy;
pub use execution_modes::ResolvedExecutionPolicyExplanation;
pub use execution_modes::RoleActivation;
pub use execution_modes::RoleExecutionPolicy;
pub use final_deliverable::ProductionFinalDeliverableInput;
pub use final_deliverable::ProductionFinalDeliverableProducer;
pub use live_coordinator::LiveOrchestrationCoordinator;
pub use live_coordinator_types::LiveEvent;
pub use live_coordinator_types::LiveOrchestrationError;
pub use live_coordinator_types::LiveOrchestrationOutcome;
pub use live_coordinator_types::LiveOrchestrationRequest;
pub use live_coordinator_types::LiveOrchestrationTerminal;
pub use live_coordinator_types::LiveRepairResult;
pub use live_coordinator_types::LiveRoleOutcome;
pub use live_coordinator_types::LiveRoleSkipReason;
pub use live_coordinator_types::LiveRoleState;
pub use live_coordinator_types::PlannerTaskSpecification;
pub use live_coordinator_types::PlanningContract;
pub use live_coordinator_types::VerificationContract;
pub use live_coordinator_types::VerificationDecision;
pub use observation_delivery::OrchestrationObservationReceiver;
pub use observation_delivery::OrchestrationObservationSink;
pub use observation_delivery::OrchestrationObservationUpdate;
pub use observation_delivery::WatchOrchestrationObservationSink;
pub use observation_delivery::observation_channel;
pub use orchestration_failure::OrchestrationFailure;
pub use orchestration_failure::OrchestrationFailureKind;
pub use orchestration_failure::Retryability;
pub use orchestration_observability::ObservationBudget;
pub use orchestration_observability::ObservationCleanupState;
pub use orchestration_observability::ObservationFailureState;
pub use orchestration_observability::ObservationQuality;
pub use orchestration_observability::ObservationTerminalReason;
pub use orchestration_observability::Observed;
pub use orchestration_observability::ObservedActiveRole;
pub use orchestration_observability::OrchestrationObservationSnapshot;
pub use orchestration_observability::OrchestrationObservationStage;
pub use orchestration_policy::OrchestrationStrategyAvailability;
pub use orchestration_policy::OrchestrationStrategyUnavailableReason;
pub use orchestration_policy::ResolvedOrchestrationPolicy;
pub use scoped_codex_session::ScopedCodexInvocationClient;
pub use scoped_codex_session::ScopedCodexSession;
pub use session_execution::SessionExecutionPolicyState;
pub use session_execution::SessionExecutionStateError;
pub use session_execution::SessionExecutionStatus;
pub use session_execution::SessionPolicySource;
pub use session_execution::SessionPolicySummary;
pub use session_execution::SessionPolicyValidation;
pub use session_execution::SessionRoutingUpdateGuard;
pub use turn_result::MAX_USER_FACING_RESPONSE_BYTES;
pub use turn_result::OrchestrationCleanupFailure;
pub use turn_result::OrchestrationEvidence;
pub use turn_result::OrchestrationOperationalMetadata;
pub use turn_result::OrchestrationPartialCause;
pub use turn_result::OrchestrationTurnResult;
pub use turn_result::OrchestrationTurnResultBuilder;
pub use turn_result::UserFacingResponse;
pub use turn_result::UserFacingResponseError;

#[cfg(test)]
#[path = "execution_budget_tests.rs"]
mod execution_budget_tests;
#[cfg(test)]
#[path = "execution_modes_tests.rs"]
mod execution_modes_tests;
#[cfg(test)]
#[path = "final_deliverable_tests.rs"]
mod final_deliverable_tests;

#[cfg(test)]
#[path = "live_coordinator_tests.rs"]
mod live_coordinator_tests;
#[cfg(test)]
#[path = "orchestration_cleanup_tests.rs"]
mod orchestration_cleanup_tests;
#[cfg(test)]
#[path = "orchestration_failure_tests.rs"]
mod orchestration_failure_tests;
#[cfg(test)]
#[path = "orchestration_observability_tests.rs"]
mod orchestration_observability_tests;
#[cfg(test)]
#[path = "production_dispatch_tests.rs"]
mod production_dispatch_tests;
#[cfg(test)]
#[path = "production_request_tests.rs"]
mod production_request_tests;
#[cfg(test)]
#[path = "routing_pool_bindings_tests.rs"]
mod routing_pool_bindings_tests;
#[cfg(test)]
#[path = "scoped_codex_session_tests.rs"]
mod scoped_codex_session_tests;
#[cfg(test)]
#[path = "session_execution_tests.rs"]
mod session_execution_tests;
#[cfg(test)]
#[path = "subagent_batch_tests.rs"]
mod subagent_batch_tests;
#[cfg(test)]
#[path = "subagent_repair_tests.rs"]
mod subagent_repair_tests;
#[cfg(test)]
#[path = "subagent_tools_tests.rs"]
mod subagent_tools_tests;
pub use invocation::ProviderInvocationError;
pub use invocation::ProviderInvocationRequest;
pub use invocation::ProviderInvocationResult;
pub use invocation::ProviderInvocationToolCall;
pub use invocation::ProviderInvocationToolDefinition;
pub use invocation::ProviderInvocationToolResult;
pub use invocation::ProviderInvocationUsage;
pub use omniroute::OMNIROUTE_DEFAULT_BASE_URL;
pub use omniroute::OMNIROUTE_PROVIDER_ID;
pub use omniroute::OmniRouteConnectionMetadata;
pub use omniroute::OmniRouteConnectionSetupRequest;
pub use omniroute::OmniRouteRegistry;
pub use omniroute::ProviderSelection;
pub use omniroute::delete_omniroute_credential;
pub use omniroute::invoke_omniroute;
pub use omniroute::list_omniroute_models;
pub use omniroute::omniroute_credential_exists;
pub use omniroute::provider_credential_exists;
pub use omniroute::setup_omniroute;
pub use openrouter_auth::OpenRouterAuthError;
pub use openrouter_callback::CallbackServerError;
pub use openrouter_setup::BrowserLaunchStatus;
pub use openrouter_setup::OpenRouterConnectionMetadata;
pub use openrouter_setup::OpenRouterSetupError;
pub use openrouter_setup::OpenRouterSetupRequest;
pub use openrouter_setup::OpenRouterSetupStarted;
pub use openrouter_setup::setup_openrouter;
pub use production_dispatch::ProductionRoleBinding;
pub use production_dispatch::ProductionRoleDispatchError;
pub use production_dispatch::ProductionRoleDispatcher;
pub use production_dispatch::ProductionRoleInvocationRequest;
pub use production_lifecycle::ProductionCancellationReason;
pub use production_lifecycle::ProductionOrchestrationCancellationHandle;
pub use production_lifecycle::ProductionOrchestrationLifecycle;
pub use production_lifecycle::ProductionOrchestrationLifecycleError;
pub use production_lifecycle::ProductionOrchestrationLifecycleState;
pub use production_request::ProductionOrchestrationInput;
pub use production_request::ProductionOrchestrationRequestBuilder;
pub use production_request::ProductionProviderAdapter;
pub use production_request::ProductionProviderRoute;
pub use production_request::ProductionRequestError;
pub use provider_connection::ConnectionValidationStatus;
pub use provider_construction::ProductionProviderConstructionBinding;
pub use provider_construction::ProductionProviderConstructionSnapshot;
pub use provider_construction::ProductionRoundRobinProviderBinding;
pub use provider_construction::ProviderConstructionError;
pub use provider_construction::native_codex_binding;
pub use provider_construction::omniroute_binding;
pub use provider_construction::openrouter_binding;
pub use provider_failure::MAX_PROVIDER_COOLDOWN;
pub use provider_failure::ProviderCooldownHint;
pub use provider_failure::ProviderFailureClass;
pub use provider_failure::ProviderFailureClassification;
pub use provider_failure::ProviderFailureCode;
pub use provider_failure::ProviderFailureEvidence;
pub use provider_failure::ProviderFailureInput;
pub use provider_failure::ProviderTransportKind;
pub use provider_failure::classify_provider_failure;
pub use provider_failure::classify_provider_invocation_error;
pub use role_capabilities::ExplicitRoleCapability;
pub use role_capabilities::RoleCapabilityApproval;
pub use role_capabilities::RoleCapabilityConfiguration;
pub use role_capabilities::RoleCapabilityDeclaration;
pub use role_capabilities::RoleCapabilityPermission;
pub use role_capabilities::RoleCapabilityState;
pub use role_capabilities::RoleCapabilityValidationContext;
pub use role_capabilities::RoleCapabilityValidationError;
pub use role_capabilities::ValidatedRoleCapability;
pub use role_capabilities::ValidatedRoleCapabilitySet;
pub use role_capabilities::validate_role_capabilities;
pub use role_capability_config::ROLE_CAPABILITY_FILE;
pub use role_capability_config::RoleCapabilityConfigError;
pub use role_capability_config::load_role_capabilities;
pub use rotation_state::AccountPoolRotationState;
pub use rotation_state::PoolRotationError;
pub use rotation_state::PoolRotationFingerprint;
pub use rotation_state::PoolRotationKey;
pub use rotation_state::PoolSelectionReservation;
pub use routing_pool_bindings::RoutingPoolResolutionError;
pub use routing_pool_bindings::resolve_routing_profile;
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
pub use subagent::SubagentDataQuality;
pub use subagent::SubagentError;
pub use subagent::SubagentLifecycle;
pub use subagent::SubagentOutcome;
pub use subagent::SubagentProvider;
pub use subagent::SubagentRequest;
pub use subagent::SubagentRuntime;
pub use subagent::SubagentStatus;
pub use subagent::SubagentUsage;
pub use subagent_batch::SubagentBatchError;
pub use subagent_batch::SubagentBatchOutcome;
pub use subagent_batch::SubagentBatchRequest;
pub use subagent_batch::SubagentBatchRuntime;
pub use subagent_batch::SubagentBatchStatus;
pub use subagent_batch::SubagentConcurrencyPolicy;
pub use subagent_batch::SubagentFailurePolicy;
pub use subagent_batch::SubagentResultOrdering;
pub use subagent_batch::SubagentTask;
pub use subagent_batch::SubagentTaskOutcome;
pub use subagent_batch::SubagentTaskState;
pub use subagent_repair::SubagentAttemptKind;
pub use subagent_repair::SubagentAttemptOutcome;
pub use subagent_repair::SubagentAttemptState;
pub use subagent_repair::SubagentRepairBatchOutcome;
pub use subagent_repair::SubagentRepairBatchRequest;
pub use subagent_repair::SubagentRepairBatchRuntime;
pub use subagent_repair::SubagentRepairBudget;
pub use subagent_repair::SubagentRepairEligibility;
pub use subagent_repair::SubagentRepairError;
pub use subagent_repair::SubagentRepairFailureCategory;
pub use subagent_repair::SubagentRepairOutcome;
pub use subagent_repair::SubagentRepairPolicy;
pub use subagent_repair::SubagentRepairRoute;
pub use subagent_repair::SubagentRepairRuntime;
pub use subagent_repair::SubagentRepairTerminal;
pub use subagent_tools::ProductionApprovedToolAdapter;
pub use subagent_tools::SubagentSessionBudget;
pub use subagent_tools::SubagentToolCallRecord;
pub use subagent_tools::SubagentToolError;
pub use subagent_tools::SubagentToolKind;
pub use subagent_tools::SubagentToolPolicy;

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
