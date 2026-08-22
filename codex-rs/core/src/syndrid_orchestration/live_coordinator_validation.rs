use super::ResolvedExecutionPolicy;
use super::RoutingConnectionDirectory;
use super::RoutingProfile;
use super::RoutingProfileRegistry;
use super::RoutingRole;
use super::live_coordinator_types::*;
use std::collections::HashSet;

pub(super) fn begin_state(
    state: &super::SessionExecutionPolicyState,
) -> Result<u64, super::SessionExecutionStateError> {
    state.begin_run()
}

pub(super) fn validate_request(
    request: &LiveOrchestrationRequest,
    policy: &ResolvedExecutionPolicy,
) -> Result<(), LiveOrchestrationError> {
    if request.run_id.is_empty()
        || request.run_id.len() > MAX_RUN_ID_BYTES
        || request.instruction.is_empty()
        || request.instruction.len() > MAX_INSTRUCTION_BYTES
        || request
            .context
            .as_ref()
            .is_some_and(|value| value.len() > MAX_CONTEXT_BYTES)
    {
        return Err(LiveOrchestrationError::InvalidRequest);
    }
    let mut task_ids = HashSet::with_capacity(request.tasks.len());
    if request.tasks.iter().any(|task| {
        task.task_id.is_empty()
            || task.instruction.is_empty()
            || task
                .context
                .as_ref()
                .is_some_and(|value| value.len() > MAX_CONTEXT_BYTES)
            || !task_ids.insert(task.task_id.as_str())
    }) {
        return Err(LiveOrchestrationError::InvalidTaskIdentifiers);
    }
    if request.tasks.len() > policy.policy().max_subagents {
        return Err(LiveOrchestrationError::ExecutorTasksExceedPolicyCeiling);
    }
    if matches!(request.planning, PlanningContract::Required { .. })
        && policy.role(RoutingRole::Planner).activation == super::RoleActivation::Disabled
    {
        return Err(LiveOrchestrationError::PlanningRequiredButDisabled);
    }
    Ok(())
}

pub(super) fn validate_routing(
    policy: &ResolvedExecutionPolicy,
    profile: &RoutingProfile,
    connections: &RoutingConnectionDirectory,
    verification: &VerificationContract,
    planning: &PlanningContract,
) -> Result<(), LiveOrchestrationError> {
    if !profile.enabled {
        return Err(LiveOrchestrationError::MissingRoutingProfile);
    }
    let mut roles = vec![RoutingRole::Main, RoutingRole::Executor];
    if matches!(planning, PlanningContract::Required { .. }) {
        roles.push(RoutingRole::Planner);
    }
    if matches!(
        verification,
        VerificationContract::Provider { .. }
            | VerificationContract::Decision(VerificationDecision::Rejected { .. })
    ) {
        roles.push(RoutingRole::Verifier);
    }
    let repair_required = matches!(
        verification,
        VerificationContract::Decision(VerificationDecision::Rejected { .. })
    ) && policy.role(RoutingRole::Repair).activation
        != super::RoleActivation::Disabled
        || policy.role(RoutingRole::Repair).activation == super::RoleActivation::Required
            && !matches!(verification, VerificationContract::NotRequested);
    if repair_required {
        roles.push(RoutingRole::Repair);
    }
    roles.sort_unstable();
    roles.dedup();
    for role in roles {
        let role_policy = policy.role(role);
        if role_policy.activation == super::RoleActivation::Disabled {
            return Err(LiveOrchestrationError::DisabledRequiredRole(role));
        }
        let assignment = profile
            .assignments
            .get(&role)
            .ok_or(LiveOrchestrationError::MissingRequiredRoleRoute(role))?;
        if !assignment.enabled {
            return Err(LiveOrchestrationError::DisabledRequiredRole(role));
        }
        if assignment.pool_id.is_none() {
            connections
                .validate_assignment(assignment)
                .map_err(|_| LiveOrchestrationError::InvalidProviderConnection(role))?;
        }
    }
    Ok(())
}

pub(super) fn selected_registry(
    registry: &RoutingProfileRegistry,
    profile: &RoutingProfile,
) -> RoutingProfileRegistry {
    let mut selected = registry.clone();
    selected.active_profile_id = Some(profile.id.clone());
    selected
}
