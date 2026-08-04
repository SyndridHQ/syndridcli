use super::*;
use crate::syndrid_orchestration::ConnectionValidationStatus;
use crate::syndrid_orchestration::ProviderInvocationToolCall;
use crate::syndrid_orchestration::ProviderInvocationUsage;
use crate::syndrid_orchestration::RoutingAssignment;
use crate::syndrid_orchestration::RoutingConnectionInfo;
use crate::syndrid_orchestration::RoutingProfile;
use crate::syndrid_orchestration::RoutingProfileId;
use crate::syndrid_orchestration::SubagentSessionBudget;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tempfile::tempdir;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct MockProvider {
    calls: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<ProviderInvocationRequest>>>,
    response: Arc<Mutex<MockResponse>>,
    started: Arc<Notify>,
}

struct MockResponse {
    delay: Duration,
    result: Result<ProviderInvocationResult, ProviderInvocationError>,
}

impl MockProvider {
    fn successful() -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            requests: Arc::new(Mutex::new(Vec::new())),
            response: Arc::new(Mutex::new(MockResponse {
                delay: Duration::ZERO,
                result: Ok(ProviderInvocationResult {
                    provider: "codex".to_string(),
                    model: "gpt-test".to_string(),
                    text: "bounded result".to_string(),
                    finish_reason: Some("stop".to_string()),
                    usage: Some(ProviderInvocationUsage {
                        input_tokens: Some(11),
                        output_tokens: Some(7),
                        total_tokens: Some(18),
                    }),
                    request_id: None,
                    tool_call: None,
                }),
            })),
            started: Arc::new(Notify::new()),
        }
    }

    async fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    async fn requests(&self) -> Vec<ProviderInvocationRequest> {
        self.requests.lock().await.clone()
    }
}

impl SubagentProvider for MockProvider {
    async fn invoke(
        &self,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> Result<ProviderInvocationResult, ProviderInvocationError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().await.push(request);
        self.started.notify_one();
        let response = self.response.lock().await;
        if !response.delay.is_zero() {
            tokio::select! {
                _ = cancellation.cancelled() => return Err(ProviderInvocationError::Cancelled),
                _ = sleep(response.delay) => {}
            }
        }
        response.result.clone()
    }
}

fn assignment(connection_id: &str, provider_id: &str, model_id: &str) -> RoutingAssignment {
    RoutingAssignment {
        connection_id: connection_id.to_string(),
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        enabled: true,
        label: None,
        pool_id: None,
    }
}

fn profile_registry() -> RoutingProfileRegistry {
    let id = RoutingProfileId::new("active").unwrap();
    let mut profile = RoutingProfile::new(id.clone(), "Active", 1).unwrap();
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
    ] {
        profile
            .assign(role, assignment("codex-secondary", "codex", "gpt-test"))
            .unwrap();
    }
    let mut registry = RoutingProfileRegistry::default();
    registry.insert(profile).unwrap();
    registry.activate(&id).unwrap();
    registry
}

fn directory(
    validation: ConnectionValidationStatus,
    models: Option<Vec<String>>,
) -> RoutingConnectionDirectory {
    let mut directory = RoutingConnectionDirectory::default();
    directory.insert(RoutingConnectionInfo {
        connection_id: "codex-secondary".to_string(),
        provider_id: "codex".to_string(),
        enabled: true,
        validation,
        authentication_supported: true,
        models,
    });
    directory
}

fn request(role: RoutingRole) -> SubagentRequest {
    SubagentRequest {
        task_id: "task-1".to_string(),
        parent_id: Some("parent-1".to_string()),
        role,
        instruction: "Return a bounded result.".to_string(),
        context: Some("Only this context.".to_string()),
        timeout: SUBAGENT_DEFAULT_TIMEOUT,
        max_output_tokens: SUBAGENT_DEFAULT_OUTPUT_TOKENS,
        cancellation: CancellationToken::new(),
        depth: 1,
        tool_policy: SubagentToolPolicy::empty(),
        budget: None,
        cleanup: None,
    }
}

#[derive(Clone)]
struct ToolLoopProvider {
    requests: Arc<Mutex<Vec<ProviderInvocationRequest>>>,
    responses: Arc<Mutex<VecDeque<Result<ProviderInvocationResult, ProviderInvocationError>>>>,
}

