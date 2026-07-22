use super::*;
use codex_orchestration::RouteSource;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;

fn model(value: &str) -> ModelRoute {
    ModelRoute {
        requested: Some(value.to_string()),
        resolved: Some(value.to_string()),
        source: RouteSource::User,
        status: RouteStatus::Resolved,
        data_quality: DataQuality::Exact,
    }
}

fn route(provider: &str, model_name: &str) -> ProviderNeutralRoute {
    ProviderNeutralRoute {
        provider: provider.to_string(),
        model: model(model_name),
        effort: EffortRoute {
            requested: Some(ReasoningEffort::Low),
            resolved: Some(ReasoningEffort::Low),
            source: RouteSource::User,
            status: RouteStatus::Resolved,
            data_quality: DataQuality::Exact,
        },
        capabilities: RouteCapabilities {
            text_generation: true,
            tool_calling: true,
            structured_output: true,
            read_only: true,
            writer: true,
            minimum_context_tokens: Some(1024),
        },
    }
}

async fn base_request() -> RunOrchestratedTaskRequest {
    RunOrchestratedTaskRequest {
        task: "implement the bounded task".to_string(),
        workflow_id: WorkflowId::new("workflow-1").expect("workflow id"),
        task_id: TaskId::new("task-1").expect("task id"),
        planner_agent_id: AgentId::new("planner").expect("agent id"),
        executor_agent_id: AgentId::new("executor").expect("agent id"),
        initial_verifier_agent_id: AgentId::new("verifier").expect("agent id"),
        repair_executor_agent_id: AgentId::new("repair").expect("agent id"),
        final_verifier_agent_id: AgentId::new("final-verifier").expect("agent id"),
        parent: ParentExecutionContext {
            agent_control: AgentControl::default(),
            parent_thread_id: ThreadId::new(),
            parent_session_source: SessionSource::Cli,
        },
        base_config: crate::config::test_config().await,
        permission_ceiling: PermissionEnvelope::new(
            WorkAccess::Writer,
            WorkAccess::Writer,
            WorkAccess::Writer,
        )
        .expect("permission ceiling"),
        planner_route: route("provider-a", "planner-model"),
        executor_route: route("provider-b", "executor-model"),
        verifier_route: route("provider-c", "verifier-model"),
    }
}

#[tokio::test]
async fn invalid_requests_are_rejected_without_spawns() {
    let mut request = base_request().await;
    request.task = " \n\t".to_string();
    let result = run_orchestrated_task(request).await;
    assert_eq!(result.outcome, OrchestratedTaskOutcome::InvalidRequest);
    assert_eq!(result.execution.spawn_count, 0);
}

#[tokio::test]
async fn oversized_requests_are_rejected_without_spawns() {
    let mut request = base_request().await;
    request.task = "x".repeat(MAX_ORCHESTRATED_TASK_BYTES + 1);
    let result = run_orchestrated_task(request).await;
    assert_eq!(result.outcome, OrchestratedTaskOutcome::InvalidRequest);
    assert_eq!(result.execution.spawn_count, 0);
}

#[tokio::test]
async fn duplicate_agents_and_incompatible_routes_are_rejected_without_spawns() {
    let mut request = base_request().await;
    request.repair_executor_agent_id = request.executor_agent_id.clone();
    assert!(request.assignments().is_err());
    request = base_request().await;
    request.verifier_route.capabilities.structured_output = false;
    let result = run_orchestrated_task(request).await;
    assert_eq!(result.outcome, OrchestratedTaskOutcome::InvalidRequest);
    assert_eq!(result.execution.spawn_count, 0);
}

#[tokio::test]
async fn assignments_preserve_provider_routes_and_fixed_policy() {
    let request = base_request().await;
    let assignments = request.assignments().expect("valid assignments").values;
    assert_eq!(assignments.len(), 5);
    assert_eq!(assignments[0].access, WorkAccess::ReadOnly);
    assert_eq!(assignments[1].access, WorkAccess::Writer);
    assert_eq!(assignments[2].access, WorkAccess::ReadOnly);
    assert_eq!(assignments[3].access, WorkAccess::Writer);
    assert_eq!(assignments[4].access, WorkAccess::ReadOnly);
    assert_eq!(assignments[1].model_route, request.executor_route.model);
    assert_eq!(assignments[3].model_route, request.executor_route.model);
    assert_eq!(assignments[2].model_route, request.verifier_route.model);
    assert_eq!(assignments[4].model_route, request.verifier_route.model);
    assert_eq!(assignments[0].provider, "provider-a");
    assert_eq!(assignments[1].provider, "provider-b");
    assert_eq!(assignments[3].provider, "provider-b");
    assert_eq!(assignments[2].provider, "provider-c");
    assert_eq!(assignments[4].provider, "provider-c");
    assert_eq!(assignments[1].effort_route, request.executor_route.effort);
    assert_eq!(assignments[3].effort_route, request.executor_route.effort);
    assert_eq!(assignments[2].effort_route, request.verifier_route.effort);
    assert_eq!(assignments[4].effort_route, request.verifier_route.effort);
    assert_eq!(request.planner_route.provider, "provider-a");
    assert_eq!(request.executor_route.provider, "provider-b");
    assert_eq!(request.verifier_route.provider, "provider-c");
}

#[tokio::test]
async fn invalid_provider_and_model_routes_are_rejected_without_spawns() {
    let mut request = base_request().await;
    request.planner_route.provider.clear();
    let result = run_orchestrated_task(request).await;
    assert_eq!(result.outcome, OrchestratedTaskOutcome::InvalidRequest);
    assert_eq!(result.execution.spawn_count, 0);
    let mut request = base_request().await;
    request.executor_route.model.resolved = None;
    let result = run_orchestrated_task(request).await;
    assert_eq!(result.outcome, OrchestratedTaskOutcome::InvalidRequest);
    assert_eq!(result.execution.spawn_count, 0);
}
