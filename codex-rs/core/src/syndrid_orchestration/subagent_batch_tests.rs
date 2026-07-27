use super::invocation::ProviderInvocationError;
use super::invocation::ProviderInvocationRequest;
use super::invocation::ProviderInvocationResult;
use super::provider_connection::ConnectionValidationStatus;
use super::routing_profiles::RoutingAssignment;
use super::routing_profiles::RoutingConnectionDirectory;
use super::routing_profiles::RoutingConnectionInfo;
use super::routing_profiles::RoutingProfile;
use super::routing_profiles::RoutingProfileId;
use super::routing_profiles::RoutingProfileRegistry;
use super::routing_profiles::RoutingRole;
use super::subagent::SUBAGENT_DEFAULT_OUTPUT_TOKENS;
use super::subagent::SUBAGENT_DEFAULT_TIMEOUT;
use super::subagent::SubagentProvider;
use super::subagent::SubagentRequest;
use super::subagent::SubagentRuntime;
use super::subagent_batch::SubagentBatchError;
use super::subagent_batch::SubagentBatchRequest;
use super::subagent_batch::SubagentBatchRuntime;
use super::subagent_batch::SubagentConcurrencyPolicy;
use super::subagent_batch::SubagentFailurePolicy;
use super::subagent_batch::SubagentTask;
use super::subagent_tools::SubagentToolPolicy;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct MockProvider {
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
    started_count: Arc<AtomicUsize>,
    started: Arc<Notify>,
    release: Arc<Notify>,
    released: Arc<AtomicBool>,
    wait: bool,
    fail: bool,
}

impl MockProvider {
    fn new(wait: bool, fail: bool) -> Self {
        Self {
            active: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            calls: Arc::new(AtomicUsize::new(0)),
            started_count: Arc::new(AtomicUsize::new(0)),
            started: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
            released: Arc::new(AtomicBool::new(false)),
            wait,
            fail,
        }
    }
}

impl SubagentProvider for MockProvider {
    async fn invoke(
        &self,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> Result<ProviderInvocationResult, ProviderInvocationError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(active, Ordering::SeqCst);
        self.started_count.fetch_add(1, Ordering::SeqCst);
        self.started.notify_one();
        if self.wait {
            while !self.released.load(Ordering::SeqCst) {
                tokio::select! {
                    _ = cancellation.cancelled() => return Err(ProviderInvocationError::Cancelled),
                    _ = self.release.notified() => {}
                }
            }
        }
        self.active.fetch_sub(1, Ordering::SeqCst);
        if self.fail {
            return Err(ProviderInvocationError::ProviderRejected);
        }
        Ok(ProviderInvocationResult {
            provider: request.provider,
            model: request.model,
            text: "bounded result".to_string(),
            finish_reason: None,
            usage: None,
            request_id: None,
            tool_call: None,
        })
    }
}

fn runtime(provider: MockProvider) -> SubagentBatchRuntime<MockProvider> {
    let profile_id = RoutingProfileId::new("active").unwrap();
    let mut profile = RoutingProfile::new(profile_id.clone(), "Active", 1).unwrap();
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
    ] {
        profile
            .assign(
                role,
                RoutingAssignment {
                    connection_id: "test-connection".to_string(),
                    provider_id: "codex".to_string(),
                    model_id: "test-model".to_string(),
                    enabled: true,
                    label: None,
                },
            )
            .unwrap();
    }
    let mut profiles = RoutingProfileRegistry::default();
    profiles.insert(profile).unwrap();
    profiles.activate(&profile_id).unwrap();
    let mut directory = RoutingConnectionDirectory::default();
    directory.insert(RoutingConnectionInfo {
        connection_id: "test-connection".to_string(),
        provider_id: "codex".to_string(),
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        authentication_supported: true,
        models: Some(vec!["test-model".to_string()]),
    });
    SubagentBatchRuntime::new(SubagentRuntime::new(provider, profiles, directory))
}

fn task(index: usize) -> SubagentTask {
    SubagentTask {
        request: SubagentRequest {
            task_id: format!("task-{index}"),
            parent_id: None,
            role: RoutingRole::Planner,
            instruction: format!("instruction-{index}"),
            context: None,
            timeout: SUBAGENT_DEFAULT_TIMEOUT,
            max_output_tokens: SUBAGENT_DEFAULT_OUTPUT_TOKENS,
            cancellation: CancellationToken::new(),
            depth: 1,
            tool_policy: SubagentToolPolicy::empty(),
            budget: None,
        },
        timeout_override: None,
    }
}

