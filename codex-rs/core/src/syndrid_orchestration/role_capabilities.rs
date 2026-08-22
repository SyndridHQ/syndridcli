use super::ResolvedExecutionPolicy;
use super::routing_profiles::RoutingRole;
use super::subagent_tools::SubagentSessionBudget;
use super::subagent_tools::SubagentToolKind;
use super::subagent_tools::SubagentToolPolicy;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;

const REQUIRED_ROLES: [RoutingRole; 4] = [
    RoutingRole::Planner,
    RoutingRole::Executor,
    RoutingRole::Verifier,
    RoutingRole::Repair,
];

/// Describes whether a role may use a session-wide permission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoleCapabilityPermission {
    Prohibited,
    SessionBound,
}

/// Describes whether a role capability is already approved or needs interaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoleCapabilityApproval {
    AlreadyAuthorized,
    InteractiveRequired,
}

/// Explicit, product-owned capability data for one orchestration role.
#[derive(Clone, Eq, PartialEq)]
pub struct ExplicitRoleCapability {
    pub tool_names: Vec<String>,
    pub workspace_root: Option<PathBuf>,
    pub shell: RoleCapabilityPermission,
    pub network: RoleCapabilityPermission,
    pub max_output_bytes: usize,
    pub max_tool_calls: usize,
    pub approval: RoleCapabilityApproval,
}

impl fmt::Debug for ExplicitRoleCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExplicitRoleCapability")
            .field("tool_count", &self.tool_names.len())
            .field("workspace_root", &"<redacted>")
            .field("shell", &self.shell)
            .field("network", &self.network)
            .field("approval", &self.approval)
            .field("max_output_bytes", &self.max_output_bytes)
            .field("max_tool_calls", &self.max_tool_calls)
            .finish()
    }
}

impl ExplicitRoleCapability {
    pub fn new(
        tool_names: Vec<String>,
        workspace_root: Option<PathBuf>,
        shell: RoleCapabilityPermission,
        network: RoleCapabilityPermission,
        max_output_bytes: usize,
        max_tool_calls: usize,
        approval: RoleCapabilityApproval,
    ) -> Self {
        Self {
            tool_names,
            workspace_root,
            shell,
            network,
            max_output_bytes,
            max_tool_calls,
            approval,
        }
    }
}

/// The explicit state for one role. Missing and no-tools are intentionally distinct.
#[derive(Clone, Eq, PartialEq)]
pub enum RoleCapabilityState {
    Missing,
    NoTools,
    Explicit(ExplicitRoleCapability),
}

impl fmt::Debug for RoleCapabilityState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => formatter.write_str("Missing"),
            Self::NoTools => formatter.write_str("ExplicitNoTools"),
            Self::Explicit(_) => formatter.write_str("ExplicitCapabilities(<redacted>)"),
        }
    }
}

/// One role declaration supplied by a trusted local product configuration.
#[derive(Clone, Eq, PartialEq)]
pub struct RoleCapabilityDeclaration {
    pub role: RoutingRole,
    pub state: RoleCapabilityState,
}

impl fmt::Debug for RoleCapabilityDeclaration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RoleCapabilityDeclaration")
            .field("role", &self.role)
            .field("state", &self.state)
            .finish()
    }
}

impl RoleCapabilityDeclaration {
    pub fn missing(role: RoutingRole) -> Self {
        Self {
            role,
            state: RoleCapabilityState::Missing,
        }
    }

    pub fn no_tools(role: RoutingRole) -> Self {
        Self {
            role,
            state: RoleCapabilityState::NoTools,
        }
    }

    pub fn explicit(role: RoutingRole, capability: ExplicitRoleCapability) -> Self {
        Self {
            role,
            state: RoleCapabilityState::Explicit(capability),
        }
    }
}

/// A deterministic collection of explicit role declarations.
#[derive(Clone, Eq, PartialEq)]
pub struct RoleCapabilityConfiguration {
    declarations: Vec<RoleCapabilityDeclaration>,
}

impl fmt::Debug for RoleCapabilityConfiguration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RoleCapabilityConfiguration")
            .field("role_count", &self.declarations.len())
            .field("declarations", &"<redacted>")
            .finish()
    }
}

impl RoleCapabilityConfiguration {
    pub fn new(declarations: Vec<RoleCapabilityDeclaration>) -> Self {
        Self { declarations }
    }

    pub fn declarations(&self) -> &[RoleCapabilityDeclaration] {
        &self.declarations
    }
}

/// Session-wide upper bounds used while validating role declarations.
#[derive(Clone, Eq, PartialEq)]
pub struct RoleCapabilityValidationContext {
    workspace_root: PathBuf,
    available_tools: BTreeSet<SubagentToolKind>,
    shell_allowed: bool,
    network_allowed: bool,
}

