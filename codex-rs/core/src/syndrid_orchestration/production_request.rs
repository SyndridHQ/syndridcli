use super::execution_modes::ExecutionModeSelection;
use super::execution_modes::ExecutionPolicyError;
use super::execution_modes::ResolvedExecutionPolicy;
use super::invocation::ProviderInvocation;
use super::invocation::ProviderInvocationError;
use super::invocation::ProviderInvocationRequest;
use super::invocation::ProviderInvocationResult;
use super::live_coordinator_types::LiveOrchestrationRequest;
use super::live_coordinator_types::PlannerTaskSpecification;
use super::live_coordinator_types::PlanningContract;
use super::live_coordinator_types::VerificationContract;
use super::routing_profiles::RoutingConnectionDirectory;
use super::routing_profiles::RoutingProfileError;
use super::routing_profiles::RoutingProfileId;
use super::routing_profiles::RoutingProfileRegistry;
use super::routing_profiles::RoutingRole;
use super::subagent::SUBAGENT_MAX_TASK_ID_BYTES;
use super::subagent::SubagentProvider;
use super::subagent_batch::SubagentFailurePolicy;
use super::subagent_tools::SubagentToolError;
use super::subagent_tools::SubagentToolPolicy;
use codex_protocol::openai_models::ReasoningEffort;
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const MAX_REPAIR_INSTRUCTION_BYTES: usize = 32 * 1024;

/// Immutable, already-resolved production state for one future orchestration turn.
#[derive(Clone)]
pub struct ProductionOrchestrationInput {
    pub run_id: String,
    pub instruction: String,
    pub context: Option<String>,
    pub workspace_root: PathBuf,
    pub tasks: Vec<PlannerTaskSpecification>,
    pub planning: PlanningContract,
    pub verification: VerificationContract,
    pub failure_policy: SubagentFailurePolicy,
    pub repair_instruction: String,
    pub approved_tool_policy: SubagentToolPolicy,
    pub cancellation: CancellationToken,
    pub overall_timeout: Option<Duration>,
}