impl ToolLoopProvider {
    fn new(responses: Vec<Result<ProviderInvocationResult, ProviderInvocationError>>) -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
        }
    }

    async fn requests(&self) -> Vec<ProviderInvocationRequest> {
        self.requests.lock().await.clone()
    }
}

impl SubagentProvider for ToolLoopProvider {
    async fn invoke(
        &self,
        request: ProviderInvocationRequest,
        _cancellation: CancellationToken,
    ) -> Result<ProviderInvocationResult, ProviderInvocationError> {
        self.requests.lock().await.push(request);
        self.responses
            .lock()
            .await
            .pop_front()
            .unwrap_or(Err(ProviderInvocationError::InvalidResponse))
    }
}

fn provider_result(
    text: &str,
    tool_call: Option<ProviderInvocationToolCall>,
) -> ProviderInvocationResult {
    ProviderInvocationResult {
        provider: "codex".to_string(),
        model: "gpt-test".to_string(),
        text: text.to_string(),
        finish_reason: Some("stop".to_string()),
        usage: None,
        request_id: None,
        tool_call,
    }
}

#[tokio::test]
async fn invokes_exact_role_connection_and_model_once() {
    let provider = MockProvider::successful();
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );

    let outcome = runtime
        .run_subagent(request(RoutingRole::Planner))
        .await
        .unwrap();

    assert_eq!(outcome.status, SubagentStatus::Completed);
    assert_eq!(outcome.provider_id, "codex");
    assert_eq!(outcome.connection_id, "codex-secondary");
    assert_eq!(outcome.model_id, "gpt-test");
    assert_eq!(provider.calls().await, 1);
    let requests = provider.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].provider, "codex");
    assert_eq!(requests[0].model, "gpt-test");
    assert!(requests[0].user.contains("Only this context."));
    assert!(requests[0].system.as_deref().unwrap().contains("no tools"));
}

#[tokio::test]
async fn validation_failures_make_zero_invocations() {
    let provider = MockProvider::successful();
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["other-model".to_string()]),
        ),
    );
    let mut request = request(RoutingRole::Planner);
    request.instruction.clear();

    assert_eq!(
        runtime.run_subagent(request).await,
        Err(SubagentError::EmptyInstruction)
    );
    assert_eq!(provider.calls().await, 0);
}

#[tokio::test]
async fn model_unverified_is_rejected_before_invocation() {
    let provider = MockProvider::successful();
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(ConnectionValidationStatus::Valid, None),
    );

    assert_eq!(
        runtime.run_subagent(request(RoutingRole::Executor)).await,
        Err(SubagentError::ModelUnverified)
    );
    assert_eq!(provider.calls().await, 0);
}

#[tokio::test]
async fn no_active_profile_fails_before_invocation() {
    let provider = MockProvider::successful();
    let runtime = SubagentRuntime::new(
        provider.clone(),
        RoutingProfileRegistry::default(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );

    assert_eq!(
        runtime.run_subagent(request(RoutingRole::Planner)).await,
        Err(SubagentError::NoActiveProfile)
    );
    assert_eq!(provider.calls().await, 0);
}

#[tokio::test]
async fn recursion_above_one_fails_before_invocation() {
    let provider = MockProvider::successful();
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );
    let mut request = request(RoutingRole::Planner);
    request.depth = 2;

    assert_eq!(
        runtime.run_subagent(request).await,
        Err(SubagentError::RecursionNotAllowed)
    );
    assert_eq!(provider.calls().await, 0);
}

#[tokio::test]
async fn oversized_context_fails_before_invocation() {
    let provider = MockProvider::successful();
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );
    let mut request = request(RoutingRole::Planner);
    request.context = Some("x".repeat(SUBAGENT_MAX_CONTEXT_BYTES + 1));

    assert_eq!(
        runtime.run_subagent(request).await,
        Err(SubagentError::ContextTooLarge)
    );
    assert_eq!(provider.calls().await, 0);
}

