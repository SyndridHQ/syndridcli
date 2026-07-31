use super::ExplicitRoleCapability;
use super::ResolvedExecutionPolicy;
use super::RoleCapabilityApproval;
use super::RoleCapabilityConfiguration;
use super::RoleCapabilityDeclaration;
use super::RoleCapabilityPermission;
use super::RoleCapabilityState;
use super::RoleCapabilityValidationContext;
use super::RoutingRole;
use super::ValidatedRoleCapabilitySet;
use super::validate_role_capabilities;
use serde::Deserialize;
use std::fs::File;
use std::io::ErrorKind;
use std::io::Read;
use std::path::Path;

/// Canonical local file containing declarative Syndrid role capabilities.
pub const ROLE_CAPABILITY_FILE: &str = "syndrid-role-capabilities.json";

const CURRENT_SCHEMA_VERSION: u32 = 1;
const MAX_ROLE_CAPABILITY_FILE_BYTES: u64 = 128 * 1024;

/// Failure while loading or validating the persisted role-capability file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RoleCapabilityConfigError {
    #[error("role-capability configuration is unavailable")]
    Unavailable,
    #[error("role-capability configuration cannot be read")]
    Unreadable,
    #[error("role-capability configuration is oversized")]
    Oversized,
    #[error("role-capability configuration is malformed")]
    Malformed,
    #[error("role-capability configuration schema version is unsupported")]
    UnsupportedVersion,
    #[error("role-capability configuration is invalid")]
    Invalid,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedRoleCapabilityFile {
    schema_version: u32,
    planner: Option<PersistedRoleCapability>,
    executor: Option<PersistedRoleCapability>,
    verifier: Option<PersistedRoleCapability>,
    repair: Option<PersistedRoleCapability>,
}

#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum PersistedRoleCapability {
    NoTools,
    Explicit {
        tool_names: Vec<String>,
        workspace: PersistedWorkspaceScope,
        shell: PersistedPermission,
        network: PersistedPermission,
        max_output_bytes: usize,
        max_tool_calls: usize,
        approval: PersistedApproval,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum PersistedWorkspaceScope {
    Session,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum PersistedPermission {
    Prohibited,
    SessionBound,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum PersistedApproval {
    PreAuthorized,
}

/// Loads and validates role capabilities from the canonical local product root.
pub fn load_role_capabilities(
    codex_home: &Path,
    policy: &ResolvedExecutionPolicy,
    context: &RoleCapabilityValidationContext,
) -> Result<ValidatedRoleCapabilitySet, RoleCapabilityConfigError> {
    let bytes = read_bounded_file(&codex_home.join(ROLE_CAPABILITY_FILE))?;
    let persisted: PersistedRoleCapabilityFile =
        serde_json::from_slice(&bytes).map_err(|_| RoleCapabilityConfigError::Malformed)?;
    if persisted.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(RoleCapabilityConfigError::UnsupportedVersion);
    }

    let configuration = RoleCapabilityConfiguration::new(vec![
        declaration(RoutingRole::Planner, persisted.planner, context),
        declaration(RoutingRole::Executor, persisted.executor, context),
        declaration(RoutingRole::Verifier, persisted.verifier, context),
        declaration(RoutingRole::Repair, persisted.repair, context),
    ]);
    validate_role_capabilities(&configuration, policy, context)
        .map_err(|_| RoleCapabilityConfigError::Invalid)
}

fn declaration(
    role: RoutingRole,
    capability: Option<PersistedRoleCapability>,
    context: &RoleCapabilityValidationContext,
) -> RoleCapabilityDeclaration {
    let Some(capability) = capability else {
        return RoleCapabilityDeclaration::missing(role);
    };
    match capability {
        PersistedRoleCapability::NoTools => RoleCapabilityDeclaration {
            role,
            state: RoleCapabilityState::NoTools,
        },
        PersistedRoleCapability::Explicit {
            tool_names,
            workspace,
            shell,
            network,
            max_output_bytes,
            max_tool_calls,
            approval,
        } => RoleCapabilityDeclaration::explicit(
            role,
            ExplicitRoleCapability::new(
                tool_names,
                Some(match workspace {
                    PersistedWorkspaceScope::Session => context.workspace_root().to_path_buf(),
                }),
                permission(shell),
                permission(network),
                max_output_bytes,
                max_tool_calls,
                match approval {
                    PersistedApproval::PreAuthorized => RoleCapabilityApproval::AlreadyAuthorized,
                },
            ),
        ),
    }
}

fn permission(permission: PersistedPermission) -> RoleCapabilityPermission {
    match permission {
        PersistedPermission::Prohibited => RoleCapabilityPermission::Prohibited,
        PersistedPermission::SessionBound => RoleCapabilityPermission::SessionBound,
    }
}

fn read_bounded_file(path: &Path) -> Result<Vec<u8>, RoleCapabilityConfigError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(RoleCapabilityConfigError::Unavailable);
        }
        Err(_) => return Err(RoleCapabilityConfigError::Unreadable),
    };
    if file
        .metadata()
        .map_err(|_| RoleCapabilityConfigError::Unreadable)?
        .len()
        > MAX_ROLE_CAPABILITY_FILE_BYTES
    {
        return Err(RoleCapabilityConfigError::Oversized);
    }

    let mut bytes = Vec::new();
    file.take(MAX_ROLE_CAPABILITY_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| RoleCapabilityConfigError::Unreadable)?;
    if bytes.len() as u64 > MAX_ROLE_CAPABILITY_FILE_BYTES {
        return Err(RoleCapabilityConfigError::Oversized);
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "role_capability_config_tests.rs"]
mod tests;
