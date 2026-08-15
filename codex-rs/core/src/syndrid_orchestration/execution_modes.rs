use super::routing_profiles::RoutingConnectionDirectory;
use super::routing_profiles::RoutingProfile;
use super::routing_profiles::RoutingProfileError;
use super::routing_profiles::RoutingProfileRegistry;
use super::routing_profiles::RoutingRole;
use super::subagent::SUBAGENT_MAX_TIMEOUT;
use super::subagent::SUBAGENT_MIN_TIMEOUT;
use super::subagent_batch::SUBAGENT_BATCH_MAX_CONCURRENCY;
use super::subagent_batch::SubagentConcurrencyPolicy;
use super::subagent_batch::SubagentFailurePolicy;
use super::subagent_batch::SubagentResultOrdering;
use super::subagent_repair::SUBAGENT_REPAIR_MAX_ATTEMPTS;
use super::subagent_repair::SUBAGENT_REPAIR_MAX_CONTEXT_BYTES;
use super::subagent_repair::SUBAGENT_REPAIR_MAX_OUTPUT_TOKENS;
use super::subagent_repair::SubagentRepairPolicy;
use super::subagent_repair::SubagentRepairRoute;
use codex_protocol::openai_models::ReasoningEffort;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

pub const EXECUTION_MAX_TASKS: usize = 8;
pub const EXECUTION_MAX_PROVIDER_INVOCATIONS: usize = 64;
pub const EXECUTION_MAX_TOOL_CALLS: usize = 128;
pub const EXECUTION_MAX_TOOL_OUTPUT_BYTES: usize = 1024 * 1024;
pub const EXECUTION_MAX_CONTEXT_BYTES: usize = SUBAGENT_REPAIR_MAX_CONTEXT_BYTES;
pub const EXECUTION_MAX_OUTPUT_TOKENS: u32 = SUBAGENT_REPAIR_MAX_OUTPUT_TOKENS;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltInExecutionMode {
    Fast,
    Balanced,
    UsageSaver,
    Deep,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionModeSelection {
    Fast,
    #[default]
    Balanced,
    UsageSaver,
    Deep,
    Custom(ExecutionPolicy),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleActivation {
    Disabled,
    Optional,
    Required,
}

impl RoleActivation {
    fn enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoleExecutionPolicy {
    pub activation: RoleActivation,
    pub effort: ReasoningEffort,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionShape {
    SinglePass,
    BoundedVerificationRepair,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPolicy {
    pub roles: BTreeMap<RoutingRole, RoleExecutionPolicy>,
    pub max_subagents: usize,
    pub max_concurrency: usize,
    pub max_provider_invocations: usize,
    pub max_tool_calls: usize,
    pub max_tool_output_bytes: usize,
    pub max_repair_attempts: u8,
    pub task_timeout: Duration,
    pub batch_timeout: Duration,
    pub repair_timeout: Duration,
    pub context_budget_bytes: usize,
    pub output_budget_tokens: u32,
    pub max_final_response_tokens: u32,
    pub optional_roles_may_skip: bool,
    pub shape: ExecutionShape,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedExecutionPolicy {
    selected_mode: ExecutionModeSelection,
    source: PolicySource,
    policy: ExecutionPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicySource {
    BuiltIn(BuiltInExecutionMode),
    Custom,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedExecutionPolicyExplanation {
    pub selected_mode: ExecutionModeSelection,
    pub source: PolicySource,
    pub roles: Vec<(RoutingRole, RoleExecutionPolicy)>,
    pub max_subagents: usize,
    pub max_concurrency: usize,
    pub max_provider_invocations: usize,
    pub max_tool_calls: usize,
    pub max_tool_output_bytes: usize,
    pub max_repair_attempts: u8,
    pub task_timeout: Duration,
    pub batch_timeout: Duration,
    pub repair_timeout: Duration,
    pub context_budget_bytes: usize,
    pub output_budget_tokens: u32,
    pub max_final_response_tokens: u32,
    pub optional_roles_may_skip: bool,
    pub shape: ExecutionShape,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionPolicyError {
    InvalidPolicy,
    PolicyExceedsHardCeiling,
    ContradictoryRoleSettings(RoutingRole),
    InvalidConcurrency,
    InvalidTimeout,
    InsufficientProviderBudget,
    InsufficientToolBudget,
    InvalidRepairConfiguration,
    UnsupportedEffort(RoutingRole),
    MissingRequiredRoute(RoutingRole),
    DisabledRoute(RoutingRole),
    InvalidProviderConnection(RoutingRole),
    RepairRouteMismatch,
    UnknownMode,
    RoutingProfileInactive,
    RoutingProfileError(RoutingProfileError),
}

impl fmt::Display for ExecutionPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPolicy => formatter.write_str("execution policy is invalid"),
            Self::PolicyExceedsHardCeiling => {
                formatter.write_str("execution policy exceeds a hard ceiling")
            }
            Self::ContradictoryRoleSettings(role) => {
                write!(
                    formatter,
                    "execution policy has contradictory {role} role settings"
                )
            }
            Self::InvalidConcurrency => formatter.write_str("execution concurrency is invalid"),
            Self::InvalidTimeout => formatter.write_str("execution timeout is invalid"),
            Self::InsufficientProviderBudget => {
                formatter.write_str("provider invocation budget is insufficient")
            }
            Self::InsufficientToolBudget => formatter.write_str("tool-call budget is insufficient"),
            Self::InvalidRepairConfiguration => {
                formatter.write_str("repair configuration is invalid")
            }
            Self::UnsupportedEffort(role) => {
                write!(formatter, "effort is unsupported for {role} role")
            }
            Self::MissingRequiredRoute(role) => {
                write!(formatter, "required {role} route is missing")
            }
            Self::DisabledRoute(role) => write!(formatter, "required {role} route is disabled"),
            Self::InvalidProviderConnection(role) => {
                write!(formatter, "{role} route has an invalid provider connection")
            }
            Self::RepairRouteMismatch => {
                formatter.write_str("repair route does not match the profile")
            }
            Self::UnknownMode => formatter.write_str("execution mode is unsupported"),
            Self::RoutingProfileInactive => formatter.write_str("routing profile is inactive"),
            Self::RoutingProfileError(_) => formatter.write_str("routing profile is invalid"),
        }
    }
}

impl std::error::Error for ExecutionPolicyError {}

impl From<RoutingProfileError> for ExecutionPolicyError {
    fn from(error: RoutingProfileError) -> Self {
        Self::RoutingProfileError(error)
    }
}

impl ExecutionModeSelection {
    pub fn resolve(&self) -> Result<ResolvedExecutionPolicy, ExecutionPolicyError> {
        let (source, selected_mode, policy) = match self {
            Self::Fast => (
                PolicySource::BuiltIn(BuiltInExecutionMode::Fast),
                self.clone(),
                builtin(BuiltInExecutionMode::Fast),
            ),
            Self::Balanced => (
                PolicySource::BuiltIn(BuiltInExecutionMode::Balanced),
                self.clone(),
                builtin(BuiltInExecutionMode::Balanced),
            ),
            Self::UsageSaver => (
                PolicySource::BuiltIn(BuiltInExecutionMode::UsageSaver),
                self.clone(),
                builtin(BuiltInExecutionMode::UsageSaver),
            ),
            Self::Deep => (
                PolicySource::BuiltIn(BuiltInExecutionMode::Deep),
                self.clone(),
                builtin(BuiltInExecutionMode::Deep),
            ),
            Self::Custom(policy) => (PolicySource::Custom, self.clone(), policy.clone()),
        };
        validate(&policy)?;
        Ok(ResolvedExecutionPolicy {
            selected_mode,
            source,
            policy,
        })
    }

    pub fn custom(policy: ExecutionPolicy) -> Self {
        Self::Custom(policy)
    }

    pub fn parse(value: &str) -> Result<Self, ExecutionPolicyError> {
        match value {
            "fast" => Ok(Self::Fast),
            "balanced" => Ok(Self::Balanced),
            "usage_saver" => Ok(Self::UsageSaver),
            "deep" => Ok(Self::Deep),
            _ => Err(ExecutionPolicyError::UnknownMode),
        }
    }
}

impl ResolvedExecutionPolicy {
    pub fn selected_mode(&self) -> &ExecutionModeSelection {
        &self.selected_mode
    }

    pub fn source(&self) -> PolicySource {
        self.source
    }

    pub fn policy(&self) -> &ExecutionPolicy {
        &self.policy
    }

    pub fn role(&self, role: RoutingRole) -> &RoleExecutionPolicy {
        self.policy
            .roles
            .get(&role)
            .expect("validated execution policy contains every role")
    }

    pub fn explain(&self) -> ResolvedExecutionPolicyExplanation {
        ResolvedExecutionPolicyExplanation {
            selected_mode: self.selected_mode.clone(),
            source: self.source,
            roles: self
                .policy
                .roles
                .iter()
                .map(|(role, policy)| (*role, policy.clone()))
                .collect(),
            max_subagents: self.policy.max_subagents,
            max_concurrency: self.policy.max_concurrency,
            max_provider_invocations: self.policy.max_provider_invocations,
            max_tool_calls: self.policy.max_tool_calls,
            max_tool_output_bytes: self.policy.max_tool_output_bytes,
            max_repair_attempts: self.policy.max_repair_attempts,
            task_timeout: self.policy.task_timeout,
            batch_timeout: self.policy.batch_timeout,
            repair_timeout: self.policy.repair_timeout,
            context_budget_bytes: self.policy.context_budget_bytes,
            output_budget_tokens: self.policy.output_budget_tokens,
            max_final_response_tokens: self.policy.max_final_response_tokens,
            optional_roles_may_skip: self.policy.optional_roles_may_skip,
            shape: self.policy.shape,
        }
    }

    pub fn validate_routing_profile(
        &self,
        profile: &RoutingProfile,
        connections: &RoutingConnectionDirectory,
    ) -> Result<(), ExecutionPolicyError> {
        if !profile.enabled {
            return Err(ExecutionPolicyError::RoutingProfileInactive);
        }
        for role in roles() {
            let activation = self.role(role).activation;
            if !activation.enabled() {
                continue;
            }
            let assignment = profile
                .assignments
                .get(&role)
                .ok_or(ExecutionPolicyError::MissingRequiredRoute(role))?;
            if !assignment.enabled {
                return Err(ExecutionPolicyError::DisabledRoute(role));
            }
            if assignment.pool_id.is_none() {
                connections
                    .validate_assignment(assignment)
                    .map_err(|_| ExecutionPolicyError::InvalidProviderConnection(role))?;
            }
        }
        Ok(())
    }

    pub fn validate_active_routing(
        &self,
        registry: &RoutingProfileRegistry,
        connections: &RoutingConnectionDirectory,
    ) -> Result<(), ExecutionPolicyError> {
        self.validate_routing_profile(registry.active()?, connections)
    }

    pub fn validate_repair_route(
        &self,
        route: &SubagentRepairRoute,
        profile: &RoutingProfile,
        connections: &RoutingConnectionDirectory,
    ) -> Result<(), ExecutionPolicyError> {
        if !self.role(RoutingRole::Repair).activation.enabled()
            || route.role != RoutingRole::Repair
            || route.profile_id != profile.id.as_str()
        {
            return Err(ExecutionPolicyError::RepairRouteMismatch);
        }
        let assignment = profile.assignments.get(&RoutingRole::Repair).ok_or(
            ExecutionPolicyError::MissingRequiredRoute(RoutingRole::Repair),
        )?;
        if !assignment.enabled
            || assignment.connection_id != route.connection_id
            || assignment.provider_id != route.provider_id
            || assignment.model_id != route.model_id
        {
            return Err(ExecutionPolicyError::RepairRouteMismatch);
        }
        if assignment.pool_id.is_none() {
            connections.validate_assignment(assignment).map_err(|_| {
                ExecutionPolicyError::InvalidProviderConnection(RoutingRole::Repair)
            })?;
        }
        Ok(())
    }

    pub fn to_batch_policy(
        &self,
        failure_policy: SubagentFailurePolicy,
    ) -> SubagentConcurrencyPolicy {
        SubagentConcurrencyPolicy {
            max_tasks: self.policy.max_subagents,
            max_concurrency: self.policy.max_concurrency,
            batch_timeout: self.policy.batch_timeout,
            max_provider_turns: self.policy.max_provider_invocations,
            max_tool_calls: self.policy.max_tool_calls,
            max_tool_output_bytes: self.policy.max_tool_output_bytes,
            failure_policy,
            result_ordering: SubagentResultOrdering::InputOrder,
        }
    }

    pub fn repair_policy(
        &self,
        route: SubagentRepairRoute,
    ) -> Result<RepairPolicyDecision, ExecutionPolicyError> {
        let role = self.role(RoutingRole::Repair);
        match role.activation {
            RoleActivation::Disabled => {
                if self.policy.shape != ExecutionShape::SinglePass {
                    return Err(ExecutionPolicyError::InvalidRepairConfiguration);
                }
                Ok(RepairPolicyDecision::Disabled)
            }
            RoleActivation::Optional | RoleActivation::Required => {
                if self.policy.shape != ExecutionShape::BoundedVerificationRepair
                    || self.policy.repair_timeout.is_zero()
                {
                    return Err(ExecutionPolicyError::InvalidRepairConfiguration);
                }
                let policy = SubagentRepairPolicy {
                    enabled: true,
                    max_repair_attempts: self.policy.max_repair_attempts,
                    route,
                    per_repair_timeout: self.policy.repair_timeout,
                    total_repair_timeout: self.policy.repair_timeout,
                    max_provider_invocations: 1,
                    max_tool_calls: self.policy.max_tool_calls.min(1),
                    max_context_bytes: self.policy.context_budget_bytes,
                    max_output_tokens: self.policy.output_budget_tokens,
                };
                policy
                    .validate()
                    .map_err(|_| ExecutionPolicyError::InvalidRepairConfiguration)?;
                Ok(RepairPolicyDecision::Enabled(policy))
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepairPolicyDecision {
    Disabled,
    Enabled(SubagentRepairPolicy),
}

fn validate(policy: &ExecutionPolicy) -> Result<(), ExecutionPolicyError> {
    if policy.roles.len() != roles().len() {
        return Err(ExecutionPolicyError::InvalidPolicy);
    }
    for role in roles() {
        let role_policy = policy
            .roles
            .get(&role)
            .ok_or(ExecutionPolicyError::InvalidPolicy)?;
        if matches!(role_policy.effort, ReasoningEffort::Custom(_)) {
            return Err(ExecutionPolicyError::UnsupportedEffort(role));
        }
    }
    if policy.max_subagents == 0
        || policy.max_subagents > EXECUTION_MAX_TASKS
        || policy.max_concurrency == 0
        || policy.max_concurrency > policy.max_subagents
        || policy.max_concurrency > SUBAGENT_BATCH_MAX_CONCURRENCY
    {
        return Err(ExecutionPolicyError::InvalidConcurrency);
    }
    if policy.max_provider_invocations == 0
        || policy.max_provider_invocations > EXECUTION_MAX_PROVIDER_INVOCATIONS
        || policy.max_provider_invocations < policy.max_subagents
    {
        return Err(ExecutionPolicyError::InsufficientProviderBudget);
    }
    if policy.max_tool_calls == 0
        || policy.max_tool_calls > EXECUTION_MAX_TOOL_CALLS
        || policy.max_tool_output_bytes == 0
        || policy.max_tool_output_bytes > EXECUTION_MAX_TOOL_OUTPUT_BYTES
    {
        return Err(ExecutionPolicyError::InsufficientToolBudget);
    }
    if policy.task_timeout.is_zero()
        || policy.batch_timeout.is_zero()
        || policy.task_timeout > policy.batch_timeout
        || policy.task_timeout < SUBAGENT_MIN_TIMEOUT
        || policy.batch_timeout > SUBAGENT_MAX_TIMEOUT
    {
        return Err(ExecutionPolicyError::InvalidTimeout);
    }
    if policy.context_budget_bytes == 0
        || policy.context_budget_bytes > EXECUTION_MAX_CONTEXT_BYTES
        || policy.output_budget_tokens == 0
        || policy.output_budget_tokens > EXECUTION_MAX_OUTPUT_TOKENS
        || policy.max_final_response_tokens == 0
        || policy.max_final_response_tokens > EXECUTION_MAX_OUTPUT_TOKENS
    {
        return Err(ExecutionPolicyError::PolicyExceedsHardCeiling);
    }
    let repair = policy.roles[&RoutingRole::Repair].activation.enabled();
    if repair != (policy.shape == ExecutionShape::BoundedVerificationRepair)
        || policy.max_repair_attempts != u8::from(repair)
        || (!repair && !policy.repair_timeout.is_zero())
        || (repair
            && (policy.repair_timeout.is_zero()
                || policy.repair_timeout < SUBAGENT_MIN_TIMEOUT
                || policy.repair_timeout > policy.task_timeout
                || policy.repair_timeout > policy.batch_timeout))
    {
        return Err(ExecutionPolicyError::InvalidRepairConfiguration);
    }
    if policy.roles[&RoutingRole::Main].activation != RoleActivation::Required
        || policy.roles[&RoutingRole::Executor].activation == RoleActivation::Disabled
    {
        return Err(ExecutionPolicyError::ContradictoryRoleSettings(
            RoutingRole::Main,
        ));
    }
    if policy.max_repair_attempts > SUBAGENT_REPAIR_MAX_ATTEMPTS {
        return Err(ExecutionPolicyError::InvalidRepairConfiguration);
    }
    Ok(())
}

fn roles() -> [RoutingRole; 5] {
    [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ]
}

fn role(activation: RoleActivation, effort: ReasoningEffort) -> RoleExecutionPolicy {
    RoleExecutionPolicy { activation, effort }
}

fn policy(
    planner: RoleActivation,
    verifier: RoleActivation,
    repair: RoleActivation,
    effort: ReasoningEffort,
    max_subagents: usize,
    max_concurrency: usize,
    provider: usize,
    tools: usize,
    tool_output: usize,
    repair_attempts: u8,
    task_timeout: Duration,
    repair_timeout: Duration,
    context: usize,
    output: u32,
) -> ExecutionPolicy {
    let mut roles = BTreeMap::new();
    roles.insert(
        RoutingRole::Main,
        role(RoleActivation::Required, effort.clone()),
    );
    roles.insert(RoutingRole::Planner, role(planner, effort.clone()));
    roles.insert(
        RoutingRole::Executor,
        role(RoleActivation::Required, effort.clone()),
    );
    roles.insert(RoutingRole::Verifier, role(verifier, effort.clone()));
    roles.insert(RoutingRole::Repair, role(repair, effort));
    ExecutionPolicy {
        roles,
        max_subagents,
        max_concurrency,
        max_provider_invocations: provider,
        max_tool_calls: tools,
        max_tool_output_bytes: tool_output,
        max_repair_attempts: repair_attempts,
        task_timeout,
        batch_timeout: task_timeout,
        repair_timeout,
        context_budget_bytes: context,
        output_budget_tokens: output,
        max_final_response_tokens: output,
        optional_roles_may_skip: true,
        shape: if repair == RoleActivation::Disabled {
            ExecutionShape::SinglePass
        } else {
            ExecutionShape::BoundedVerificationRepair
        },
    }
}

fn builtin(mode: BuiltInExecutionMode) -> ExecutionPolicy {
    match mode {
        BuiltInExecutionMode::Fast => policy(
            RoleActivation::Disabled,
            RoleActivation::Disabled,
            RoleActivation::Disabled,
            ReasoningEffort::Low,
            1,
            1,
            1,
            4,
            32 * 1024,
            0,
            Duration::from_secs(30),
            Duration::ZERO,
            4 * 1024,
            1_000,
        ),
        BuiltInExecutionMode::Balanced => policy(
            RoleActivation::Optional,
            RoleActivation::Optional,
            RoleActivation::Optional,
            ReasoningEffort::Medium,
            2,
            2,
            8,
            16,
            128 * 1024,
            1,
            Duration::from_secs(120),
            Duration::from_secs(30),
            16 * 1024,
            4_000,
        ),
        BuiltInExecutionMode::UsageSaver => policy(
            RoleActivation::Disabled,
            RoleActivation::Disabled,
            RoleActivation::Disabled,
            ReasoningEffort::Low,
            1,
            1,
            1,
            1,
            32 * 1024,
            0,
            Duration::from_secs(20),
            Duration::ZERO,
            4 * 1024,
            1_000,
        ),
        BuiltInExecutionMode::Deep => policy(
            RoleActivation::Required,
            RoleActivation::Required,
            RoleActivation::Required,
            ReasoningEffort::High,
            EXECUTION_MAX_TASKS,
            SUBAGENT_BATCH_MAX_CONCURRENCY,
            EXECUTION_MAX_PROVIDER_INVOCATIONS,
            EXECUTION_MAX_TOOL_CALLS,
            EXECUTION_MAX_TOOL_OUTPUT_BYTES,
            1,
            SUBAGENT_MAX_TIMEOUT,
            Duration::from_secs(120),
            EXECUTION_MAX_CONTEXT_BYTES,
            EXECUTION_MAX_OUTPUT_TOKENS,
        ),
    }
}