impl fmt::Debug for RoleCapabilityValidationContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RoleCapabilityValidationContext")
            .field("workspace_root", &"<redacted>")
            .field("available_tool_count", &self.available_tools.len())
            .field("shell_allowed", &self.shell_allowed)
            .field("network_allowed", &self.network_allowed)
            .finish()
    }
}

impl RoleCapabilityValidationContext {
    pub fn new(
        workspace_root: PathBuf,
        available_tools: BTreeSet<SubagentToolKind>,
        shell_allowed: bool,
        network_allowed: bool,
    ) -> Self {
        Self {
            workspace_root,
            available_tools,
            shell_allowed,
            network_allowed,
        }
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }
}

/// Validated role capability data that can be passed to a future tool adapter.
#[derive(Clone, Eq, PartialEq)]
pub struct ValidatedRoleCapability {
    role: RoutingRole,
    tool_policy: SubagentToolPolicy,
    shell: RoleCapabilityPermission,
    network: RoleCapabilityPermission,
    approval: RoleCapabilityApproval,
    max_output_bytes: usize,
    max_tool_calls: usize,
}

impl fmt::Debug for ValidatedRoleCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ValidatedRoleCapability")
            .field("role", &self.role)
            .field("tool_policy", &"<redacted>")
            .field("shell", &self.shell)
            .field("network", &self.network)
            .field("approval", &self.approval)
            .field("max_output_bytes", &self.max_output_bytes)
            .field("max_tool_calls", &self.max_tool_calls)
            .finish()
    }
}

impl ValidatedRoleCapability {
    pub fn role(&self) -> RoutingRole {
        self.role
    }

    pub fn tool_policy(&self) -> &SubagentToolPolicy {
        &self.tool_policy
    }

    pub fn shell(&self) -> RoleCapabilityPermission {
        self.shell
    }

    pub fn network(&self) -> RoleCapabilityPermission {
        self.network
    }

    pub fn approval(&self) -> RoleCapabilityApproval {
        self.approval
    }

    pub fn max_output_bytes(&self) -> usize {
        self.max_output_bytes
    }

    pub fn max_tool_calls(&self) -> usize {
        self.max_tool_calls
    }
}

/// Immutable, validated role capabilities in deterministic role order.
#[derive(Clone, Eq, PartialEq)]
pub struct ValidatedRoleCapabilitySet {
    roles: BTreeMap<RoutingRole, ValidatedRoleCapability>,
}

impl fmt::Debug for ValidatedRoleCapabilitySet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ValidatedRoleCapabilitySet")
            .field("role_count", &self.roles.len())
            .field("roles", &self.roles.keys().collect::<Vec<_>>())
            .field("capabilities", &"<redacted>")
            .finish()
    }
}

impl ValidatedRoleCapabilitySet {
    pub fn get(&self, role: RoutingRole) -> Option<&ValidatedRoleCapability> {
        self.roles.get(&role)
    }

    pub fn roles(&self) -> impl Iterator<Item = &ValidatedRoleCapability> {
        self.roles.values()
    }
}

/// Errors produced by the single core role-capability validation path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RoleCapabilityValidationError {
    #[error("role capability declaration is missing")]
    MissingRole,
    #[error("role capability declaration is duplicated")]
    DuplicateRole,
    #[error("role capability declaration is invalid")]
    Invalid,
    #[error("role capability contains an unknown tool")]
    UnknownTool,
    #[error("role capability contains a duplicate tool")]
    DuplicateTool,
    #[error("role capability tool is unavailable in the session")]
    ToolUnavailable,
    #[error("role capability workspace does not match the session")]
    WorkspaceMismatch,
    #[error("role shell capability exceeds the session authority")]
    ShellCapabilityExceedsSession,
    #[error("role network capability exceeds the session authority")]
    NetworkCapabilityExceedsSession,
    #[error("role capability output bound is invalid")]
    OutputBoundInvalid,
    #[error("role capability tool budget is invalid")]
    ToolBudgetInvalid,
    #[error("role capability requires interactive approval")]
    ApprovalUnavailable,
    #[error("explicit no-tool capability is inconsistent")]
    IntentionalNoToolsConflict,
}

