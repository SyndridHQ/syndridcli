use super::ResolvedExecutionPolicy;
use super::RoutingConnectionDirectory;
use super::RoutingProfileId;
use super::RoutingProfileRegistry;
use super::RoutingRole;
use super::SessionExecutionPolicyState;
use super::SessionExecutionStatus;
use super::SubagentProvider;
use super::SubagentStatus;
use super::SubagentTaskState;
use super::live_coordinator_mapping::*;
use super::live_coordinator_stages::*;
use super::live_coordinator_types::*;
use super::live_coordinator_validation::*;
use std::sync::Arc;

/// Executes one explicit bounded workflow using O6A–O6E runtimes.
pub struct LiveOrchestrationCoordinator<P> {
    pub(super) provider: Arc<P>,
    pub(super) profiles: RoutingProfileRegistry,
    pub(super) connections: RoutingConnectionDirectory,
}

impl<P> LiveOrchestrationCoordinator<P> {
    pub fn new(
        provider: P,
        profiles: RoutingProfileRegistry,
        connections: RoutingConnectionDirectory,
    ) -> Self {
        Self {
            provider: Arc::new(provider),
            profiles,
            connections,
        }
    }
}

impl<P: SubagentProvider + 'static> LiveOrchestrationCoordinator<P> {
    pub async fn run(
        &self,
        state: &SessionExecutionPolicyState,
        request: LiveOrchestrationRequest,
    ) -> Result<LiveOrchestrationOutcome, LiveOrchestrationError> {
        let policy = request
            .policy
            .clone()
            .or_else(|| state.resolved_policy().ok())
            .ok_or(LiveOrchestrationError::UnresolvedExecutionPolicy)?;
        let profile_id = request
            .routing_profile_id
            .clone()
            .or_else(|| state.routing_profile_id().ok().flatten())
            .or_else(|| self.profiles.active_profile_id.clone())
            .ok_or(LiveOrchestrationError::MissingRoutingProfile)?;
        validate_request(&request, &policy)?;
        begin_state(state).map_err(map_state_error)?;
        let mut events = vec![LiveEvent::RunPrepared];

        let timeout = request
            .overall_timeout
            .unwrap_or_else(|| policy.policy().batch_timeout);
        let cancellation = request.cancellation.clone();
        let mut run = Box::pin(self.run_inner(state, request, policy, profile_id, &mut events));
        match tokio::time::timeout(timeout, &mut run).await {
            Ok(result) => result,
            Err(_) => {
                // The pinned run is awaited after cancellation so O6C/O6D cannot outlive this
                // coordinator call.
                cancellation.cancel();
                let outcome = run.await?;
                state
                    .mark_timed_out_after_cleanup()
                    .map_err(map_state_error)?;
                Ok(LiveOrchestrationOutcome {
                    terminal: LiveOrchestrationTerminal::TimedOut,
                    cancelled: false,
                    timed_out: true,
                    terminal_error: Some(LiveOrchestrationError::Timeout),
                    synthesis_permitted: false,
                    events: outcome
                        .events
                        .into_iter()
                        .map(|event| match event {
                            LiveEvent::RunTerminal(_) => {
                                LiveEvent::RunTerminal(LiveOrchestrationTerminal::TimedOut)
                            }
                            event => event,
                        })
                        .collect(),
                    ..outcome
                })
            }
        }
    }

    async fn run_inner(
        &self,
        state: &SessionExecutionPolicyState,
        request: LiveOrchestrationRequest,
        policy: ResolvedExecutionPolicy,
        profile_id: RoutingProfileId,
        events: &mut Vec<LiveEvent>,
    ) -> Result<LiveOrchestrationOutcome, LiveOrchestrationError> {
        state
            .transition(SessionExecutionStatus::Validating)
            .map_err(map_state_error)?;
        let profile = self
            .profiles
            .get(&profile_id)
            .ok_or(LiveOrchestrationError::MissingRoutingProfile)?;
        let selected_profiles = selected_registry(&self.profiles, profile);
        validate_routing(
            &policy,
            profile,
            &self.connections,
            &request.verification,
            &request.planning,
        )?;
        state.mark_policy_valid().map_err(map_state_error)?;
        events.push(LiveEvent::PolicyValidated);
        if request.cancellation.is_cancelled() {
            return self.finish(
                state,
                request,
                policy,
                profile_id,
                events,
                LiveOrchestrationTerminal::Cancelled,
                Some(LiveOrchestrationError::Cancellation),
                Vec::new(),
                0,
                0,
                0,
            );
        }
        state
            .transition(SessionExecutionStatus::Running)
            .map_err(map_state_error)?;

        let mut roles = Vec::new();
        let mut provider_invocations = 0;
        let mut tool_calls = 0;
        let mut peak_concurrency = 0;

        if matches!(request.planning, PlanningContract::Required { .. }) {
            events.push(LiveEvent::RoleStarted(RoutingRole::Planner));
            let instruction = match &request.planning {
                PlanningContract::Required { instruction } => instruction.clone(),
                PlanningContract::NotRequested => String::new(),
            };
            let outcome = self
                .run_single(
                    &selected_profiles,
                    RoutingRole::Planner,
                    "planner",
                    instruction,
                    request.context.clone(),
                    request.approved_tool_policy.clone(),
                    &policy,
                    request.cancellation.clone(),
                )
                .await;
            let role = role_from_single(RoutingRole::Planner, &outcome);
            provider_invocations += role.provider_invocations;
            tool_calls += role.tool_calls;
            roles.push(role);
            if !matches!(
                outcome.as_ref().map(|value| value.status),
                Ok(SubagentStatus::Completed)
            ) {
                let terminal = if request.cancellation.is_cancelled() {
                    LiveOrchestrationTerminal::Cancelled
                } else {
                    LiveOrchestrationTerminal::Failed
                };
                return self.finish(
                    state,
                    request,
                    policy,
                    profile_id,
                    events,
                    terminal,
                    Some(if terminal == LiveOrchestrationTerminal::Cancelled {
                        LiveOrchestrationError::Cancellation
                    } else {
                        LiveOrchestrationError::ExecutorBatchFailure
                    }),
                    roles,
                    peak_concurrency,
                    provider_invocations,
                    tool_calls,
                );
            }
        } else {
            roles.push(skipped(
                RoutingRole::Planner,
                LiveRoleSkipReason::NotRequested,
            ));
            events.push(LiveEvent::RoleSkipped(
                RoutingRole::Planner,
                LiveRoleSkipReason::NotRequested,
            ));
        }

        events.push(LiveEvent::ExecutorBatchStarted);
        events.push(LiveEvent::RoleStarted(RoutingRole::Executor));
        let batch = self
            .run_executor(&selected_profiles, &policy, &request)
            .await?;
        peak_concurrency = batch.peak_observed_concurrency;
        provider_invocations += batch.aggregate_provider_turns;
        tool_calls += batch.aggregate_tool_calls;
        let executor_roles = role_from_batch(&batch);
        let executor_failed = batch.tasks.iter().any(|task| {
            matches!(
                task.state,
                SubagentTaskState::Failed | SubagentTaskState::BudgetExhausted
            )
        });
        roles.push(executor_roles);

        if request.cancellation.is_cancelled() {
            return self.finish(
                state,
                request,
                policy,
                profile_id,
                events,
                LiveOrchestrationTerminal::Cancelled,
                Some(LiveOrchestrationError::Cancellation),
                roles,
                peak_concurrency,
                provider_invocations,
                tool_calls,
            );
        }

        if matches!(request.verification, VerificationContract::Provider { .. }) {
            events.push(LiveEvent::RoleStarted(RoutingRole::Verifier));
        }
        let verification = self.verify(&selected_profiles, &policy, &request).await;
        let mut rejection = None;
        match verification {
            VerificationResult::Skipped(reason) => {
                roles.push(skipped(RoutingRole::Verifier, reason));
                events.push(LiveEvent::RoleSkipped(RoutingRole::Verifier, reason));
            }
            VerificationResult::Accepted(role) => {
                provider_invocations += role.provider_invocations;
                tool_calls += role.tool_calls;
                roles.push(role);
                events.push(LiveEvent::VerifierDecision);
            }
            VerificationResult::Rejected(role, decision) => {
                provider_invocations += role.provider_invocations;
                tool_calls += role.tool_calls;
                roles.push(role);
                rejection = Some(decision);
                events.push(LiveEvent::VerifierDecision);
            }
            VerificationResult::Failed(role) => {
                roles.push(role);
                let cancelled = request.cancellation.is_cancelled();
                return self.finish(
                    state,
                    request,
                    policy,
                    profile_id,
                    events,
                    if cancelled {
                        LiveOrchestrationTerminal::Cancelled
                    } else {
                        LiveOrchestrationTerminal::Failed
                    },
                    Some(if cancelled {
                        LiveOrchestrationError::Cancellation
                    } else {
                        LiveOrchestrationError::VerifierRuntimeFailure
                    }),
                    roles,
                    peak_concurrency,
                    provider_invocations,
                    tool_calls,
                );
            }
        }

        if let Some(VerificationDecision::Rejected {
            category,
            reason,
            repair_instruction,
        }) = rejection
        {
            events.push(LiveEvent::RepairEligibilityEvaluated);
            if policy.role(RoutingRole::Repair).activation == super::RoleActivation::Disabled {
                roles.push(skipped(RoutingRole::Repair, LiveRoleSkipReason::Disabled));
                events.push(LiveEvent::RoleSkipped(
                    RoutingRole::Repair,
                    LiveRoleSkipReason::Disabled,
                ));
                return self.finish(
                    state,
                    request,
                    policy,
                    profile_id,
                    events,
                    LiveOrchestrationTerminal::Failed,
                    Some(LiveOrchestrationError::VerifierRejected),
                    roles,
                    peak_concurrency,
                    provider_invocations,
                    tool_calls,
                );
            }
            events.push(LiveEvent::RepairStarted);
            let repair = self
                .run_repair(
                    &selected_profiles,
                    &policy,
                    &request,
                    category,
                    reason,
                    repair_instruction,
                )
                .await;
            let repair = match repair {
                Ok(outcome) => outcome,
                Err(error) => {
                    let repair_role = role_from_repair_error(error);
                    let (terminal, coordinator_error) = repair_error_terminal(error);
                    roles.push(repair_role);
                    return self.finish(
                        state,
                        request,
                        policy,
                        profile_id,
                        events,
                        terminal,
                        Some(coordinator_error),
                        roles,
                        peak_concurrency,
                        provider_invocations,
                        tool_calls,
                    );
                }
            };
            let repair_role = role_from_repair(&repair);
            provider_invocations += repair_role.provider_invocations;
            tool_calls += repair_role.tool_calls;
            let repair_ok = repair_role.state == LiveRoleState::Succeeded;
            roles.push(repair_role);
            if !repair_ok {
                let terminal = match roles.last().map(|role| role.state) {
                    Some(LiveRoleState::Cancelled) => LiveOrchestrationTerminal::Cancelled,
                    Some(LiveRoleState::TimedOut) => LiveOrchestrationTerminal::TimedOut,
                    Some(LiveRoleState::BudgetExhausted) => {
                        LiveOrchestrationTerminal::BudgetExhausted
                    }
                    _ => LiveOrchestrationTerminal::Failed,
                };
                return self.finish(
                    state,
                    request,
                    policy,
                    profile_id,
                    events,
                    terminal,
                    Some(match terminal {
                        LiveOrchestrationTerminal::Cancelled => {
                            LiveOrchestrationError::Cancellation
                        }
                        LiveOrchestrationTerminal::TimedOut => LiveOrchestrationError::Timeout,
                        LiveOrchestrationTerminal::BudgetExhausted => {
                            LiveOrchestrationError::BudgetExhaustion
                        }
                        _ => LiveOrchestrationError::RepairFailed,
                    }),
                    roles,
                    peak_concurrency,
                    provider_invocations,
                    tool_calls,
                );
            }
        } else {
            let reason =
                if policy.role(RoutingRole::Repair).activation == super::RoleActivation::Disabled {
                    LiveRoleSkipReason::Disabled
                } else {
                    LiveRoleSkipReason::NoEligibleRepair
                };
            roles.push(skipped(RoutingRole::Repair, reason));
            events.push(LiveEvent::RoleSkipped(RoutingRole::Repair, reason));
        }

        if request.cancellation.is_cancelled() {
            return self.finish(
                state,
                request,
                policy,
                profile_id,
                events,
                LiveOrchestrationTerminal::Cancelled,
                Some(LiveOrchestrationError::Cancellation),
                roles,
                peak_concurrency,
                provider_invocations,
                tool_calls,
            );
        }
        let terminal = if executor_failed {
            LiveOrchestrationTerminal::Failed
        } else {
            LiveOrchestrationTerminal::Completed
        };
        let error = match terminal {
            LiveOrchestrationTerminal::Cancelled => Some(LiveOrchestrationError::Cancellation),
            LiveOrchestrationTerminal::Failed => Some(LiveOrchestrationError::ExecutorBatchFailure),
            _ => None,
        };
        self.finish(
            state,
            request,
            policy,
            profile_id,
            events,
            terminal,
            error,
            roles,
            peak_concurrency,
            provider_invocations,
            tool_calls,
        )
    }

    fn finish(
        &self,
        state: &SessionExecutionPolicyState,
        request: LiveOrchestrationRequest,
        policy: ResolvedExecutionPolicy,
        profile_id: RoutingProfileId,
        events: &mut Vec<LiveEvent>,
        terminal: LiveOrchestrationTerminal,
        error: Option<LiveOrchestrationError>,
        roles: Vec<LiveRoleOutcome>,
        peak_concurrency: usize,
        provider_invocations: usize,
        tool_calls: usize,
    ) -> Result<LiveOrchestrationOutcome, LiveOrchestrationError> {
        let state_terminal = match terminal {
            LiveOrchestrationTerminal::Completed => SessionExecutionStatus::Completed,
            LiveOrchestrationTerminal::Cancelled => SessionExecutionStatus::Cancelling,
            LiveOrchestrationTerminal::TimedOut => SessionExecutionStatus::TimedOut,
            LiveOrchestrationTerminal::Failed | LiveOrchestrationTerminal::BudgetExhausted => {
                SessionExecutionStatus::Failed
            }
        };
        if state_terminal == SessionExecutionStatus::Cancelling {
            state.transition(state_terminal).map_err(map_state_error)?;
            state
                .transition(SessionExecutionStatus::Cancelled)
                .map_err(map_state_error)?;
        } else {
            state.transition(state_terminal).map_err(map_state_error)?;
        }
        events.push(LiveEvent::RunTerminal(terminal));
        events.truncate(MAX_EVENTS);
        Ok(LiveOrchestrationOutcome {
            run_id: request.run_id,
            selected_mode: policy.selected_mode().clone(),
            resolved_policy: policy.explain(),
            routing_profile_id: profile_id,
            terminal,
            roles,
            peak_concurrency,
            provider_invocations,
            tool_calls,
            cancelled: terminal == LiveOrchestrationTerminal::Cancelled,
            timed_out: terminal == LiveOrchestrationTerminal::TimedOut,
            budget_exhausted: terminal == LiveOrchestrationTerminal::BudgetExhausted,
            terminal_error: error,
            synthesis_permitted: matches!(terminal, LiveOrchestrationTerminal::Completed),
            events: events.clone(),
        })
    }
}
