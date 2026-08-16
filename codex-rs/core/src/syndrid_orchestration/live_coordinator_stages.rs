use super::ExecutionBudgetLedger;
use super::ResolvedExecutionPolicy;
use super::RoutingProfileRegistry;
use super::RoutingRole;
use super::SubagentBatchOutcome;
use super::SubagentBatchRequest;
use super::SubagentBatchRuntime;
use super::SubagentOutcome;
use super::SubagentProvider;
use super::SubagentRepairEligibility;
use super::SubagentRepairFailureCategory;
use super::SubagentRepairRoute;
use super::SubagentRepairRuntime;
use super::SubagentRequest;
use super::SubagentRuntime;
use super::SubagentStatus;
use super::SubagentTask;
use super::SubagentToolPolicy;
use super::live_coordinator_mapping::*;
use super::live_coordinator_types::*;
use super::orchestration_cleanup::OrchestrationCleanup;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

impl<P: SubagentProvider + 'static> super::live_coordinator::LiveOrchestrationCoordinator<P> {
    pub(super) async fn run_executor(
        &self,
        profiles: &RoutingProfileRegistry,
        policy: &ResolvedExecutionPolicy,
        request: &LiveOrchestrationRequest,
        budget: Arc<ExecutionBudgetLedger>,
        cleanup: Arc<OrchestrationCleanup>,
    ) -> Result<SubagentBatchOutcome, LiveOrchestrationError> {
        let runtime = SubagentBatchRuntime::new(SubagentRuntime::new(
            SharedProvider(self.provider.clone()),
            profiles.clone(),
            self.connections.clone(),
        ));
        let task_count = request.tasks.len().max(1);
        let provider_budget = (policy.policy().max_provider_invocations / task_count).max(1);
        let tool_budget = (policy.policy().max_tool_calls / task_count).max(1);
        let tool_output_budget = (policy.policy().max_tool_output_bytes / task_count).max(1);
        let executor_cancellation = request.cancellation.child_token();
        let tasks = request
            .tasks
            .iter()
            .map(|task| SubagentTask {
                request: self.make_request(
                    profiles,
                    RoutingRole::Executor,
                    task.task_id.clone(),
                    task.instruction.clone(),
                    task.context.clone(),
                    task.tool_policy.with_execution_limits(
                        provider_budget,
                        tool_budget,
                        tool_output_budget,
                    ),
                    policy,
                    request.cancellation.clone(),
                    Some(budget.clone()),
                    Some(cleanup.clone()),
                ),
                timeout_override: task.timeout,
            })
            .collect();
        runtime
            .run(SubagentBatchRequest {
                tasks,
                policy: policy.to_batch_policy(request.failure_policy),
                cancellation: executor_cancellation,
            })
            .await
            .map_err(|error| match error {
                super::SubagentBatchError::TaskCountExceeded => {
                    LiveOrchestrationError::ExecutorTasksExceedPolicyCeiling
                }
                super::SubagentBatchError::DuplicateTaskId
                | super::SubagentBatchError::InvalidTask(_) => {
                    LiveOrchestrationError::InvalidTaskIdentifiers
                }
                _ => LiveOrchestrationError::ExecutorBatchFailure,
            })
    }

    // This helper carries the complete single-subagent authority set. Grouping these values
    // solely for lint shape would obscure the ownership boundary used by planner/verifier calls.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn run_single(
        &self,
        profiles: &RoutingProfileRegistry,
        role: RoutingRole,
        task_id: &str,
        instruction: String,
        context: Option<String>,
        tool_policy: SubagentToolPolicy,
        policy: &ResolvedExecutionPolicy,
        cancellation: CancellationToken,
        budget: Arc<ExecutionBudgetLedger>,
        cleanup: Option<Arc<OrchestrationCleanup>>,
    ) -> Result<SubagentOutcome, super::SubagentError> {
        let runtime = SubagentRuntime::new(
            SharedProvider(self.provider.clone()),
            profiles.clone(),
            self.connections.clone(),
        );
        runtime
            .run_subagent(self.make_request(
                profiles,
                role,
                task_id.to_string(),
                instruction,
                context,
                tool_policy,
                policy,
                cancellation,
                Some(budget),
                cleanup,
            ))
            .await
    }

    pub(super) async fn verify(
        &self,
        profiles: &RoutingProfileRegistry,
        policy: &ResolvedExecutionPolicy,
        request: &LiveOrchestrationRequest,
        budget: Arc<ExecutionBudgetLedger>,
        cleanup: Arc<OrchestrationCleanup>,
    ) -> VerificationResult {
        if policy.role(RoutingRole::Verifier).activation == super::RoleActivation::Disabled {
            return VerificationResult::Skipped(LiveRoleSkipReason::Disabled);
        }
        match &request.verification {
            VerificationContract::NotRequested => {
                VerificationResult::Skipped(LiveRoleSkipReason::NotRequested)
            }
            VerificationContract::Decision(decision) => match decision {
                VerificationDecision::Accepted => VerificationResult::Accepted(skipped(
                    RoutingRole::Verifier,
                    LiveRoleSkipReason::NotRequested,
                )),
                VerificationDecision::Rejected {
                    category,
                    reason,
                    repair_instruction,
                } => VerificationResult::Rejected(
                    skipped(RoutingRole::Verifier, LiveRoleSkipReason::NotRequested),
                    VerificationDecision::Rejected {
                        category: *category,
                        reason: reason.clone(),
                        repair_instruction: repair_instruction.clone(),
                    },
                ),
            },
            VerificationContract::Provider { instruction } => match self
                .run_single(
                    profiles,
                    RoutingRole::Verifier,
                    "verifier",
                    instruction.clone(),
                    request.context.clone(),
                    request.approved_tool_policy.clone(),
                    policy,
                    request.cancellation.clone(),
                    budget.clone(),
                    Some(cleanup),
                )
                .await
            {
                Ok(outcome)
                    if outcome.status == SubagentStatus::Completed
                        && outcome.output.as_deref().map(str::trim) == Some("ACCEPT") =>
                {
                    VerificationResult::Accepted(role_from_single(
                        RoutingRole::Verifier,
                        &Ok(outcome),
                    ))
                }
                Ok(outcome)
                    if outcome.status == SubagentStatus::Completed
                        && outcome
                            .output
                            .as_deref()
                            .and_then(|text| text.strip_prefix("REJECT\n"))
                            .is_some() =>
                {
                    let reason = outcome
                        .output
                        .clone()
                        .map_or_else(|| "verifier rejected".to_string(), |text| text);
                    VerificationResult::Rejected(
                        role_from_single(RoutingRole::Verifier, &Ok(outcome)),
                        VerificationDecision::Rejected {
                            category: SubagentRepairFailureCategory::VerifierRejected,
                            reason,
                            repair_instruction: request.repair_instruction.clone(),
                        },
                    )
                }
                Ok(outcome) => VerificationResult::Failed(role_from_single(
                    RoutingRole::Verifier,
                    &Ok(outcome),
                )),
                Err(_) => VerificationResult::Failed(LiveRoleOutcome {
                    role: RoutingRole::Verifier,
                    state: LiveRoleState::Failed,
                    skip_reason: None,
                    task_ids: vec!["verifier".to_string()],
                    task_states: vec![LiveRoleState::Failed],
                    provider_invocations: 0,
                    tool_calls: 0,
                    repair_result: None,
                    repair_attempts: 0,
                }),
            },
        }
    }

    pub(super) async fn run_repair(
        &self,
        profiles: &RoutingProfileRegistry,
        policy: &ResolvedExecutionPolicy,
        request: &LiveOrchestrationRequest,
        category: SubagentRepairFailureCategory,
        reason: String,
        instruction: String,
        budget: Arc<ExecutionBudgetLedger>,
        cleanup: Arc<OrchestrationCleanup>,
    ) -> Result<super::SubagentRepairOutcome, super::SubagentRepairError> {
        let profile = profiles
            .active()
            .map_err(|_| super::SubagentRepairError::RouteMismatch)?;
        let assignment = profile
            .assignments
            .get(&RoutingRole::Repair)
            .ok_or(super::SubagentRepairError::RouteMismatch)?;
        let route = SubagentRepairRoute {
            profile_id: profile.id.as_str().to_string(),
            role: RoutingRole::Repair,
            provider_id: assignment.provider_id.clone(),
            connection_id: assignment.connection_id.clone(),
            model_id: assignment.model_id.clone(),
        };
        let repair_policy = policy
            .repair_policy(route)
            .map_err(|_| super::SubagentRepairError::PolicyInvalid)?;
        let super::RepairPolicyDecision::Enabled(repair_policy) = repair_policy else {
            return Err(super::SubagentRepairError::PolicyInvalid);
        };
        let runtime = SubagentRepairRuntime::new(
            SubagentRuntime::new(
                SharedProvider(self.provider.clone()),
                profiles.clone(),
                self.connections.clone(),
            ),
            super::SubagentRepairBudget::new(
                repair_policy.max_provider_invocations,
                repair_policy.max_tool_calls,
                repair_policy.max_context_bytes,
                repair_policy.max_output_tokens,
            )
            .map_err(|_| super::SubagentRepairError::BudgetExhausted)?,
        );
        runtime
            .run(
                self.make_request(
                    profiles,
                    RoutingRole::Repair,
                    "repair".to_string(),
                    request.instruction.clone(),
                    request.context.clone(),
                    request.approved_tool_policy.clone(),
                    policy,
                    request.cancellation.clone(),
                    Some(budget.clone()),
                    Some(cleanup),
                ),
                repair_policy,
                SubagentRepairEligibility::Eligible(category),
                reason,
                instruction,
            )
            .await
    }

    pub(super) fn make_request(
        &self,
        _profiles: &RoutingProfileRegistry,
        role: RoutingRole,
        task_id: String,
        instruction: String,
        context: Option<String>,
        tool_policy: SubagentToolPolicy,
        policy: &ResolvedExecutionPolicy,
        cancellation: CancellationToken,
        budget: Option<Arc<ExecutionBudgetLedger>>,
        cleanup: Option<Arc<OrchestrationCleanup>>,
    ) -> SubagentRequest {
        SubagentRequest {
            task_id,
            parent_id: None,
            role,
            instruction,
            context,
            timeout: policy.policy().task_timeout,
            max_output_tokens: policy.policy().output_budget_tokens,
            cancellation,
            depth: 1,
            tool_policy: tool_policy.with_execution_limits(
                policy.policy().max_provider_invocations,
                policy.policy().max_tool_calls,
                policy.policy().max_tool_output_bytes,
            ),
            budget,
            cleanup,
        }
    }
}

#[derive(Clone)]
pub(super) struct SharedProvider<P>(pub(super) Arc<P>);

impl<P: SubagentProvider> SubagentProvider for SharedProvider<P> {
    fn invoke(
        &self,
        request: super::ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> impl std::future::Future<
        Output = Result<super::ProviderInvocationResult, super::ProviderInvocationError>,
    > + Send {
        self.0.invoke(request, cancellation)
    }

    fn invoke_role(
        &self,
        role: RoutingRole,
        request: super::ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> impl std::future::Future<
        Output = Result<super::ProviderInvocationResult, super::ProviderInvocationError>,
    > + Send {
        self.0.invoke_role(role, request, cancellation)
    }

    fn resolved_role_route(
        &self,
        role: RoutingRole,
    ) -> Option<super::subagent::SubagentResolvedRoute> {
        self.0.resolved_role_route(role)
    }
}

pub(super) enum VerificationResult {
    Skipped(LiveRoleSkipReason),
    Accepted(LiveRoleOutcome),
    Rejected(LiveRoleOutcome, VerificationDecision),
    Failed(LiveRoleOutcome),
}
