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
use super::subagent::SubagentProvider;
use super::subagent::SubagentRequest;
use super::subagent::SubagentRuntime;
use super::subagent_batch::SubagentFailurePolicy;
use super::subagent_repair::*;
use super::subagent_tools::SubagentToolPolicy;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct Provider {
    calls: Arc<AtomicUsize>,
    reject_first: bool,
    reject_all: bool,
    reject_initial: bool,
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    started: Arc<Notify>,
    requests: Arc<Mutex<Vec<ProviderInvocationRequest>>>,
    block_after_first: bool,
    tool_call_after_first: bool,
    repair_release: Option<Arc<Notify>>,
    reject_instruction: Option<String>,
}

struct ActiveProviderCall(Arc<AtomicUsize>);

impl Drop for ActiveProviderCall {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl SubagentProvider for Provider {
    async fn invoke(
        &self,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> Result<ProviderInvocationResult, ProviderInvocationError> {
        if cancellation.is_cancelled() {
            return Err(ProviderInvocationError::Cancelled);
        }
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push(request.clone());
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        let _active_call = ActiveProviderCall(self.active.clone());
        self.peak.fetch_max(active, Ordering::SeqCst);
        self.started.notify_waiters();
        let rejected = self.reject_all
            || (self.reject_first && call == 0)
            || (self.reject_initial
                && request
                    .system
                    .as_deref()
                    .is_some_and(|system| system.to_ascii_lowercase().contains("planner")))
            || self
                .reject_instruction
                .as_deref()
                .is_some_and(|instruction| request.user.contains(instruction));
        let result = if rejected {
            Err(ProviderInvocationError::ProviderRejected)
        } else if self.block_after_first
            && call > 0
            && request
                .system
                .as_deref()
                .is_some_and(|system| system.to_ascii_lowercase().contains("repair"))
        {
            if let Some(release) = &self.repair_release {
                tokio::select! {
                    _ = cancellation.cancelled() => return Err(ProviderInvocationError::Cancelled),
                    _ = release.notified() => return Ok(ProviderInvocationResult {
                        provider: request.provider,
                        model: request.model,
                        text: "accepted repair result".to_string(),
                        finish_reason: None,
                        usage: None,
                        request_id: None,
                        tool_call: None,
                    }),
                }
            } else {
                cancellation.cancelled().await;
            }
            Err(ProviderInvocationError::Cancelled)
        } else if self.tool_call_after_first && call > 0 {
            Ok(ProviderInvocationResult {
                provider: request.provider,
                model: request.model,
                text: String::new(),
                finish_reason: None,
                usage: None,
                request_id: None,
                tool_call: Some(super::invocation::ProviderInvocationToolCall {
                    id: "repair-tool-call".to_string(),
                    name: "read_file".to_string(),
                    arguments: "{\"path\":\"Cargo.toml\"}".to_string(),
                }),
            })
        } else {
            Ok(ProviderInvocationResult {
                provider: request.provider,
                model: request.model,
                text: "accepted repair result".to_string(),
                finish_reason: None,
                usage: None,
                request_id: None,
                tool_call: None,
            })
        };
        result
    }
}

fn provider_state() -> (
    Arc<AtomicUsize>,
    Arc<AtomicUsize>,
    Arc<AtomicUsize>,
    Arc<Notify>,
    Arc<Mutex<Vec<ProviderInvocationRequest>>>,
) {
    (
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
        Arc::new(Notify::new()),
        Arc::new(Mutex::new(Vec::new())),
    )
}

async fn wait_for_calls(calls: &AtomicUsize, started: &Notify, expected: usize) {
    while calls.load(Ordering::SeqCst) < expected {
        let notified = started.notified();
        if calls.load(Ordering::SeqCst) < expected {
            notified.await;
        }
    }
}

fn provider_with(
    calls: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    started: Arc<Notify>,
    requests: Arc<Mutex<Vec<ProviderInvocationRequest>>>,
    reject_first: bool,
    reject_all: bool,
    reject_initial: bool,
    block_after_first: bool,
    tool_call_after_first: bool,
    repair_release: Option<Arc<Notify>>,
    reject_instruction: Option<String>,
) -> Provider {
    Provider {
        calls,
        reject_first,
        reject_all,
        reject_initial,
        active,
        peak,
        started,
        requests,
        block_after_first,
        tool_call_after_first,
        repair_release,
        reject_instruction,
    }
}

fn runtime(provider: Provider) -> SubagentRuntime<Provider> {
    let id = RoutingProfileId::new("profile").unwrap();
    let mut profile = RoutingProfile::new(id.clone(), "Profile", 1).unwrap();
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ] {
        profile
            .assign(
                role,
                RoutingAssignment {
                    connection_id: "connection".to_string(),
                    provider_id: "codex".to_string(),
                    model_id: "model".to_string(),
                    enabled: true,
                    label: None,
                    pool_id: None,
                },
            )
            .unwrap();
    }
    let mut profiles = RoutingProfileRegistry::default();
    profiles.insert(profile).unwrap();
    profiles.activate(&id).unwrap();
    let mut directory = RoutingConnectionDirectory::default();
    directory.insert(RoutingConnectionInfo {
        connection_id: "connection".to_string(),
        provider_id: "codex".to_string(),
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        authentication_supported: true,
        models: Some(vec!["model".to_string()]),
    });
    SubagentRuntime::new(provider, profiles, directory)
}

fn request(cancellation: CancellationToken) -> SubagentRequest {
    SubagentRequest {
        task_id: "task".to_string(),
        parent_id: None,
        role: RoutingRole::Planner,
        instruction: "perform bounded task".to_string(),
        context: None,
        timeout: super::subagent::SUBAGENT_DEFAULT_TIMEOUT,
        max_output_tokens: super::subagent::SUBAGENT_DEFAULT_OUTPUT_TOKENS,
        cancellation,
        depth: 1,
        tool_policy: SubagentToolPolicy::empty(),
        budget: None,
        cleanup: None,
    }
}

fn policy(enabled: bool) -> SubagentRepairPolicy {
    SubagentRepairPolicy {
        enabled,
        max_repair_attempts: 1,
        route: SubagentRepairRoute {
            profile_id: "profile".to_string(),
            role: RoutingRole::Repair,
            provider_id: "codex".to_string(),
            connection_id: "connection".to_string(),
            model_id: "model".to_string(),
        },
        per_repair_timeout: Duration::from_secs(2),
        total_repair_timeout: Duration::from_secs(2),
        max_provider_invocations: 1,
        max_tool_calls: 1,
        max_context_bytes: 1024,
        max_output_tokens: 100,
    }
}

use std::time::Duration;

#[tokio::test]
async fn disabled_repair_has_one_initial_attempt_and_no_repair_call() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = SubagentRepairRuntime::new(
        runtime(Provider {
            calls: calls.clone(),
            reject_first: true,
            reject_all: false,
            reject_initial: false,
            active: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            started: Arc::new(Notify::new()),
            requests: Arc::new(Mutex::new(Vec::new())),
            block_after_first: false,
            tool_call_after_first: false,
            repair_release: None,
            reject_instruction: None,
        }),
        SubagentRepairBudget::new(1, 1, 1024, 100).unwrap(),
    );
    let outcome = runtime
        .run(
            request(CancellationToken::new()),
            policy(false),
            SubagentRepairEligibility::Eligible(SubagentRepairFailureCategory::VerifierRejected),
            "rejected".to_string(),
            "repair".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.terminal, SubagentRepairTerminal::RepairDisabled);
    assert_eq!(outcome.attempts.len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn successful_repair_is_ordered_and_retains_route() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = SubagentRepairRuntime::new(
        runtime(Provider {
            calls: calls.clone(),
            reject_first: true,
            reject_all: false,
            reject_initial: false,
            active: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            started: Arc::new(Notify::new()),
            requests: Arc::new(Mutex::new(Vec::new())),
            block_after_first: false,
            tool_call_after_first: false,
            repair_release: None,
            reject_instruction: None,
        }),
        SubagentRepairBudget::new(1, 1, 1024, 100).unwrap(),
    );
    let outcome = runtime
        .run(
            request(CancellationToken::new()),
            policy(true),
            SubagentRepairEligibility::Eligible(SubagentRepairFailureCategory::VerifierRejected),
            "explicit verifier rejection".to_string(),
            "return a valid result".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.terminal, SubagentRepairTerminal::RepairSucceeded);
    assert_eq!(outcome.attempts.len(), 2);
    assert_eq!(outcome.attempts[0].attempt_number, 1);
    assert_eq!(outcome.attempts[1].attempt_number, 2);
    assert_eq!(outcome.attempts[1].route, policy(true).route);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn non_repairable_failure_and_cancelled_before_repair_start_zero_repairs() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = SubagentRepairRuntime::new(
        runtime(Provider {
            calls: calls.clone(),
            reject_first: true,
            reject_all: false,
            reject_initial: false,
            active: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            started: Arc::new(Notify::new()),
            requests: Arc::new(Mutex::new(Vec::new())),
            block_after_first: false,
            tool_call_after_first: false,
            repair_release: None,
            reject_instruction: None,
        }),
        SubagentRepairBudget::new(1, 1, 1024, 100).unwrap(),
    );
    let outcome = runtime
        .run(
            request(CancellationToken::new()),
            policy(true),
            SubagentRepairEligibility::Ineligible(SubagentRepairFailureCategory::InvalidRoute),
            "invalid route".to_string(),
            "repair".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.terminal, SubagentRepairTerminal::NotEligible);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let outcome = runtime
        .run(
            request(cancellation),
            policy(true),
            SubagentRepairEligibility::Eligible(SubagentRepairFailureCategory::VerifierRejected),
            "rejected".to_string(),
            "repair".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.terminal, SubagentRepairTerminal::Cancelled);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_repair_is_terminal_and_has_no_third_attempt() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = SubagentRepairRuntime::new(
        runtime(Provider {
            calls: calls.clone(),
            reject_first: false,
            reject_all: true,
            reject_initial: false,
            active: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            started: Arc::new(Notify::new()),
            requests: Arc::new(Mutex::new(Vec::new())),
            block_after_first: false,
            tool_call_after_first: false,
            repair_release: None,
            reject_instruction: None,
        }),
        SubagentRepairBudget::new(1, 1, 1024, 100).unwrap(),
    );
    let outcome = runtime
        .run(
            request(CancellationToken::new()),
            policy(true),
            SubagentRepairEligibility::Eligible(SubagentRepairFailureCategory::VerifierRejected),
            "rejected".to_string(),
            "repair".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.terminal, SubagentRepairTerminal::RepairFailed);
    assert_eq!(outcome.attempts.len(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn repair_policy_rejects_more_than_one_attempt() {
    let mut repair_policy = policy(true);
    repair_policy.max_repair_attempts = 2;
    assert_eq!(
        repair_policy.validate(),
        Err(SubagentRepairError::PolicyInvalid)
    );
}

#[test]
fn debug_output_excludes_repair_material() {
    let route = SubagentRepairRoute {
        profile_id: "profile".to_string(),
        role: RoutingRole::Repair,
        provider_id: "provider".to_string(),
        connection_id: "connection".to_string(),
        model_id: "model".to_string(),
    };
    let debug = format!("{route:?}");
    assert!(!debug.contains("secret"));
    assert!(!debug.contains("prompt"));
}

#[tokio::test]
async fn cancellation_during_repair_is_awaited_without_detached_provider() {
    let (calls, active, peak, started, requests) = provider_state();
    let cancellation = CancellationToken::new();
    let runtime = Arc::new(SubagentRepairRuntime::new(
        runtime(provider_with(
            calls.clone(),
            active.clone(),
            peak,
            started.clone(),
            requests,
            true,
            false,
            false,
            true,
            false,
            None,
            None,
        )),
        SubagentRepairBudget::new(1, 1, 1024, 100).unwrap(),
    ));
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let cancellation = cancellation.clone();
        async move {
            runtime
                .run(
                    request(cancellation),
                    policy(true),
                    SubagentRepairEligibility::Eligible(
                        SubagentRepairFailureCategory::VerifierRejected,
                    ),
                    "rejected".to_string(),
                    "repair".to_string(),
                )
                .await
                .unwrap()
        }
    });
    wait_for_calls(&calls, &started, 2).await;
    cancellation.cancel();
    let outcome = task.await.unwrap();
    assert_eq!(outcome.terminal, SubagentRepairTerminal::Cancelled);
    assert_eq!(outcome.attempts.len(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(active.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn repair_timeout_cancels_and_awaits_active_provider() {
    let (calls, active, peak, started, requests) = provider_state();
    let runtime = SubagentRepairRuntime::new(
        runtime(provider_with(
            calls.clone(),
            active.clone(),
            peak,
            started.clone(),
            requests,
            true,
            false,
            false,
            true,
            false,
            None,
            None,
        )),
        SubagentRepairBudget::new(1, 1, 1024, 100).unwrap(),
    );
    let mut repair_policy = policy(true);
    repair_policy.per_repair_timeout = Duration::from_secs(1);
    repair_policy.total_repair_timeout = Duration::from_secs(2);
    let outcome = runtime
        .run(
            request(CancellationToken::new()),
            repair_policy,
            SubagentRepairEligibility::Eligible(SubagentRepairFailureCategory::Timeout),
            "rejected".to_string(),
            "repair".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.terminal, SubagentRepairTerminal::RepairTimedOut);
    assert_eq!(outcome.attempts.len(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(active.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn shared_budget_allows_one_concurrent_repair_and_releases_after_completion() {
    let (calls, active, peak, started, requests) = provider_state();
    let budget = SubagentRepairBudget::new(1, 1, 1024, 100).unwrap();
    let release = Arc::new(Notify::new());
    let runtime = Arc::new(SubagentRepairRuntime::new(
        runtime(provider_with(
            calls.clone(),
            active.clone(),
            peak,
            started.clone(),
            requests,
            true,
            false,
            true,
            true,
            false,
            Some(release.clone()),
            None,
        )),
        budget,
    ));
    let first_cancellation = CancellationToken::new();
    let first = tokio::spawn({
        let runtime = runtime.clone();
        let cancellation = first_cancellation.clone();
        async move {
            runtime
                .run(
                    request(cancellation),
                    policy(true),
                    SubagentRepairEligibility::Eligible(
                        SubagentRepairFailureCategory::VerifierRejected,
                    ),
                    "rejected".to_string(),
                    "repair".to_string(),
                )
                .await
                .unwrap()
        }
    });
    wait_for_calls(&calls, &started, 2).await;
    let second = runtime
        .run(
            request(CancellationToken::new()),
            policy(true),
            SubagentRepairEligibility::Eligible(SubagentRepairFailureCategory::VerifierRejected),
            "rejected".to_string(),
            "repair".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(second.terminal, SubagentRepairTerminal::BudgetExhausted);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    release.notify_one();
    first.await.unwrap();
    assert_eq!(active.load(Ordering::SeqCst), 0);
    release.notify_one();
    let third = runtime
        .run(
            request(CancellationToken::new()),
            policy(true),
            SubagentRepairEligibility::Eligible(SubagentRepairFailureCategory::VerifierRejected),
            "rejected".to_string(),
            "repair".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(third.terminal, SubagentRepairTerminal::RepairSucceeded);
}

#[tokio::test]
async fn repair_batch_enforces_ceiling_and_input_and_attempt_order() {
    let (calls, active, peak, started, requests) = provider_state();
    let release = Arc::new(Notify::new());
    let runtime = Arc::new(SubagentRepairRuntime::new(
        runtime(provider_with(
            calls.clone(),
            active,
            peak.clone(),
            started.clone(),
            requests,
            false,
            false,
            true,
            true,
            false,
            Some(release.clone()),
            None,
        )),
        SubagentRepairBudget::new(4, 4, 4096, 400).unwrap(),
    ));
    let tasks = (0..2)
        .map(|index| {
            let mut initial = request(CancellationToken::new());
            initial.task_id = format!("task-{index}");
            (
                initial,
                policy(true),
                SubagentRepairEligibility::Eligible(
                    SubagentRepairFailureCategory::VerifierRejected,
                ),
                "rejected".to_string(),
                "repair".to_string(),
            )
        })
        .collect();
    let batch = tokio::spawn(SubagentRepairBatchRuntime::run(
        SubagentRepairBatchRequest {
            tasks,
            max_concurrency: 2,
            failure_policy: SubagentFailurePolicy::ContinueIndependent,
            cancellation: CancellationToken::new(),
            runtime,
        },
    ));
    wait_for_calls(&calls, &started, 3).await;
    release.notify_waiters();
    let outcome = batch.await.unwrap().unwrap();
    assert_eq!(outcome.peak_observed_concurrency, 2);
    assert!(
        outcome
            .outcomes
            .iter()
            .all(|result| result.as_ref().unwrap().terminal
                == SubagentRepairTerminal::RepairSucceeded)
    );
    assert_eq!(
        outcome
            .outcomes
            .iter()
            .map(|result| result.as_ref().unwrap().task_id.clone())
            .collect::<Vec<_>>(),
        vec!["task-0", "task-1"]
    );
    assert!(peak.load(Ordering::SeqCst) <= 2);
    assert!(outcome.outcomes.iter().all(|result| {
        result
            .as_ref()
            .unwrap()
            .attempts
            .iter()
            .map(|attempt| attempt.attempt_number)
            .eq([1, 2])
    }));
}

#[tokio::test]
async fn continue_independent_preserves_success_after_repair_failure() {
    let (calls, active, peak, started, requests) = provider_state();
    let runtime = Arc::new(SubagentRepairRuntime::new(
        runtime(provider_with(
            calls,
            active,
            peak,
            started,
            requests,
            false,
            false,
            false,
            false,
            false,
            None,
            Some("fail".to_string()),
        )),
        SubagentRepairBudget::new(2, 2, 2048, 200).unwrap(),
    ));
    let mut first = request(CancellationToken::new());
    first.task_id = "failed".to_string();
    first.instruction = "fail this task".to_string();
    let mut second = request(CancellationToken::new());
    second.task_id = "successful".to_string();
    second.instruction = "complete this task".to_string();
    let outcome = SubagentRepairBatchRuntime::run(SubagentRepairBatchRequest {
        tasks: vec![
            (
                first,
                policy(true),
                SubagentRepairEligibility::Eligible(
                    SubagentRepairFailureCategory::VerifierRejected,
                ),
                "rejected".to_string(),
                "repair".to_string(),
            ),
            (
                second,
                policy(true),
                SubagentRepairEligibility::Eligible(
                    SubagentRepairFailureCategory::VerifierRejected,
                ),
                "rejected".to_string(),
                "repair".to_string(),
            ),
        ],
        max_concurrency: 2,
        failure_policy: SubagentFailurePolicy::ContinueIndependent,
        cancellation: CancellationToken::new(),
        runtime,
    })
    .await
    .unwrap();
    assert_eq!(
        outcome.outcomes[0].as_ref().unwrap().terminal,
        SubagentRepairTerminal::RepairFailed
    );
    assert_eq!(
        outcome.outcomes[1].as_ref().unwrap().terminal,
        SubagentRepairTerminal::InitialSucceeded
    );
}

#[tokio::test]
async fn cancel_remaining_waits_for_failed_repair_and_skips_queued_tasks() {
    let (calls, active, peak, started, requests) = provider_state();
    let runtime = Arc::new(SubagentRepairRuntime::new(
        runtime(provider_with(
            calls.clone(),
            active.clone(),
            peak,
            started.clone(),
            requests,
            true,
            false,
            true,
            true,
            false,
            None,
            Some("fail-repair".to_string()),
        )),
        SubagentRepairBudget::new(2, 2, 2048, 200).unwrap(),
    ));
    let tasks = (0..3)
        .map(|index| {
            let mut initial = request(CancellationToken::new());
            initial.task_id = format!("task-{index}");
            (
                initial,
                policy(true),
                SubagentRepairEligibility::Eligible(
                    SubagentRepairFailureCategory::VerifierRejected,
                ),
                "rejected".to_string(),
                if index == 0 {
                    "fail-repair".to_string()
                } else {
                    "block-repair".to_string()
                },
            )
        })
        .collect();
    let batch = tokio::spawn(SubagentRepairBatchRuntime::run(
        SubagentRepairBatchRequest {
            tasks,
            max_concurrency: 2,
            failure_policy: SubagentFailurePolicy::CancelRemaining,
            cancellation: CancellationToken::new(),
            runtime,
        },
    ));
    wait_for_calls(&calls, &started, 3).await;
    let outcome = batch.await.unwrap().unwrap();
    assert_eq!(outcome.outcomes.len(), 3);
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(
        outcome.outcomes[0].as_ref().unwrap().terminal,
        SubagentRepairTerminal::RepairFailed
    );
    assert_eq!(
        outcome.outcomes[1].as_ref().unwrap().terminal,
        SubagentRepairTerminal::Cancelled
    );
    assert_eq!(
        outcome.outcomes[2],
        Err(SubagentRepairError::CancelledBeforeRepair)
    );
}

#[tokio::test]
async fn tool_budget_exhaustion_does_not_execute_tool_or_retry_provider() {
    let (calls, active, peak, started, requests) = provider_state();
    let budget = SubagentRepairBudget::new(1, 1, 1024, 100).unwrap();
    let first_cancellation = CancellationToken::new();
    let runtime = Arc::new(SubagentRepairRuntime::new(
        runtime(provider_with(
            calls.clone(),
            active.clone(),
            peak,
            started.clone(),
            requests,
            true,
            false,
            true,
            true,
            false,
            None,
            None,
        )),
        budget,
    ));
    let first = tokio::spawn({
        let runtime = runtime.clone();
        let cancellation = first_cancellation.clone();
        async move {
            runtime
                .run(
                    request(cancellation),
                    policy(true),
                    SubagentRepairEligibility::Eligible(
                        SubagentRepairFailureCategory::ToolBudgetExhausted,
                    ),
                    "rejected".to_string(),
                    "repair".to_string(),
                )
                .await
                .unwrap()
        }
    });
    wait_for_calls(&calls, &started, 2).await;
    let second = runtime
        .run(
            request(CancellationToken::new()),
            policy(true),
            SubagentRepairEligibility::Eligible(SubagentRepairFailureCategory::ToolBudgetExhausted),
            "rejected".to_string(),
            "repair".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(second.terminal, SubagentRepairTerminal::BudgetExhausted);
    assert_eq!(second.attempts.len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    first_cancellation.cancel();
    first.await.unwrap();
    assert_eq!(active.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn repair_route_mismatch_is_rejected_before_provider_invocation() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = SubagentRepairRuntime::new(
        runtime(Provider {
            calls: calls.clone(),
            reject_first: true,
            reject_all: false,
            reject_initial: false,
            active: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            started: Arc::new(Notify::new()),
            requests: Arc::new(Mutex::new(Vec::new())),
            block_after_first: false,
            tool_call_after_first: false,
            repair_release: None,
            reject_instruction: None,
        }),
        SubagentRepairBudget::new(1, 1, 1024, 100).unwrap(),
    );
    let mut repair_policy = policy(true);
    repair_policy.route.provider_id = "openrouter".to_string();
    let outcome = runtime
        .run(
            request(CancellationToken::new()),
            repair_policy,
            SubagentRepairEligibility::Eligible(SubagentRepairFailureCategory::InvalidRoute),
            "rejected".to_string(),
            "repair".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.terminal, SubagentRepairTerminal::RepairFailed);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        outcome.attempts[1].terminal_failure,
        Some(SubagentRepairFailureCategory::InvalidRoute)
    );
}

#[test]
fn repair_policy_validation_rejects_all_invalid_boundaries_before_invocation() {
    let mut invalid = vec![policy(true)];
    let mut zero_attempts = policy(true);
    zero_attempts.max_repair_attempts = 0;
    invalid.push(zero_attempts);
    let mut zero_per_timeout = policy(true);
    zero_per_timeout.per_repair_timeout = Duration::ZERO;
    invalid.push(zero_per_timeout);
    let mut total_less_than_per = policy(true);
    total_less_than_per.total_repair_timeout = Duration::from_millis(1);
    invalid.push(total_less_than_per);
    let mut zero_provider = policy(true);
    zero_provider.max_provider_invocations = 0;
    invalid.push(zero_provider);
    let mut zero_tools = policy(true);
    zero_tools.max_tool_calls = 0;
    invalid.push(zero_tools);
    let mut zero_context = policy(true);
    zero_context.max_context_bytes = 0;
    invalid.push(zero_context);
    let mut oversized_context = policy(true);
    oversized_context.max_context_bytes = SUBAGENT_REPAIR_MAX_CONTEXT_BYTES + 1;
    invalid.push(oversized_context);
    let mut zero_output = policy(true);
    zero_output.max_output_tokens = 0;
    invalid.push(zero_output);
    let mut oversized_output = policy(true);
    oversized_output.max_output_tokens = SUBAGENT_REPAIR_MAX_OUTPUT_TOKENS + 1;
    invalid.push(oversized_output);
    let mut wrong_role = policy(true);
    wrong_role.route.role = RoutingRole::Planner;
    invalid.push(wrong_role);
    let mut empty_route = policy(true);
    empty_route.route.model_id.clear();
    invalid.push(empty_route);
    assert!(
        invalid
            .iter()
            .skip(1)
            .all(|candidate| { candidate.validate() == Err(SubagentRepairError::PolicyInvalid) })
    );
}

#[test]
fn debug_output_is_privacy_safe_for_all_repair_material() {
    let mut request = request(CancellationToken::new());
    request.instruction = "ORIGINAL_SENTINEL".to_string();
    request.context = Some("CONTEXT_SECRET".to_string());
    let mut policy = policy(true);
    policy.route.profile_id = "PROFILE_IDENTIFIER".to_string();
    policy.route.provider_id = "ACCOUNT_IDENTIFIER".to_string();
    policy.route.connection_id = "CONNECTION_IDENTIFIER".to_string();
    policy.route.model_id = "MODEL_IDENTIFIER".to_string();
    let route_debug = format!("{:?}", policy.route);
    let debug = format!("{:?}{:?}", request, policy);
    for sentinel in [
        "ORIGINAL_SENTINEL",
        "CONTEXT_SECRET",
        "PROFILE_IDENTIFIER",
        "ACCOUNT_IDENTIFIER",
        "CONNECTION_IDENTIFIER",
        "MODEL_IDENTIFIER",
    ] {
        assert!(!debug.contains(sentinel), "debug leaked {sentinel}");
        assert!(
            !route_debug.contains(sentinel),
            "route debug leaked {sentinel}"
        );
    }
}

#[test]
fn disabled_policy_may_use_zero_repair_attempts() {
    let mut disabled = policy(false);
    disabled.max_repair_attempts = 0;
    assert_eq!(disabled.validate(), Ok(()));
}