fn batch(tasks: Vec<SubagentTask>, policy: SubagentConcurrencyPolicy) -> SubagentBatchRequest {
    SubagentBatchRequest {
        tasks,
        policy,
        cancellation: CancellationToken::new(),
    }
}

#[tokio::test]
async fn empty_batch_is_rejected_before_execution() {
    assert_eq!(
        runtime(MockProvider::new(false, false))
            .run(batch(Vec::new(), SubagentConcurrencyPolicy::default()))
            .await,
        Err(SubagentBatchError::EmptyBatch)
    );
}

#[tokio::test]
async fn invalid_policy_and_duplicate_ids_start_zero_tasks() {
    let mut policy = SubagentConcurrencyPolicy::default();
    policy.max_concurrency = 0;
    let provider = MockProvider::new(false, false);
    let calls = provider.calls.clone();
    assert_eq!(
        runtime(provider).run(batch(vec![task(0)], policy)).await,
        Err(SubagentBatchError::InvalidPolicy)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let provider = MockProvider::new(false, false);
    let calls = provider.calls.clone();
    let mut duplicate = task(0);
    duplicate.request.instruction = "different".to_string();
    assert_eq!(
        runtime(provider)
            .run(batch(
                vec![task(0), duplicate],
                SubagentConcurrencyPolicy::default(),
            ))
            .await,
        Err(SubagentBatchError::DuplicateTaskId)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn concurrency_two_reaches_two_and_results_are_in_input_order() {
    let provider = MockProvider::new(true, false);
    let started = provider.started.clone();
    let started_count = provider.started_count.clone();
    let release = provider.release.clone();
    let released = provider.released.clone();
    let peak = provider.peak.clone();
    let mut policy = SubagentConcurrencyPolicy::default();
    policy.max_concurrency = 2;
    let run = tokio::spawn(async move {
        runtime(provider)
            .run(batch((0..4).map(task).collect(), policy))
            .await
            .unwrap()
    });
    while started_count.load(Ordering::SeqCst) < 2 {
        started.notified().await;
    }
    released.store(true, Ordering::SeqCst);
    release.notify_waiters();
    let outcome = run.await.unwrap();
    assert_eq!(peak.load(Ordering::SeqCst), 2);
    assert_eq!(
        outcome
            .tasks
            .iter()
            .map(|task| task.task_id.as_str())
            .collect::<Vec<_>>(),
        vec!["task-0", "task-1", "task-2", "task-3"]
    );
}

#[tokio::test]
async fn cancellation_prevents_queued_tasks_and_leaves_no_active_provider_calls() {
    let provider = MockProvider::new(true, false);
    let started = provider.started.clone();
    let started_count = provider.started_count.clone();
    let cancellation = CancellationToken::new();
    let mut policy = SubagentConcurrencyPolicy::default();
    policy.max_concurrency = 1;
    let run = tokio::spawn({
        let cancellation = cancellation.clone();
        async move {
            runtime(provider)
                .run(SubagentBatchRequest {
                    tasks: (0..3).map(task).collect(),
                    policy,
                    cancellation,
                })
                .await
                .unwrap()
        }
    });
    while started_count.load(Ordering::SeqCst) < 1 {
        started.notified().await;
    }
    cancellation.cancel();
    let outcome = run.await.unwrap();
    assert!(outcome.not_started_task_count >= 1);
}

#[tokio::test]
async fn cancel_remaining_stops_after_first_failure_without_retry() {
    let provider = MockProvider::new(false, true);
    let calls = provider.calls.clone();
    let mut policy = SubagentConcurrencyPolicy::default();
    policy.max_concurrency = 1;
    policy.failure_policy = SubagentFailurePolicy::CancelRemaining;
    let outcome = runtime(provider)
        .run(batch((0..3).map(task).collect(), policy))
        .await
        .unwrap();
    assert_eq!(outcome.failed_task_count, 1);
    assert_eq!(outcome.not_started_task_count, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn exact_route_is_retained_per_task() {
    let outcome = runtime(MockProvider::new(false, false))
        .run(batch(vec![task(0)], SubagentConcurrencyPolicy::default()))
        .await
        .unwrap();
    let result = outcome.tasks[0].outcome.as_ref().unwrap();
    assert_eq!(result.role, RoutingRole::Planner);
    assert_eq!(result.profile_id, "active");
    assert_eq!(result.provider_id, "codex");
    assert_eq!(result.connection_id, "test-connection");
    assert_eq!(result.model_id, "test-model");
}
