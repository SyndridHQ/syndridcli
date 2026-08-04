//! Trusted, non-activating assembly of one embedded Syndrid session runtime.
//!
//! This module consumes an already captured composition snapshot. It does not
//! reread configuration, select routes, invoke providers or tools, or create
//! per-turn lifecycle state. The returned runtime remains inert until a later
//! activation milestone admits a turn.

use std::sync::Arc;

use crate::production_orchestration_turn::ProductionOrchestrationTurnRunner;
use crate::production_orchestration_turn::ProductionOrchestrationTurnRunnerInput;
use crate::production_runner_adapter::ProductionOrchestrationTurnRunnerFactory;
use crate::trusted_composition::AuthoritativeSyndridCompositionSnapshot;
use crate::trusted_runtime::TrustedProductionRuntimeBuilder;
use crate::trusted_runtime::TrustedProductionRuntimeDependencies;
use crate::trusted_runtime::TrustedRuntimeConstructionError;
use codex_app_server::OrchestrationTranscriptContext;
use codex_app_server::ProductionSessionRuntime;
use codex_app_server::ProductionTurnPreparationError;
use codex_app_server::ProductionTurnRunnerFactory;
use codex_core::OpenRouterSetupCancellation as CancellationToken;
use codex_core::OrchestrationStrategyAvailability;
use codex_core::OrchestrationStrategyUnavailableReason;
use codex_core::PlanningContract;
use codex_core::ProductionApprovedToolAdapter;
use codex_core::ProductionOrchestrationInput;
use codex_core::ProductionOrchestrationRequestBuilder;
use codex_core::ProductionRoleDispatcher;
use codex_core::ProviderConstructionError;
use codex_core::RoleActivation;
use codex_core::RoutingProfileRegistry;
use codex_core::RoutingRole;
use codex_core::SessionExecutionPolicyState;
use codex_core::SubagentFailurePolicy;
use codex_core::SubagentSessionBudget;
use codex_core::VerificationContract;

#[cfg(test)]
#[path = "trusted_runtime_assembly_tests.rs"]
mod tests;

/// Bounded failures raised while assembling a trusted session runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustedRuntimeAssemblyError {
    CodexCompatibilitySelected,
    StrategyUnavailable(OrchestrationStrategyUnavailableReason),
    ProviderConstructionUnavailable,
    RoleDispatcherUnavailable,
    RunnerFactoryUnavailable,
    RuntimeUnavailable,
}

impl std::fmt::Display for TrustedRuntimeAssemblyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::CodexCompatibilitySelected => {
                "Codex compatibility strategy does not use a Syndrid runtime"
            }
            Self::StrategyUnavailable(reason) => return write!(formatter, "{reason}"),
            Self::ProviderConstructionUnavailable => {
                "provider construction authority is unavailable"
            }
            Self::RoleDispatcherUnavailable => "production role dispatcher is unavailable",
            Self::RunnerFactoryUnavailable => "production runner factory is unavailable",
            Self::RuntimeUnavailable => "trusted production runtime is unavailable",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for TrustedRuntimeAssemblyError {}

