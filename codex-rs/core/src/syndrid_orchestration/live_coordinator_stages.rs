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

    // This constructor deliberately keeps the request's independent routing,
    // policy, cancellation, budget, and cleanup authorities explicit. Folding
    // them into a synthetic context object solely to satisfy argument-count
    // linting would obscure ownership at the subagent request boundary.
    #[allow(clippy::too_many_arguments)]
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