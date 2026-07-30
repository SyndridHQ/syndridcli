use super::*;
use crate::syndrid_orchestration::invocation::ProviderInvocation;
use crate::syndrid_orchestration::invocation::ProviderInvocationUsage;
use crate::syndrid_orchestration::omniroute::ProviderSelection;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct RecordingProvider {
    result: Result<(), ProviderInvocationError>,
    calls: Arc<Mutex<Vec<ProviderInvocationRequest>>>,
}

impl ProviderInvocation for RecordingProvider {
    async fn invoke(
        &self,
        request: ProviderInvocationRequest,
        _cancellation: CancellationToken,
    ) -> Result<ProviderInvocationResult, ProviderInvocationError> {
        self.calls.lock().expect("calls lock").push(request.clone());
        self.result.clone()?;
        Ok(ProviderInvocationResult {
            provider: request.provider,
            model: request.model,
            text: "bounded-result".to_string(),
            finish_reason: Some("stop".to_string()),
            usage: Some(ProviderInvocationUsage {
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
            }),
            request_id: None,
            tool_call: None,
        })
    }
}

fn route(connection: &str, provider: &str, model: &str) -> ProductionProviderRoute {
    ProductionProviderRoute::new(
        ProviderSelection::new(connection, provider, model).expect("selection"),
        ReasoningEffort::Medium,
    )
}

fn request(provider: &str, model: &str) -> ProviderInvocationRequest {
    ProviderInvocationRequest {
        provider: provider.to_string(),
        model: model.to_string(),
        system: None,
        user: "bounded-input".to_string(),
        max_output_tokens: 32,
        tools: Vec::new(),
        tool_results: Vec::new(),
    }
}

fn invocation(
    role: RoutingRole,
    route: &ProductionProviderRoute,
    request: ProviderInvocationRequest,
) -> ProductionRoleInvocationRequest {
    ProductionRoleInvocationRequest::new(role, route, request, None)
}

#[tokio::test]
async fn dispatches_each_role_through_its_exact_route() {
    let planner_calls = Arc::new(Mutex::new(Vec::new()));
    let executor_calls = Arc::new(Mutex::new(Vec::new()));
    let planner_route = route("codex-a", "codex", "planner-model");
    let executor_route = route("omni-b", "omniroute", "executor-model");
    let dispatcher = ProductionRoleDispatcher::new([
        (
            RoutingRole::Planner,
            ProductionRoleBinding::new(
                planner_route.clone(),
                RecordingProvider {
                    result: Ok(()),
                    calls: Arc::clone(&planner_calls),
                },
            ),
        ),
        (
            RoutingRole::Executor,
            ProductionRoleBinding::new(
                executor_route.clone(),
                RecordingProvider {
                    result: Ok(()),
                    calls: Arc::clone(&executor_calls),
                },
            ),
        ),
    ])
    .expect("dispatcher");

    let planner = dispatcher
        .invoke(
            invocation(
                RoutingRole::Planner,
                &planner_route,
                request("codex", "planner-model"),
            ),
            CancellationToken::new(),
        )
        .await
        .expect("planner result");
    let executor = dispatcher
        .invoke(
            invocation(
                RoutingRole::Executor,
                &executor_route,
                request("omniroute", "executor-model"),
            ),
            CancellationToken::new(),
        )
        .await
        .expect("executor result");

    assert_eq!(
        (planner.provider, planner.model),
        ("codex".to_string(), "planner-model".to_string())
    );
    assert_eq!(
        (executor.provider, executor.model),
        ("omniroute".to_string(), "executor-model".to_string())
    );
    assert_eq!(planner_calls.lock().expect("planner calls").len(), 1);
    assert_eq!(executor_calls.lock().expect("executor calls").len(), 1);
}