#[tokio::test]
async fn cancellation_before_start_makes_zero_invocations() {
    let provider = MockProvider::successful();
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let mut request = request(RoutingRole::Verifier);
    request.cancellation = cancellation;
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );

    assert_eq!(
        runtime.run_subagent(request).await,
        Err(SubagentError::CancelledBeforeStart)
    );
    assert_eq!(provider.calls().await, 0);
}

#[tokio::test]
async fn provider_failure_is_single_shot() {
    let provider = MockProvider::successful();
    provider.response.lock().await.result = Err(ProviderInvocationError::ProviderRejected);
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );

    let outcome = runtime
        .run_subagent(request(RoutingRole::Planner))
        .await
        .unwrap();
    assert_eq!(outcome.status, SubagentStatus::ProviderRejected);
    assert_eq!(outcome.profile_id, "active");
    assert_eq!(outcome.provider_id, "codex");
    assert_eq!(outcome.connection_id, "codex-secondary");
    assert_eq!(outcome.model_id, "gpt-test");
    assert_eq!(outcome.lifecycle.last(), Some(&SubagentLifecycle::Failed));
    assert_eq!(provider.calls().await, 1);
}

#[tokio::test]
async fn usage_limited_provider_failure_does_not_cycle_accounts() {
    let provider = MockProvider::successful();
    provider.response.lock().await.result = Err(ProviderInvocationError::RateLimited);
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );

    let outcome = runtime
        .run_subagent(request(RoutingRole::Planner))
        .await
        .unwrap();

    assert_eq!(outcome.status, SubagentStatus::ProviderRejected);
    assert_eq!(provider.calls().await, 1);
}

#[tokio::test]
async fn provider_output_is_plain_text_without_second_invocation() {
    let provider = MockProvider::successful();
    provider.response.lock().await.result.as_mut().unwrap().text =
        "spawn_subagent(task_id=unexpected)".to_string();
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );

    let outcome = runtime
        .run_subagent(request(RoutingRole::Planner))
        .await
        .unwrap();

    assert_eq!(outcome.status, SubagentStatus::Completed);
    assert_eq!(
        outcome.output.as_deref(),
        Some("spawn_subagent(task_id=unexpected)")
    );
    assert_eq!(provider.calls().await, 1);
}

#[tokio::test]
async fn output_limit_is_utf8_safe_and_terminal() {
    let provider = MockProvider::successful();
    provider.response.lock().await.result.as_mut().unwrap().text = "😀abc".to_string();
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );
    let mut request = request(RoutingRole::Planner);
    request.max_output_tokens = 1;

    let outcome = runtime.run_subagent(request).await.unwrap();

    assert_eq!(outcome.status, SubagentStatus::OutputLimitReached);
    assert_eq!(outcome.output.as_deref(), Some("😀"));
    assert_eq!(outcome.lifecycle.last(), Some(&SubagentLifecycle::Failed));
    assert_eq!(provider.calls().await, 1);
}

#[tokio::test]
async fn timeout_is_terminal_after_one_invocation() {
    let provider = MockProvider::successful();
    provider.response.lock().await.delay = Duration::from_secs(2);
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );
    let mut request = request(RoutingRole::Planner);
    request.timeout = Duration::from_secs(1);

    let outcome = runtime.run_subagent(request).await.unwrap();
    assert_eq!(outcome.status, SubagentStatus::TimedOut);
    assert_eq!(outcome.lifecycle.last(), Some(&SubagentLifecycle::TimedOut));
    assert!(!matches!(
        outcome.lifecycle.last(),
        Some(SubagentLifecycle::Running)
    ));
    assert_eq!(provider.calls().await, 1);
}

#[tokio::test]
async fn cancellation_during_invocation_is_terminal_after_one_attempt() {
    let provider = MockProvider::successful();
    provider.response.lock().await.delay = Duration::from_secs(120);
    let cancellation = CancellationToken::new();
    let mut request = request(RoutingRole::Planner);
    request.cancellation = cancellation.clone();
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );
    let started = provider.started.clone();
    let task = tokio::spawn(async move { runtime.run_subagent(request).await });
    started.notified().await;
    cancellation.cancel();

    let outcome = task.await.unwrap().unwrap();
    assert_eq!(outcome.status, SubagentStatus::Cancelled);
    assert_eq!(
        outcome.lifecycle.last(),
        Some(&SubagentLifecycle::Cancelled)
    );
    assert_eq!(provider.calls().await, 1);
}

