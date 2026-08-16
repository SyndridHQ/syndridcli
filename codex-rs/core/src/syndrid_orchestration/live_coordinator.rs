use super::ExecutionBudgetLedger;
use super::ResolvedExecutionPolicy;
use super::RoutingConnectionDirectory;
use super::RoutingProfileId;
use super::RoutingProfileRegistry;
use super::RoutingRole;
use super::SessionExecutionPolicyState;
use super::SessionExecutionStatus;
use super::SubagentError;
use super::SubagentProvider;
use super::SubagentStatus;
use super::SubagentTaskState;
use super::live_coordinator_mapping::*;
use super::live_coordinator_stages::*;
use super::live_coordinator_types::*;
use super::live_coordinator_validation::*;
use super::observation_delivery::NoopOrchestrationObservationSink;
use super::observation_delivery::OrchestrationObservationSink;
use super::orchestration_cleanup::CleanupChildKind;
use super::orchestration_cleanup::OrchestrationCleanup;
use super::orchestration_failure::OrchestrationFailure;
use super::orchestration_observability_runtime::ObservationIdentity;
use super::orchestration_observability_runtime::OrchestrationObservationCollector;
use std::sync::Arc;

/// Executes one explicit bounded workflow using O6A–O6E runtimes.
pub struct LiveOrchestrationCoordinator<P> {
    pub(super) provider: Arc<P>,
    pub(super) profiles: RoutingProfileRegistry,
    pub(super) connections: RoutingConnectionDirectory,
    pub(super) observation_sink: Arc<dyn OrchestrationObservationSink>,
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
            observation_sink: Arc::new(NoopOrchestrationObservationSink),
        }
    }

    pub fn with_observation_sink<S>(mut self, sink: S) -> Self
    where
        S: OrchestrationObservationSink + 'static,
    {
        self.observation_sink = Arc::new(sink);
        self
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
        let context_bytes = request
            .instruction
            .len()
            .saturating_add(request.context.as_ref().map_or(0, String::len))
            .saturating_add(
                request
                    .tasks
                    .iter()
                    .map(|task| {
                        task.instruction
                            .len()
                            .saturating_add(task.context.as_ref().map_or(0, String::len))
                    })
                    .sum(),
            );
        if context_bytes > policy.policy().context_budget_bytes {
            return Err(LiveOrchestrationError::BudgetExhaustionCategory(
                super::BudgetExhaustionCategory::InputOrContextLimit,
            ));
        }
        let generation = begin_state(state).map_err(map_state_error)?;
        let budget = Arc::new(ExecutionBudgetLedger::new_for_generation(
            &policy, generation,
        ));
        let cleanup = Arc::new(OrchestrationCleanup::new(generation));
        budget
            .reserve_context(context_bytes)
            .map_err(|_| LiveOrchestrationError::BudgetExhaustion)?;
        let mut events = vec![LiveEvent::RunPrepared];
        let identity = ObservationIdentity {
            generation,
            run_id: request.run_id.clone(),
            mode: policy.selected_mode().clone(),
            source: policy.explain().source,
            profile_id: profile_id.clone(),
            policy: policy.clone(),
        };
        let collector = Arc::new(OrchestrationObservationCollector::new(&identity));
        self.publish_progress(&collector, &identity, &budget, &events);

        let timeout = request
            .overall_timeout
            .unwrap_or_else(|| policy.policy().batch_timeout);
        let cancellation = request.cancellation.clone();
        let timeout_cleanup = cleanup.clone();
        let mut run = Box::pin(self.run_inner(
            state,
            request,
            policy,
            profile_id,
            budget,
            cleanup,
            &mut events,
            &identity,
            &collector,
        ));
        let result = match tokio::time::timeout(timeout, &mut run).await {
            Ok(result) => result,
            Err(_) => {
                let timeout_failure = OrchestrationFailure::from_terminal(
                    LiveOrchestrationTerminal::TimedOut,
                    Some(LiveOrchestrationError::Timeout),
                    &[],
                    None,
                )
                .ok_or(LiveOrchestrationError::InternalCoordinatorFailure)?;
                timeout_cleanup.submit_failure(generation, timeout_failure);
                cancellation.cancel();
                // The pinned run is awaited after arbiter submission and cancellation so
                // O6C/O6D cannot outlive this coordinator call.
                run.await
            }
        };
        if let Ok(outcome) = &result {
            self.observation_sink.publish(outcome.observation.clone());
        }
        result
    }

    fn publish_progress(
        &self,
        collector: &OrchestrationObservationCollector,
        identity: &ObservationIdentity,
        budget: &ExecutionBudgetLedger,
        events: &[LiveEvent],
    ) {
        self.observation_sink.publish(collector.snapshot_progress(
            identity,
            &[],
            &budget.snapshot(),
            events,
            0,
        ));
    }

    // Keeping these lifecycle, budget, observation, and request authorities explicit avoids
    // bundling independently owned state into a synthetic context object solely for lint shape.
    #[allow(clippy::too_many_arguments)]
    async fn run_inner(
        &self,
        state: &SessionExecutionPolicyState,
        request: LiveOrchestrationRequest,
        policy: ResolvedExecutionPolicy,
        profile_id: RoutingProfileId,
        budget: Arc<ExecutionBudgetLedger>,
        cleanup: Arc<OrchestrationCleanup>,
        events: &mut Vec<LiveEvent>,
        identity: &ObservationIdentity,
        collector: &OrchestrationObservationCollector,
    ) -> Result<LiveOrchestrationOutcome, LiveOrchestrationError> {
        state
            .transition(SessionExecutionStatus::Validating)
            .map_err(map_state_error)?;
        if request.cancellation.is_cancelled() {
            return finish_outcome(
                state,
                request,
                policy,
                profile_id,
                &budget,
                events,
                LiveOrchestrationTerminal::Cancelled,
                Some(LiveOrchestrationError::Cancellation),
                Vec::new(),
                0,
                0,
                0,
                cleanup.as_ref(),
            );
        }
        let profile = match self.profiles.get(&profile_id) {
            Some(profile) => profile,
            None => {
                return Err(terminalize_pre_execution_failure(
                    state,
                    &budget,
                    cleanup.as_ref(),
                    LiveOrchestrationError::MissingRoutingProfile,
                ));
            }
        };
        let selected_profiles = selected_registry(&self.profiles, profile);
        if let Err(error) = validate_routing(
            &policy,
            profile,
            &self.connections,
            &request.verification,
            &request.planning,
        ) {
            return Err(terminalize_pre_execution_failure(
                state,
                &budget,
                cleanup.as_ref(),
                error,
            ));
        }
        state.mark_policy_valid().map_err(map_state_error)?;
        events.push(LiveEvent::PolicyValidated);
        self.publish_progress(collector, identity, &budget, events);
        if request.cancellation.is_cancelled() {
            return finish_outcome(
                state,
                request,
                policy,
                profile_id,
                &budget,
                events,
                LiveOrchestrationTerminal::Cancelled,
                Some(LiveOrchestrationError::Cancellation),
                Vec::new(),
                0,
                0,
                0,
                cleanup.as_ref(),
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
            let planner_child = cleanup
                .register_child(budget.generation(), CleanupChildKind::Planner)
                .map_err(|_| LiveOrchestrationError::InternalCoordinatorFailure)?;
            events.push(LiveEvent::RoleStarted(RoutingRole::Planner));
            self.publish_progress(collector, identity, &budget, events);
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
                    budget.clone(),
                    Some(cleanup.clone()),
                )
                .await;
            cleanup
                .complete_child(budget.generation(), planner_child)
                .map_err(|_| LiveOrchestrationError::InternalCoordinatorFailure)?;
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
                return finish_outcome(
                    state,
                    request,
                    policy,
                    profile_id,
                    &budget,
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
                    cleanup.as_ref(),
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
        self.publish_progress(collector, identity, &budget, events);
        let executor_child = cleanup
            .register_child(budget.generation(), CleanupChildKind::ExecutorBatch)
            .map_err(|_| LiveOrchestrationError::InternalCoordinatorFailure)?;
        if budget
            .admit_executor_tasks(request.tasks.len().max(1))
            .is_err()
        {
            cleanup
                .complete_child(budget.generation(), executor_child)
                .map_err(|_| LiveOrchestrationError::InternalCoordinatorFailure)?;
            return finish_outcome(
                state,
                request,
                policy,
                profile_id,
                &budget,
                events,
                LiveOrchestrationTerminal::BudgetExhausted,
                Some(LiveOrchestrationError::BudgetExhaustion),
                roles,
                peak_concurrency,
                provider_invocations,
                tool_calls,
                cleanup.as_ref(),
            );
        }
        let batch = match self
            .run_executor(
                &selected_profiles,
                &policy,
                &request,
                budget.clone(),
                cleanup.clone(),
            )
            .await
        {
            Ok(batch) => batch,
            Err(error) => {
                cleanup
                    .complete_child(budget.generation(), executor_child)
                    .map_err(|_| LiveOrchestrationError::InternalCoordinatorFailure)?;
                return finish_outcome(
                    state,
                    request,
                    policy,
                    profile_id,
                    &budget,
                    events,
                    LiveOrchestrationTerminal::Failed,
                    Some(error),
                    roles,
                    peak_concurrency,
                    provider_invocations,
                    tool_calls,
                    cleanup.as_ref(),
                );
            }
        };
        cleanup
            .complete_child(budget.generation(), executor_child)
            .map_err(|_| LiveOrchestrationError::InternalCoordinatorFailure)?;
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
            return finish_outcome(
                state,
                request,
                policy,
                profile_id,
                &budget,
                events,
                LiveOrchestrationTerminal::Cancelled,
                Some(LiveOrchestrationError::Cancellation),
                roles,
                peak_concurrency,
                provider_invocations,
                tool_calls,
                cleanup.as_ref(),
            );
        }

        if matches!(request.verification, VerificationContract::Provider { .. }) {
            events.push(LiveEvent::RoleStarted(RoutingRole::Verifier));
            self.publish_progress(collector, identity, &budget, events);
        }
        let verifier_child = cleanup
            .register_child(budget.generation(), CleanupChildKind::Verifier)
            .map_err(|_| LiveOrchestrationError::InternalCoordinatorFailure)?;
        let verification = self
            .verify(
                &selected_profiles,
                &policy,
                &request,
                budget.clone(),
                cleanup.clone(),
            )
            .await;
        cleanup
            .complete_child(budget.generation(), verifier_child)
            .map_err(|_| LiveOrchestrationError::InternalCoordinatorFailure)?;
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
                return finish_outcome(
                    state,
                    request,
                    policy,
                    profile_id,
                    &budget,
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
                    cleanup.as_ref(),
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
                return finish_outcome(
                    state,
                    request,
                    policy,
                    profile_id,
                    &budget,
                    events,
                    LiveOrchestrationTerminal::Failed,
                    Some(LiveOrchestrationError::VerifierRejected),
                    roles,
                    peak_concurrency,
                    provider_invocations,
                    tool_calls,
                    cleanup.as_ref(),
                );
            }
            events.push(LiveEvent::RepairStarted);
            self.publish_progress(collector, identity, &budget, events);
            let repair_child = cleanup
                .register_child(budget.generation(), CleanupChildKind::Repair)
                .map_err(|_| LiveOrchestrationError::InternalCoordinatorFailure)?;
            if budget.admit_repair_attempt().is_err() {
                cleanup
                    .complete_child(budget.generation(), repair_child)
                    .map_err(|_| LiveOrchestrationError::InternalCoordinatorFailure)?;
                return finish_outcome(
                    state,
                    request,
                    policy,
                    profile_id,
                    &budget,
                    events,
                    LiveOrchestrationTerminal::BudgetExhausted,
                    Some(LiveOrchestrationError::BudgetExhaustion),
                    roles,
                    peak_concurrency,
                    provider_invocations,
                    tool_calls,
                    cleanup.as_ref(),
                );
            }
            let repair = self
                .run_repair(
                    &selected_profiles,
                    &policy,
                    &request,
                    category,
                    reason,
                    repair_instruction,
                    budget.clone(),
                    cleanup.clone(),
                )
                .await;
            cleanup
                .complete_child(budget.generation(), repair_child)
                .map_err(|_| LiveOrchestrationError::InternalCoordinatorFailure)?;
            let repair = match repair {
                Ok(outcome) => outcome,
                Err(error) => {
                    let repair_role = role_from_repair_error(error);
                    let (terminal, coordinator_error) = repair_error_terminal(error);
                    roles.push(repair_role);
                    return finish_outcome(
                        state,
                        request,
                        policy,
                        profile_id,
                        &budget,
                        events,
                        terminal,
                        Some(coordinator_error),
                        roles,
                        peak_concurrency,
                        provider_invocations,
                        tool_calls,
                        cleanup.as_ref(),
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
                return finish_outcome(
                    state,
                    request,
                    policy,
                    profile_id,
                    &budget,
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
                    cleanup.as_ref(),
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
            return finish_outcome(
                state,
                request,
                policy,
                profile_id,
                &budget,
                events,
                LiveOrchestrationTerminal::Cancelled,
                Some(LiveOrchestrationError::Cancellation),
                roles,
                peak_concurrency,
                provider_invocations,
                tool_calls,
                cleanup.as_ref(),
            );
        }
        let terminal = if executor_failed {
            LiveOrchestrationTerminal::Failed
        } else {
            LiveOrchestrationTerminal::Completed
        };
        let error = match terminal {
            LiveOrchestrationTerminal::Cancelled => Some(LiveOrchestrationError::Cancellation),
            LiveOrchestrationTerminal::Failed
                if batch
                    .tasks
                    .iter()
                    .any(|task| task.error == Some(SubagentError::JoinFailure)) =>
            {
                Some(LiveOrchestrationError::ExecutorJoinFailure)
            }
            LiveOrchestrationTerminal::Failed => Some(LiveOrchestrationError::ExecutorBatchFailure),
            _ => None,
        };
        finish_outcome(
            state,
            request,
            policy,
            profile_id,
            &budget,
            events,
            terminal,
            error,
            roles,
            peak_concurrency,
            provider_invocations,
            tool_calls,
            cleanup.as_ref(),
        )
    }
}