#[tokio::test]
async fn rejects_missing_and_mismatched_routes_without_fallback() {
    let route = route("codex-a", "codex", "model-a");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let dispatcher = ProductionRoleDispatcher::new([(
        RoutingRole::Planner,
        ProductionRoleBinding::new(
            route.clone(),
            RecordingProvider {
                result: Ok(()),
                calls: Arc::clone(&calls),
            },
        ),
    )])
    .expect("dispatcher");

    let missing = dispatcher
        .invoke(
            invocation(RoutingRole::Verifier, &route, request("codex", "model-a")),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(
        missing,
        Err(ProductionRoleDispatchError::MissingRole(
            RoutingRole::Verifier
        ))
    );

    let mismatch = dispatcher
        .invoke(
            invocation(
                RoutingRole::Planner,
                &route,
                request("openrouter", "model-a"),
            ),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(
        mismatch,
        Err(ProductionRoleDispatchError::ProviderMismatch {
            role: RoutingRole::Planner
        })
    );
    assert!(calls.lock().expect("calls").is_empty());
}

#[tokio::test]
async fn preserves_provider_failure_without_retry_or_fallback() {
    let route = route("openrouter-a", "openrouter", "model-a");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let dispatcher = ProductionRoleDispatcher::new([(
        RoutingRole::Verifier,
        ProductionRoleBinding::new(
            route.clone(),
            RecordingProvider {
                result: Err(ProviderInvocationError::Unauthorized),
                calls: Arc::clone(&calls),
            },
        ),
    )])
    .expect("dispatcher");

    let error = dispatcher
        .invoke(
            invocation(
                RoutingRole::Verifier,
                &route,
                request("openrouter", "model-a"),
            ),
            CancellationToken::new(),
        )
        .await
        .expect_err("provider failure");
    assert_eq!(
        error,
        ProductionRoleDispatchError::ProviderFailure {
            role: RoutingRole::Verifier,
            source: ProviderInvocationError::Unauthorized,
        }
    );
    assert_eq!(calls.lock().expect("calls").len(), 1);
    assert!(!error.to_string().contains("bounded-input"));
}

#[tokio::test]
async fn rejects_connection_account_model_and_effort_mismatches_before_invocation() {
    let route = route("codex-a", "codex", "model-a");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let dispatcher = ProductionRoleDispatcher::new([(
        RoutingRole::Executor,
        ProductionRoleBinding::new(
            route.clone(),
            RecordingProvider {
                result: Ok(()),
                calls: Arc::clone(&calls),
            },
        ),
    )])
    .expect("dispatcher");

    let mut wrong_connection =
        invocation(RoutingRole::Executor, &route, request("codex", "model-a"));
    wrong_connection.connection_id = "codex-b".to_string();
    assert_eq!(
        dispatcher
            .invoke(wrong_connection, CancellationToken::new())
            .await,
        Err(ProductionRoleDispatchError::ConnectionMismatch {
            role: RoutingRole::Executor
        })
    );

    let mut wrong_account = invocation(RoutingRole::Executor, &route, request("codex", "model-a"));
    wrong_account.account_id = Some("account-b".to_string());
    assert_eq!(
        dispatcher
            .invoke(wrong_account, CancellationToken::new())
            .await,
        Err(ProductionRoleDispatchError::AccountMismatch {
            role: RoutingRole::Executor
        })
    );

    let mut wrong_model = invocation(RoutingRole::Executor, &route, request("codex", "model-a"));
    wrong_model.request.model = "model-b".to_string();
    assert_eq!(
        dispatcher
            .invoke(wrong_model, CancellationToken::new())
            .await,
        Err(ProductionRoleDispatchError::ModelMismatch {
            role: RoutingRole::Executor
        })
    );

    let mut wrong_effort = invocation(RoutingRole::Executor, &route, request("codex", "model-a"));
    wrong_effort.effort = ReasoningEffort::High;
    assert_eq!(
        dispatcher
            .invoke(wrong_effort, CancellationToken::new())
            .await,
        Err(ProductionRoleDispatchError::EffortMismatch {
            role: RoutingRole::Executor
        })
    );
    assert!(calls.lock().expect("calls").is_empty());
}

#[test]
fn rejects_duplicate_roles_and_keeps_routes_immutable() {
    let first = route("codex-a", "codex", "model-a");
    let second = route("codex-b", "codex", "model-b");
    let dispatcher = ProductionRoleDispatcher::new([
        (
            RoutingRole::Repair,
            ProductionRoleBinding::new(
                first,
                RecordingProvider {
                    result: Ok(()),
                    calls: Arc::new(Mutex::new(Vec::new())),
                },
            ),
        ),
        (
            RoutingRole::Repair,
            ProductionRoleBinding::new(
                second,
                RecordingProvider {
                    result: Ok(()),
                    calls: Arc::new(Mutex::new(Vec::new())),
                },
            ),
        ),
    ]);
    assert_eq!(
        dispatcher.err(),
        Some(ProductionRoleDispatchError::DuplicateRole)
    );
}