/// Assembles one inert, session-scoped production runtime from exact trusted state.
pub fn assemble_trusted_production_runtime(
    snapshot: &AuthoritativeSyndridCompositionSnapshot,
    policy_state: SessionExecutionPolicyState,
) -> Result<ProductionSessionRuntime, TrustedRuntimeAssemblyError> {
    if matches!(
        snapshot.strategy_availability,
        OrchestrationStrategyAvailability::Unavailable(_)
    ) {
        let OrchestrationStrategyAvailability::Unavailable(reason) = snapshot.strategy_availability
        else {
            unreachable!();
        };
        return Err(TrustedRuntimeAssemblyError::StrategyUnavailable(reason));
    }
    if snapshot.strategy == codex_core::OrchestrationMode::Single {
        return Err(TrustedRuntimeAssemblyError::CodexCompatibilitySelected);
    }
    let mut profiles = RoutingProfileRegistry::default();
    profiles
        .insert(snapshot.routing.profile.clone())
        .map_err(|_| TrustedRuntimeAssemblyError::RunnerFactoryUnavailable)?;
    profiles
        .activate(&snapshot.routing.profile_id)
        .map_err(|_| TrustedRuntimeAssemblyError::RunnerFactoryUnavailable)?;
    let connections = snapshot.routing.connections.clone();

    let mut bindings = Vec::new();
    let mut round_robin_bindings = Vec::new();
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ] {
        if snapshot.policy.role(role).activation == RoleActivation::Disabled {
            continue;
        }
        if snapshot.provider_construction.is_round_robin(role) {
            let binding = snapshot
                .provider_construction
                .round_robin_binding(role)
                .map_err(map_provider_error)?
                .clone();
            round_robin_bindings.push((role, binding));
        } else {
            let binding = snapshot
                .provider_construction
                .build_role_binding(role)
                .map_err(map_provider_error)?;
            bindings.push((role, binding));
        }
    }
    let dispatcher = ProductionRoleDispatcher::with_round_robin(
        bindings,
        round_robin_bindings,
        policy_state.rotation_state(),
    )
    .map(|dispatcher| dispatcher.with_session_state(policy_state.clone()))
    .map_err(|_| TrustedRuntimeAssemblyError::RoleDispatcherUnavailable)?;
    let mut tool_budget = SubagentSessionBudget::default();
    tool_budget.max_provider_turns = snapshot.policy.policy().max_provider_invocations;
    tool_budget.max_tool_calls = snapshot.policy.policy().max_tool_calls;
    tool_budget.max_tool_output_bytes = snapshot.policy.policy().max_tool_output_bytes;
    tool_budget.max_aggregate_tool_output_bytes = snapshot.policy.policy().max_tool_output_bytes;
    let tool_adapter = ProductionApprovedToolAdapter::from_role_capabilities(
        snapshot.workspace_root.clone(),
        snapshot.approved_tools.role_capabilities.clone(),
        tool_budget,
    )
    .map_err(|_| TrustedRuntimeAssemblyError::RunnerFactoryUnavailable)?;

    let mode = snapshot.policy.selected_mode().clone();
    let profile_id = snapshot.routing.profile_id.clone();
    let factory_profiles = profiles.clone();
    let factory_connections = connections.clone();
    let factory_dispatcher = dispatcher.clone();
    let factory_tool_adapter = tool_adapter.clone();
    let factory_policy_state = policy_state.clone();
    let factory_strategy = snapshot.strategy;
    let overall_timeout = snapshot.policy.policy().batch_timeout;
    let factory = ProductionOrchestrationTurnRunnerFactory::new(move |admission, context| {
        let builder = ProductionOrchestrationRequestBuilder::new(
            mode.clone(),
            profile_id.clone(),
            factory_profiles.clone(),
            factory_connections.clone(),
        )
        .map_err(|_| ProductionTurnPreparationError::RunnerUnavailable)?;
        let input = ProductionOrchestrationInput {
            run_id: admission.turn_id().to_owned(),
            instruction: admission.objective().to_owned(),
            context: context.map(str::to_owned),
            workspace_root: admission.workspace_root().to_path_buf(),
            tasks: Vec::new(),
            planning: PlanningContract::NotRequested,
            verification: VerificationContract::NotRequested,
            failure_policy: SubagentFailurePolicy::ContinueIndependent,
            repair_instruction: String::new(),
            approved_tool_policy: factory_tool_adapter.policy().clone(),
            cancellation: CancellationToken::new(),
            overall_timeout: Some(overall_timeout),
        };
        ProductionOrchestrationTurnRunner::new(ProductionOrchestrationTurnRunnerInput {
            strategy: factory_strategy,
            builder,
            input,
            policy_state: factory_policy_state.clone(),
            dispatcher: factory_dispatcher.clone(),
            profiles: factory_profiles.clone(),
            connections: factory_connections.clone(),
            tool_adapter: factory_tool_adapter.clone(),
            transcript_context: OrchestrationTranscriptContext {
                thread_id: admission.thread_id().to_owned(),
                turn_id: admission.turn_id().to_owned(),
                assistant_item_id: format!("syndrid-assistant-{}", admission.turn_id()),
                completed_at_ms: 0,
            },
        })
        .map_err(|_| ProductionTurnPreparationError::RunnerUnavailable)
    });

    let dependencies = TrustedProductionRuntimeDependencies {
        session_id: snapshot.session_id.clone(),
        runner_factory: Some(Arc::new(factory) as Arc<dyn ProductionTurnRunnerFactory>),
        context_provider: Some(Arc::clone(&snapshot.context_provider)),
    };
    TrustedProductionRuntimeBuilder::new(dependencies)
        .build(snapshot.event_sender.clone())
        .map_err(map_runtime_error)
}

fn map_provider_error(_error: ProviderConstructionError) -> TrustedRuntimeAssemblyError {
    TrustedRuntimeAssemblyError::ProviderConstructionUnavailable
}

fn map_runtime_error(_error: TrustedRuntimeConstructionError) -> TrustedRuntimeAssemblyError {
    TrustedRuntimeAssemblyError::RuntimeUnavailable
}