/// Validates explicit role capabilities against one resolved policy and session authority.
pub fn validate_role_capabilities(
    configuration: &RoleCapabilityConfiguration,
    policy: &ResolvedExecutionPolicy,
    context: &RoleCapabilityValidationContext,
) -> Result<ValidatedRoleCapabilitySet, RoleCapabilityValidationError> {
    if !context.workspace_root.is_absolute() {
        return Err(RoleCapabilityValidationError::WorkspaceMismatch);
    }

    let mut declarations = BTreeMap::new();
    for declaration in configuration.declarations() {
        if declarations.insert(declaration.role, declaration).is_some() {
            return Err(RoleCapabilityValidationError::DuplicateRole);
        }
    }

    for role in REQUIRED_ROLES {
        if !declarations.contains_key(&role) {
            return Err(RoleCapabilityValidationError::MissingRole);
        }
    }

    let mut roles = BTreeMap::new();
    for (role, declaration) in declarations {
        let capability = match &declaration.state {
            RoleCapabilityState::Missing => {
                return Err(RoleCapabilityValidationError::MissingRole);
            }
            RoleCapabilityState::NoTools => validated_no_tools(role),
            RoleCapabilityState::Explicit(spec) => {
                validate_explicit(declaration.role, spec, policy, context)?
            }
        };
        roles.insert(role, capability);
    }
    Ok(ValidatedRoleCapabilitySet { roles })
}

fn validated_no_tools(role: RoutingRole) -> ValidatedRoleCapability {
    let budget = SubagentSessionBudget {
        max_tool_calls: 0,
        max_tool_output_bytes: 0,
        max_aggregate_tool_output_bytes: 0,
        ..Default::default()
    };
    ValidatedRoleCapability {
        role,
        tool_policy: SubagentToolPolicy::from_parts(BTreeSet::new(), None, budget),
        shell: RoleCapabilityPermission::Prohibited,
        network: RoleCapabilityPermission::Prohibited,
        approval: RoleCapabilityApproval::AlreadyAuthorized,
        max_output_bytes: 0,
        max_tool_calls: 0,
    }
}

fn validate_explicit(
    role: RoutingRole,
    spec: &ExplicitRoleCapability,
    policy: &ResolvedExecutionPolicy,
    context: &RoleCapabilityValidationContext,
) -> Result<ValidatedRoleCapability, RoleCapabilityValidationError> {
    if spec.tool_names.is_empty() {
        return Err(RoleCapabilityValidationError::IntentionalNoToolsConflict);
    }
    if spec.max_output_bytes == 0 || spec.max_output_bytes > policy.policy().max_tool_output_bytes {
        return Err(RoleCapabilityValidationError::OutputBoundInvalid);
    }
    if spec.max_tool_calls == 0 || spec.max_tool_calls > policy.policy().max_tool_calls {
        return Err(RoleCapabilityValidationError::ToolBudgetInvalid);
    }
    if spec.shell == RoleCapabilityPermission::SessionBound && !context.shell_allowed {
        return Err(RoleCapabilityValidationError::ShellCapabilityExceedsSession);
    }
    if spec.network == RoleCapabilityPermission::SessionBound && !context.network_allowed {
        return Err(RoleCapabilityValidationError::NetworkCapabilityExceedsSession);
    }
    if spec.approval == RoleCapabilityApproval::InteractiveRequired {
        return Err(RoleCapabilityValidationError::ApprovalUnavailable);
    }
    let workspace_root = spec
        .workspace_root
        .as_deref()
        .ok_or(RoleCapabilityValidationError::WorkspaceMismatch)?;
    if workspace_root != context.workspace_root {
        return Err(RoleCapabilityValidationError::WorkspaceMismatch);
    }

    let mut tools = BTreeSet::new();
    for name in &spec.tool_names {
        let tool = SubagentToolKind::from_provider_name(name)
            .ok_or(RoleCapabilityValidationError::UnknownTool)?;
        if !tools.insert(tool) {
            return Err(RoleCapabilityValidationError::DuplicateTool);
        }
        if !context.available_tools.contains(&tool) {
            return Err(RoleCapabilityValidationError::ToolUnavailable);
        }
    }

    let mut budget = SubagentSessionBudget::default();
    budget.max_provider_turns = budget
        .max_provider_turns
        .min(policy.policy().max_provider_invocations);
    budget.max_tool_calls = spec.max_tool_calls;
    budget.max_tool_output_bytes = spec.max_output_bytes;
    budget.max_aggregate_tool_output_bytes = budget
        .max_aggregate_tool_output_bytes
        .min(spec.max_output_bytes.saturating_mul(spec.max_tool_calls));
    Ok(ValidatedRoleCapability {
        role,
        tool_policy: SubagentToolPolicy::from_parts(
            tools,
            Some(workspace_root.to_path_buf()),
            budget,
        ),
        shell: spec.shell,
        network: spec.network,
        approval: spec.approval,
        max_output_bytes: spec.max_output_bytes,
        max_tool_calls: spec.max_tool_calls,
    })
}

#[cfg(test)]
#[path = "role_capabilities_tests.rs"]
mod tests;