#[tokio::test]
async fn debug_request_does_not_expose_prompt_material() {
    let mut request = request(RoutingRole::Planner);
    request.instruction = "instruction-secret".to_string();
    request.context = Some("context-secret".to_string());
    let debug = format!("{request:?}");
    assert!(!debug.contains("instruction-secret"));
    assert!(!debug.contains("context-secret"));
}

#[tokio::test]
async fn debug_formatting_does_not_expose_secret_sentinels() {
    let secret = "O6A-SECRET-SENTINEL";
    let mut request = request(RoutingRole::Planner);
    request.task_id = secret.to_string();
    request.parent_id = Some(secret.to_string());
    request.instruction = secret.to_string();
    request.context = Some(secret.to_string());
    let request_debug = format!("{request:?}");

    let outcome = SubagentOutcome {
        task_id: secret.to_string(),
        parent_id: Some(secret.to_string()),
        role: RoutingRole::Planner,
        profile_id: secret.to_string(),
        provider_id: secret.to_string(),
        connection_id: secret.to_string(),
        model_id: secret.to_string(),
        status: SubagentStatus::ProviderRejected,
        output: Some(secret.to_string()),
        usage: None,
        latency_ms: 0,
        lifecycle: vec![SubagentLifecycle::Failed],
        warnings: vec![secret.to_string()],
        provider_turns: 1,
        tool_calls: 0,
        tool_call_counts: BTreeMap::new(),
        tool_audit: Vec::new(),
        output_truncated: false,
        budget_exhausted: false,
    };
    let outcome_debug = format!("{outcome:?}");
    let error_debug = format!("{:?}", SubagentError::ProviderRejected);
    let lifecycle_debug = format!("{:?}", SubagentLifecycle::Failed);

    assert!(!request_debug.contains(secret));
    assert!(!outcome_debug.contains(secret));
    assert!(!error_debug.contains(secret));
    assert!(!lifecycle_debug.contains(secret));
}

#[tokio::test]
async fn empty_tool_policy_preserves_tool_free_completion() {
    let provider = MockProvider::successful();
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );

    let outcome = runtime
        .run_subagent(request(RoutingRole::Planner))
        .await
        .unwrap();
    let requests = provider.requests().await;

    assert_eq!(outcome.provider_turns, 1);
    assert_eq!(outcome.tool_calls, 0);
    assert!(requests[0].tools.is_empty());
    assert!(requests[0].tool_results.is_empty());
}

#[tokio::test]
async fn approved_read_file_round_trip_retains_exact_route() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("note.txt"), "approved content").unwrap();
    let policy =
        SubagentToolPolicy::for_workspace(workspace.path(), SubagentSessionBudget::default())
            .unwrap()
            .approve(SubagentToolKind::ReadFile);
    let provider = ToolLoopProvider::new(vec![
        Ok(provider_result(
            "",
            Some(ProviderInvocationToolCall {
                id: "call-1".to_string(),
                name: "read_file".to_string(),
                arguments: r#"{"path":"note.txt"}"#.to_string(),
            }),
        )),
        Ok(provider_result("final result", None)),
    ]);
    let mut request = request(RoutingRole::Planner);
    request.tool_policy = policy;
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );

    let outcome = runtime.run_subagent(request).await.unwrap();
    let requests = provider.requests().await;

    assert_eq!(outcome.status, SubagentStatus::Completed);
    assert_eq!(outcome.provider_turns, 2);
    assert_eq!(outcome.tool_calls, 1);
    assert_eq!(outcome.tool_call_counts[&SubagentToolKind::ReadFile], 1);
    assert_eq!(outcome.output.as_deref(), Some("final result"));
    assert_eq!(outcome.provider_id, "codex");
    assert_eq!(outcome.connection_id, "codex-secondary");
    assert_eq!(outcome.model_id, "gpt-test");
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].tools[0].name, "read_file");
    assert_eq!(requests[1].provider, "codex");
    assert_eq!(requests[1].model, "gpt-test");
    assert!(
        requests[1].tool_results[0]
            .content
            .contains("approved content")
    );
}