impl std::fmt::Debug for ProductionOrchestrationInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProductionOrchestrationInput")
            .field("run_id_bytes", &self.run_id.len())
            .field("instruction_bytes", &self.instruction.len())
            .field(
                "context_bytes",
                &self.context.as_ref().map_or(0, String::len),
            )
            .field("has_workspace_root", &true)
            .field("task_count", &self.tasks.len())
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProductionRequestError {
    #[error("production execution policy is invalid: {0}")]
    InvalidExecutionPolicy(#[from] ExecutionPolicyError),
    #[error("production routing profile is invalid: {0}")]
    InvalidRoutingProfile(#[from] RoutingProfileError),
    #[error("production approved-tool policy is invalid: {0}")]
    InvalidToolPolicy(#[from] SubagentToolError),
    #[error("production request field is invalid: {0}")]
    InvalidField(&'static str),
    #[error("production workspace does not match the approved-tool workspace")]
    WorkspaceMismatch,
}

/// Converts trusted, resolved turn state into the existing coordinator request contract.
///
/// It validates the captured policy, profile, and connection directory without selecting
/// fallbacks or invoking a provider. Mutable UI and session selectors are never consulted.
pub struct ProductionOrchestrationRequestBuilder {
    policy: ResolvedExecutionPolicy,
    routing_profile_id: RoutingProfileId,
    profiles: RoutingProfileRegistry,
    connections: RoutingConnectionDirectory,
}

impl ProductionOrchestrationRequestBuilder {
    pub fn new(
        mode: ExecutionModeSelection,
        routing_profile_id: RoutingProfileId,
        profiles: RoutingProfileRegistry,
        connections: RoutingConnectionDirectory,
    ) -> Result<Self, ProductionRequestError> {
        let policy = mode.resolve()?;
        let profile = profiles
            .get(&routing_profile_id)
            .ok_or(RoutingProfileError::UnknownProfile)?;
        policy.validate_routing_profile(profile, &connections)?;
        Ok(Self {
            policy,
            routing_profile_id,
            profiles,
            connections,
        })
    }

    pub fn build(
        &self,
        input: ProductionOrchestrationInput,
    ) -> Result<LiveOrchestrationRequest, ProductionRequestError> {
        if input.run_id.trim().is_empty()
            || input.run_id.len() > super::live_coordinator_types::MAX_RUN_ID_BYTES
        {
            return Err(ProductionRequestError::InvalidField("run_id"));
        }
        if input.instruction.trim().is_empty()
            || input.instruction.len() > super::live_coordinator_types::MAX_INSTRUCTION_BYTES
        {
            return Err(ProductionRequestError::InvalidField("instruction"));
        }
        if input
            .context
            .as_ref()
            .is_some_and(|context| context.len() > super::live_coordinator_types::MAX_CONTEXT_BYTES)
        {
            return Err(ProductionRequestError::InvalidField("context"));
        }
        if input.repair_instruction.len() > MAX_REPAIR_INSTRUCTION_BYTES {
            return Err(ProductionRequestError::InvalidField("repair_instruction"));
        }
        if input.tasks.len() > super::execution_modes::EXECUTION_MAX_TASKS {
            return Err(ProductionRequestError::InvalidField("tasks"));
        }
        let workspace_root = dunce::canonicalize(&input.workspace_root).map_err(|_| {
            ProductionRequestError::InvalidToolPolicy(SubagentToolError::InvalidWorkspace)
        })?;
        if input.approved_tool_policy.workspace_root() != Some(workspace_root.as_path()) {
            return Err(ProductionRequestError::WorkspaceMismatch);
        }
        for task in &input.tasks {
            if task.task_id.trim().is_empty() || task.task_id.len() > SUBAGENT_MAX_TASK_ID_BYTES {
                return Err(ProductionRequestError::InvalidField("task_id"));
            }
            if task.instruction.trim().is_empty()
                || task.instruction.len() > super::live_coordinator_types::MAX_INSTRUCTION_BYTES
            {
                return Err(ProductionRequestError::InvalidField("task_instruction"));
            }
            if task.context.as_ref().is_some_and(|context| {
                context.len() > super::live_coordinator_types::MAX_CONTEXT_BYTES
            }) {
                return Err(ProductionRequestError::InvalidField("task_context"));
            }
            if task.tool_policy.workspace_root() != Some(workspace_root.as_path()) {
                return Err(ProductionRequestError::WorkspaceMismatch);
            }
        }

        let profile = self
            .profiles
            .get(&self.routing_profile_id)
            .ok_or(RoutingProfileError::UnknownProfile)?;
        self.policy
            .validate_routing_profile(profile, &self.connections)?;
        for role in [
            RoutingRole::Main,
            RoutingRole::Planner,
            RoutingRole::Executor,
            RoutingRole::Verifier,
            RoutingRole::Repair,
        ] {
            if self.policy.role(role).activation == super::execution_modes::RoleActivation::Disabled
            {
                continue;
            }
            let assignment = profile
                .assignments
                .get(&role)
                .ok_or(RoutingProfileError::MissingRoleAssignment)?;
            if assignment.pool_id.is_none() {
                self.connections.validate_assignment(assignment)?;
            }
        }

        Ok(LiveOrchestrationRequest {
            run_id: input.run_id,
            policy: Some(self.policy.clone()),
            routing_profile_id: Some(self.routing_profile_id.clone()),
            instruction: input.instruction,
            context: input.context,
            tasks: input.tasks,
            planning: input.planning,
            verification: input.verification,
            failure_policy: input.failure_policy,
            repair_instruction: input.repair_instruction,
            approved_tool_policy: input.approved_tool_policy,
            cancellation: input.cancellation,
            overall_timeout: input.overall_timeout,
        })
    }

    pub fn policy(&self) -> &ResolvedExecutionPolicy {
        &self.policy
    }

    pub fn routing_profile_id(&self) -> &RoutingProfileId {
        &self.routing_profile_id
    }

    pub fn provider_selection(
        &self,
        role: RoutingRole,
    ) -> Result<super::omniroute::ProviderSelection, ProductionRequestError> {
        let profile = self
            .profiles
            .get(&self.routing_profile_id)
            .ok_or(RoutingProfileError::UnknownProfile)?;
        let assignment = profile
            .assignments
            .get(&role)
            .ok_or(RoutingProfileError::MissingRoleAssignment)?;
        if assignment.pool_id.is_none() {
            self.connections.validate_assignment(assignment)?;
        }
        let connection_id = assignment.pool_id.as_ref().map_or_else(
            || assignment.connection_id.clone(),
            |pool_id| format!("pool-{pool_id}"),
        );
        super::omniroute::ProviderSelection::new(
            &connection_id,
            &assignment.provider_id,
            &assignment.model_id,
        )
        .map_err(|_| RoutingProfileError::InvalidAssignment.into())
    }

    pub fn provider_route(
        &self,
        role: RoutingRole,
    ) -> Result<ProductionProviderRoute, ProductionRequestError> {
        Ok(ProductionProviderRoute {
            selection: self.provider_selection(role)?,
            effort: self.policy.role(role).effort.clone(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductionProviderRoute {
    selection: super::omniroute::ProviderSelection,
    effort: ReasoningEffort,
}

impl ProductionProviderRoute {
    pub fn new(selection: super::omniroute::ProviderSelection, effort: ReasoningEffort) -> Self {
        Self { selection, effort }
    }

    pub fn selection(&self) -> &super::omniroute::ProviderSelection {
        &self.selection
    }

    pub fn effort(&self) -> ReasoningEffort {
        self.effort.clone()
    }
}

/// Binds one exact resolved route to an existing provider invocation implementation.
///
/// Authentication, transport, output bounds, and typed failures remain owned by the wrapped
/// provider. This adapter only enforces that the request matches the captured route.
#[derive(Clone, Debug)]
pub struct ProductionProviderAdapter<P> {
    route: ProductionProviderRoute,
    provider: P,
}

impl<P> ProductionProviderAdapter<P> {
    pub fn new(
        route: ProductionProviderRoute,
        provider: P,
    ) -> Result<Self, ProviderInvocationError> {
        let selection = super::omniroute::ProviderSelection::new(
            route.selection.connection_id.clone(),
            route.selection.provider_id.clone(),
            route.selection.model_id.clone(),
        )
        .map_err(|_| ProviderInvocationError::InvalidRequest)?;
        Ok(Self {
            route: ProductionProviderRoute {
                selection,
                effort: route.effort,
            },
            provider,
        })
    }

    pub fn selection(&self) -> &super::omniroute::ProviderSelection {
        &self.route.selection
    }

    pub fn effort(&self) -> ReasoningEffort {
        self.route.effort.clone()
    }
}

impl<P: ProviderInvocation> SubagentProvider for ProductionProviderAdapter<P> {
    fn invoke(
        &self,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> impl std::future::Future<Output = Result<ProviderInvocationResult, ProviderInvocationError>>
    + Send {
        async move {
            if request.provider != self.route.selection.provider_id
                || request.model != self.route.selection.model_id
            {
                return Err(ProviderInvocationError::InvalidRequest);
            }
            self.provider.invoke(request, cancellation).await
        }
    }
}
