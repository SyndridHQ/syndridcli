use super::ExplicitRoleCapability;
use super::RoleCapabilityApproval;
use super::RoleCapabilityConfiguration;
use super::RoleCapabilityDeclaration;
use super::RoleCapabilityPermission;
use super::RoleCapabilityState;
use super::RoleCapabilityValidationContext;
use super::RoleCapabilityValidationError;
use super::SubagentToolKind;
use super::validate_role_capabilities;
use crate::ExecutionModeSelection;
use crate::RoutingRole;
use pretty_assertions::assert_eq;
use std::path::PathBuf;

fn policy() -> super::ResolvedExecutionPolicy {
    ExecutionModeSelection::Balanced
        .resolve()
        .expect("balanced policy")
}

fn workspace_root() -> PathBuf {
    std::env::current_dir().expect("workspace")
}

fn context() -> RoleCapabilityValidationContext {
    RoleCapabilityValidationContext::new(
        workspace_root(),
        [
            SubagentToolKind::ReadFile,
            SubagentToolKind::SearchText,
            SubagentToolKind::GitStatus,
        ]
        .into_iter()
        .collect(),
        false,
        false,
    )
}

fn no_tool_declarations() -> Vec<RoleCapabilityDeclaration> {
    [
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ]
    .into_iter()
    .map(RoleCapabilityDeclaration::no_tools)
    .collect()
}

fn explicit(role: RoutingRole) -> RoleCapabilityDeclaration {
    RoleCapabilityDeclaration::explicit(
        role,
        ExplicitRoleCapability::new(
            vec!["read_file".to_string()],
            Some(workspace_root()),
            RoleCapabilityPermission::Prohibited,
            RoleCapabilityPermission::Prohibited,
            1024,
            1,
            RoleCapabilityApproval::AlreadyAuthorized,
        ),
    )
}

#[test]
fn explicit_roles_are_validated_and_kept_isolated() {
    let configuration = RoleCapabilityConfiguration::new(
        [
            RoutingRole::Planner,
            RoutingRole::Executor,
            RoutingRole::Verifier,
            RoutingRole::Repair,
        ]
        .into_iter()
        .map(explicit)
        .collect(),
    );
    let capabilities = validate_role_capabilities(&configuration, &policy(), &context())
        .expect("explicit capabilities");

    assert_eq!(
        capabilities
            .roles()
            .map(|capability| capability.role())
            .collect::<Vec<_>>(),
        vec![
            RoutingRole::Planner,
            RoutingRole::Executor,
            RoutingRole::Verifier,
            RoutingRole::Repair,
        ]
    );
    assert_eq!(
        capabilities
            .get(RoutingRole::Planner)
            .unwrap()
            .max_tool_calls(),
        1
    );
    assert_eq!(
        capabilities.get(RoutingRole::Executor).unwrap().shell(),
        RoleCapabilityPermission::Prohibited
    );
    assert!(format!("{capabilities:?}").contains("<redacted>"));
}

#[test]
fn no_tools_is_valid_but_missing_is_not() {
    let capabilities = validate_role_capabilities(
        &RoleCapabilityConfiguration::new(no_tool_declarations()),
        &policy(),
        &context(),
    )
    .expect("explicit no-tool declarations");
    assert_eq!(
        capabilities
            .get(RoutingRole::Repair)
            .unwrap()
            .max_tool_calls(),
        0
    );

    let missing = RoleCapabilityConfiguration::new(vec![RoleCapabilityDeclaration::missing(
        RoutingRole::Planner,
    )]);
    assert_eq!(
        validate_role_capabilities(&missing, &policy(), &context()),
        Err(RoleCapabilityValidationError::MissingRole)
    );
}

#[test]
fn invalid_tools_permissions_and_bounds_are_rejected() {
    let mut duplicate = no_tool_declarations();
    duplicate.push(explicit(RoutingRole::Planner));
    assert_eq!(
        validate_role_capabilities(
            &RoleCapabilityConfiguration::new(duplicate),
            &policy(),
            &context(),
        ),
        Err(RoleCapabilityValidationError::DuplicateRole)
    );

    let unknown = RoleCapabilityDeclaration::explicit(
        RoutingRole::Planner,
        ExplicitRoleCapability::new(
            vec!["unknown".to_string()],
            Some(workspace_root()),
            RoleCapabilityPermission::Prohibited,
            RoleCapabilityPermission::Prohibited,
            1024,
            1,
            RoleCapabilityApproval::AlreadyAuthorized,
        ),
    );
    let mut declarations = no_tool_declarations();
    declarations[0] = unknown;
    assert_eq!(
        validate_role_capabilities(
            &RoleCapabilityConfiguration::new(declarations),
            &policy(),
            &context(),
        ),
        Err(RoleCapabilityValidationError::UnknownTool)
    );

    let mut duplicate_tool = no_tool_declarations();
    duplicate_tool[0] = RoleCapabilityDeclaration::explicit(
        RoutingRole::Planner,
        ExplicitRoleCapability::new(
            vec!["read_file".to_string(), "read_file".to_string()],
            Some(workspace_root()),
            RoleCapabilityPermission::Prohibited,
            RoleCapabilityPermission::Prohibited,
            1024,
            1,
            RoleCapabilityApproval::AlreadyAuthorized,
        ),
    );
    assert_eq!(
        validate_role_capabilities(
            &RoleCapabilityConfiguration::new(duplicate_tool),
            &policy(),
            &context()
        ),
        Err(RoleCapabilityValidationError::DuplicateTool)
    );

    let mut shell = no_tool_declarations();
    shell[0] = RoleCapabilityDeclaration::explicit(
        RoutingRole::Planner,
        ExplicitRoleCapability::new(
            vec!["read_file".to_string()],
            Some(workspace_root()),
            RoleCapabilityPermission::SessionBound,
            RoleCapabilityPermission::Prohibited,
            1024,
            1,
            RoleCapabilityApproval::AlreadyAuthorized,
        ),
    );
    assert_eq!(
        validate_role_capabilities(
            &RoleCapabilityConfiguration::new(shell),
            &policy(),
            &context()
        ),
        Err(RoleCapabilityValidationError::ShellCapabilityExceedsSession)
    );

    let mut approval = no_tool_declarations();
    approval[0].state = RoleCapabilityState::Explicit(ExplicitRoleCapability::new(
        vec!["read_file".to_string()],
        Some(workspace_root()),
        RoleCapabilityPermission::Prohibited,
        RoleCapabilityPermission::Prohibited,
        1024,
        1,
        RoleCapabilityApproval::InteractiveRequired,
    ));
    assert_eq!(
        validate_role_capabilities(
            &RoleCapabilityConfiguration::new(approval),
            &policy(),
            &context()
        ),
        Err(RoleCapabilityValidationError::ApprovalUnavailable)
    );
}