#[tokio::test]
async fn unknown_unapproved_and_malformed_tool_requests_execute_nothing() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("note.txt"), "content").unwrap();
    for (name, arguments) in [
        ("unknown_tool", "{}"),
        ("read_file", r#"{"path":"note.txt","extra":true}"#),
    ] {
        let policy =
            SubagentToolPolicy::for_workspace(workspace.path(), SubagentSessionBudget::default())
                .unwrap();
        let provider = ToolLoopProvider::new(vec![
            Ok(provider_result(
                "",
                Some(ProviderInvocationToolCall {
                    id: "call-1".to_string(),
                    name: name.to_string(),
                    arguments: arguments.to_string(),
                }),
            )),
            Ok(provider_result("safe final", None)),
        ]);
        let mut request = request(RoutingRole::Planner);
        request.tool_policy = policy;
        let runtime = SubagentRuntime::new(
            provider.clone(),
            profile_registry(),
            directory(
                ConnectionValidationStatus::Valid,
                Some(vec!["gpt-test".to_string()]),
            ),
        );

        let outcome = runtime.run_subagent(request).await.unwrap();
        assert_eq!(outcome.status, SubagentStatus::Completed);
        assert_eq!(outcome.tool_calls, 1);
        assert!(!outcome.tool_audit[0].succeeded);
        assert_eq!(provider.requests().await.len(), 2);
    }
}

#[tokio::test]
async fn plain_text_resembling_tool_request_is_not_executed() {
    let provider = ToolLoopProvider::new(vec![Ok(provider_result(
        "call read_file {path: note.txt}",
        None,
    ))]);
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );

    let outcome = runtime
        .run_subagent(request(RoutingRole::Planner))
        .await
        .unwrap();

    assert_eq!(outcome.status, SubagentStatus::Completed);
    assert_eq!(outcome.tool_calls, 0);
    assert_eq!(provider.requests().await.len(), 1);
}

#[tokio::test]
async fn provider_failure_after_tool_use_has_no_retry_or_fallback() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("note.txt"), "content").unwrap();
    let policy =
        SubagentToolPolicy::for_workspace(workspace.path(), SubagentSessionBudget::default())
            .unwrap()
            .approve(SubagentToolKind::ReadFile);
    let provider = ToolLoopProvider::new(vec![
        Ok(provider_result(
            "",
            Some(ProviderInvocationToolCall {
                id: "call-1".to_string(),
                name: "read_file".to_string(),
                arguments: r#"{"path":"note.txt"}"#.to_string(),
            }),
        )),
        Err(ProviderInvocationError::ProviderRejected),
    ]);
    let mut request = request(RoutingRole::Planner);
    request.tool_policy = policy;
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );

    let outcome = runtime.run_subagent(request).await.unwrap();

    assert_eq!(outcome.status, SubagentStatus::ProviderRejected);
    assert_eq!(outcome.provider_turns, 2);
    assert_eq!(provider.requests().await.len(), 2);
}

#[tokio::test]
async fn provider_turn_budget_ends_before_extra_tool_execution() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("note.txt"), "content").unwrap();
    let mut budget = SubagentSessionBudget::default();
    budget.max_provider_turns = 1;
    let policy = SubagentToolPolicy::for_workspace(workspace.path(), budget)
        .unwrap()
        .approve(SubagentToolKind::ReadFile);
    let provider = ToolLoopProvider::new(vec![Ok(provider_result(
        "",
        Some(ProviderInvocationToolCall {
            id: "call-1".to_string(),
            name: "read_file".to_string(),
            arguments: r#"{"path":"note.txt"}"#.to_string(),
        }),
    ))]);
    let mut request = request(RoutingRole::Planner);
    request.tool_policy = policy;
    let runtime = SubagentRuntime::new(
        provider.clone(),
        profile_registry(),
        directory(
            ConnectionValidationStatus::Valid,
            Some(vec!["gpt-test".to_string()]),
        ),
    );

    let outcome = runtime.run_subagent(request).await.unwrap();

    assert_eq!(outcome.status, SubagentStatus::BudgetExhausted);
    assert!(outcome.budget_exhausted);
    assert_eq!(outcome.provider_turns, 1);
    assert_eq!(provider.requests().await.len(), 1);
}
