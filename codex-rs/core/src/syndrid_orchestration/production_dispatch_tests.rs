use super::*;
use crate::syndrid_orchestration::account_pools::AccountPoolMember;
use crate::syndrid_orchestration::account_pools::AccountPoolProviderFamily;
use crate::syndrid_orchestration::account_pools::AccountPoolSelectionPolicy;
use crate::syndrid_orchestration::account_pools::AccountPoolTarget;
use crate::syndrid_orchestration::account_pools::NamedAccountPool;
use crate::syndrid_orchestration::account_pools::PoolId;
use crate::syndrid_orchestration::account_pools::PoolMemberId;
use crate::syndrid_orchestration::codex_accounts::CodexAccountConnectionMetadata;
use crate::syndrid_orchestration::codex_accounts::CodexAccountProfileId;
use crate::syndrid_orchestration::codex_accounts::CodexAccountProfileRegistry;
use crate::syndrid_orchestration::codex_accounts::CodexAccountProfileState;
use crate::syndrid_orchestration::invocation::ProviderInvocation;
use crate::syndrid_orchestration::invocation::ProviderInvocationUsage;
use crate::syndrid_orchestration::omniroute::ProviderSelection;
use crate::syndrid_orchestration::provider_connection::ConnectionValidationStatus;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
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

fn account(connection_id: &str) -> CodexAccountConnectionMetadata {
    CodexAccountConnectionMetadata {
        connection_id: connection_id.to_string(),
        profile_id: CodexAccountProfileId::new(connection_id).expect("profile id"),
        provider_id: "codex".to_string(),
        label: format!("account {connection_id}"),
        state: CodexAccountProfileState::Connected,
        account_email: None,
        account_id: Some(format!("opaque-{connection_id}")),
        plan_label: None,
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        last_authenticated_at: None,
        last_validated_at: None,
        credential_reference: CodexAccountProfileRegistry::credential_reference_for(connection_id)
            .expect("credential reference"),
        schema_version: 1,
    }
}

fn round_robin_binding(
    pool_id: &str,
    member_ids: &[(&str, &str)],
) -> ProductionRoundRobinProviderBinding {
    let pool = NamedAccountPool {
        id: PoolId::new(pool_id).expect("pool id"),
        display_name: pool_id.to_string(),
        provider_family: AccountPoolProviderFamily::NativeCodex,
        members: member_ids
            .iter()
            .map(|(member_id, connection_id)| AccountPoolMember {
                id: PoolMemberId::new(*member_id).expect("member id"),
                target: AccountPoolTarget::NativeCodexAccount(
                    CodexAccountProfileId::new(*connection_id).expect("profile id"),
                ),
            })
            .collect(),
        selection_policy: AccountPoolSelectionPolicy::RoundRobin,
    };
    let mut accounts = CodexAccountProfileRegistry::default();
    for (_, connection_id) in member_ids {
        accounts.insert(account(connection_id)).expect("account");
    }
    ProductionRoundRobinProviderBinding::new(
        route(&format!("pool-{pool_id}"), "codex", "model-a"),
        pool,
        accounts,
        Default::default(),
    )
    .expect("round-robin binding")
}

fn unavailable_round_robin_binding() -> ProductionRoundRobinProviderBinding {
    let pool = NamedAccountPool {
        id: PoolId::new("pool-a").expect("pool id"),
        display_name: "pool-a".to_string(),
        provider_family: AccountPoolProviderFamily::NativeCodex,
        members: vec![AccountPoolMember {
            id: PoolMemberId::new("member-a").expect("member id"),
            target: AccountPoolTarget::NativeCodexAccount(
                CodexAccountProfileId::new("missing-account").expect("profile id"),
            ),
        }],
        selection_policy: AccountPoolSelectionPolicy::RoundRobin,
    };
    ProductionRoundRobinProviderBinding::new(
        route("pool-pool-a", "codex", "model-a"),
        pool,
        CodexAccountProfileRegistry::default(),
        Default::default(),
    )
    .expect("round-robin binding")
}

#[tokio::test]
async fn round_robin_selection_is_lazy_and_reused_for_one_turn() {
    let rotation_state = Arc::new(Mutex::new(AccountPoolRotationState::new()));
    let dispatcher = ProductionRoleDispatcher::with_round_robin(
        [],
        [
            (
                RoutingRole::Planner,
                round_robin_binding(
                    "pool-a",
                    &[("member-a", "account-a"), ("member-b", "account-b")],
                ),
            ),
            (
                RoutingRole::Executor,
                round_robin_binding(
                    "pool-a",
                    &[("member-a", "account-a"), ("member-b", "account-b")],
                ),
            ),
        ],
        Arc::clone(&rotation_state),
    )
    .expect("dispatcher")
    .begin_turn();

    let planner_route = dispatcher
        .prepare_role_binding(RoutingRole::Planner)
        .await
        .expect("planner binding")
        .route()
        .selection()
        .connection_id
        .clone();
    let planner_reuse = dispatcher
        .prepare_role_binding(RoutingRole::Planner)
        .await
        .expect("planner reuse")
        .route()
        .selection()
        .connection_id
        .clone();
    let executor_route = dispatcher
        .prepare_role_binding(RoutingRole::Executor)
        .await
        .expect("executor binding")
        .route()
        .selection()
        .connection_id
        .clone();

    assert_eq!(planner_route, "account-a");
    assert_eq!(planner_reuse, planner_route);
    assert_eq!(executor_route, "account-a");

    let next_turn = dispatcher.begin_turn();
    let next_planner_route = next_turn
        .prepare_role_binding(RoutingRole::Planner)
        .await
        .expect("next planner binding")
        .route()
        .selection()
        .connection_id
        .clone();
    assert_eq!(next_planner_route, "account-b");
}

#[tokio::test]
async fn concurrent_first_use_of_one_role_commits_once_and_reuses_selection() {
    let rotation_state = Arc::new(Mutex::new(AccountPoolRotationState::new()));
    let dispatcher = ProductionRoleDispatcher::with_round_robin(
        [],
        [(
            RoutingRole::Planner,
            round_robin_binding(
                "pool-a",
                &[("member-a", "account-a"), ("member-b", "account-b")],
            ),
        )],
        Arc::clone(&rotation_state),
    )
    .expect("dispatcher")
    .begin_turn();

    let (first, second) = tokio::join!(
        dispatcher.prepare_role_binding(RoutingRole::Planner),
        dispatcher.prepare_role_binding(RoutingRole::Planner),
    );
    let first = first.expect("first planner binding");
    let second = second.expect("second planner binding");
    assert_eq!(first.route(), second.route());

    let next_turn = dispatcher.begin_turn();
    let next = next_turn
        .prepare_role_binding(RoutingRole::Planner)
        .await
        .expect("next planner binding");
    assert_eq!(next.route().selection().connection_id, "account-b");
}

#[tokio::test]
async fn round_robin_preparation_failure_does_not_consume_member() {
    let rotation_state = Arc::new(Mutex::new(AccountPoolRotationState::new()));
    let binding = unavailable_round_robin_binding();
    let dispatcher = ProductionRoleDispatcher::with_round_robin(
        [],
        [(RoutingRole::Planner, binding)],
        Arc::clone(&rotation_state),
    )
    .expect("dispatcher")
    .begin_turn();

    assert!(
        dispatcher
            .prepare_role_binding(RoutingRole::Planner)
            .await
            .is_err()
    );
    let next_turn = dispatcher.begin_turn();
    assert!(
        next_turn
            .prepare_role_binding(RoutingRole::Planner)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cooled_round_robin_members_are_skipped_without_consuming_them() {
    let session = SessionExecutionPolicyState::new().expect("session");
    session.begin_run().expect("run generation");
    let key = ProviderCooldownKey::new(AccountPoolTarget::native_codex(
        CodexAccountProfileId::new("account-a").expect("profile"),
    ));
    session
        .cooldown_state()
        .lock()
        .expect("cooldown lock")
        .record_cooldown(
            key,
            ProviderFailureClass::RateLimited,
            Duration::from_secs(60),
            Instant::now(),
        )
        .expect("cooldown");
    let rotation = session.rotation_state();
    let dispatcher = ProductionRoleDispatcher::with_round_robin(
        [],
        [(
            RoutingRole::Planner,
            round_robin_binding(
                "pool-a",
                &[("member-a", "account-a"), ("member-b", "account-b")],
            ),
        )],
        rotation,
    )
    .expect("dispatcher")
    .with_session_state(session)
    .begin_turn();
    let selected = dispatcher
        .prepare_role_binding(RoutingRole::Planner)
        .await
        .expect("eligible member");
    assert_eq!(selected.route().selection().connection_id, "account-b");
}

#[tokio::test]
async fn all_cooled_round_robin_pool_does_not_commit_or_invoke() {
    let session = SessionExecutionPolicyState::new().expect("session");
    session.begin_run().expect("run generation");
    for account_id in ["account-a", "account-b"] {
        session
            .cooldown_state()
            .lock()
            .expect("cooldown lock")
            .record_cooldown(
                ProviderCooldownKey::new(AccountPoolTarget::native_codex(
                    CodexAccountProfileId::new(account_id).expect("profile"),
                )),
                ProviderFailureClass::ProviderUnavailable,
                Duration::from_secs(60),
                Instant::now(),
            )
            .expect("cooldown");
    }
    let rotation = session.rotation_state();
    let dispatcher = ProductionRoleDispatcher::with_round_robin(
        [],
        [(
            RoutingRole::Planner,
            round_robin_binding(
                "pool-a",
                &[("member-a", "account-a"), ("member-b", "account-b")],
            ),
        )],
        Arc::clone(&rotation),
    )
    .expect("dispatcher")
    .with_session_state(session)
    .begin_turn();
    assert!(matches!(
        dispatcher.prepare_role_binding(RoutingRole::Planner).await,
        Err(ProductionRoleDispatchError::AllPoolTargetsCoolingDown { .. })
    ));
    assert_eq!(
        rotation
            .lock()
            .expect("rotation lock")
            .cursor_generation(&PoolId::new("pool-a").unwrap(), RoutingRole::Planner),
        None
    );
}

#[tokio::test]
async fn provider_failure_records_one_exact_target_cooldown_and_preserves_error() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let route = route("account-a", "codex", "model-a");
    let session = SessionExecutionPolicyState::new().expect("session");
    let dispatcher = ProductionRoleDispatcher::new([(
        RoutingRole::Planner,
        ProductionRoleBinding::new(
            route.clone(),
            RecordingProvider {
                result: Err(ProviderInvocationError::RateLimitedWithRetryAfter(Some(
                    Duration::from_secs(60),
                ))),
                calls: Arc::clone(&calls),
            },
        ),
    )])
    .expect("dispatcher")
    .with_session_state(session.clone());
    assert!(matches!(
        dispatcher
            .invoke(
                invocation(RoutingRole::Planner, &route, request("codex", "model-a")),
                CancellationToken::new(),
            )
            .await,
        Err(ProductionRoleDispatchError::ProviderFailure {
            source: ProviderInvocationError::RateLimitedWithRetryAfter(Some(_)),
            ..
        })
    ));
    assert_eq!(calls.lock().expect("calls lock").len(), 1);
    let key = ProviderCooldownKey::new(AccountPoolTarget::native_codex(
        CodexAccountProfileId::new("account-a").expect("profile"),
    ));
    assert!(matches!(
        session
            .cooldown_state()
            .lock()
            .expect("cooldown lock")
            .status(&key, Instant::now()),
        ProviderCooldownStatus::CoolingDown {
            failure_class: ProviderFailureClass::RateLimited,
            ..
        }
    ));
}

#[tokio::test]
async fn direct_target_cooling_fails_before_provider_invocation() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let route = route("account-a", "codex", "model-a");
    let session = SessionExecutionPolicyState::new().expect("session");
    session
        .cooldown_state()
        .lock()
        .expect("cooldown lock")
        .record_cooldown(
            ProviderCooldownKey::new(AccountPoolTarget::native_codex(
                CodexAccountProfileId::new("account-a").expect("profile"),
            )),
            ProviderFailureClass::RateLimited,
            Duration::from_secs(60),
            Instant::now(),
        )
        .expect("cooldown");
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
    .expect("dispatcher")
    .with_session_state(session);
    assert!(matches!(
        dispatcher
            .invoke(
                invocation(RoutingRole::Planner, &route, request("codex", "model-a")),
                CancellationToken::new(),
            )
            .await,
        Err(ProductionRoleDispatchError::TargetCoolingDown { .. })
    ));
    assert!(calls.lock().expect("calls lock").is_empty());
}

#[tokio::test]
async fn automatic_selects_first_eligible_configured_direct_candidate() {
    let session = SessionExecutionPolicyState::new().expect("session");
    session.begin_run().expect("run generation");
    session
        .cooldown_state()
        .lock()
        .expect("cooldown lock")
        .record_cooldown(
            ProviderCooldownKey::new(AccountPoolTarget::native_codex(
                CodexAccountProfileId::new("account-a").expect("profile"),
            )),
            ProviderFailureClass::RateLimited,
            Duration::from_secs(60),
            Instant::now(),
        )
        .expect("cooldown");
    let first = route("account-a", "codex", "model-a");
    let second = route("account-b", "codex", "model-b");
    let second_calls = Arc::new(Mutex::new(Vec::new()));
    let candidate = |connection: &str, model: &str| {
        let target = RoutingStrategyCandidateTarget::direct(
            AccountPoolTarget::native_codex(
                CodexAccountProfileId::new(connection).expect("profile"),
            ),
            "codex",
            model,
        )
        .expect("candidate target");
        RoutingStrategyCandidate::new(RoutingStrategyCandidateId::new(
            RoutingProfileId::new("automatic").expect("profile id"),
            RoutingRole::Planner,
            target,
        ))
    };
    let dispatcher = ProductionRoleDispatcher::new([])
        .expect("dispatcher")
        .with_session_state(session)
        .with_automatic_candidates(BTreeMap::from([(
            RoutingRole::Planner,
            vec![
                ProductionAutomaticRoleCandidate::new(
                    candidate("account-a", "model-a"),
                    ProductionAutomaticRoleBinding::Direct(ProductionRoleBinding::new(
                        first,
                        RecordingProvider {
                            result: Ok(()),
                            calls: Arc::new(Mutex::new(Vec::new())),
                        },
                    )),
                ),
                ProductionAutomaticRoleCandidate::new(
                    candidate("account-b", "model-b"),
                    ProductionAutomaticRoleBinding::Direct(ProductionRoleBinding::new(
                        second.clone(),
                        RecordingProvider {
                            result: Ok(()),
                            calls: Arc::clone(&second_calls),
                        },
                    )),
                ),
            ],
        )]))
        .begin_turn();
    let selected = dispatcher
        .prepare_role_binding(RoutingRole::Planner)
        .await
        .expect("automatic selection");
    assert_eq!(selected.route(), &second);
    let result = dispatcher
        .invoke_role(
            RoutingRole::Planner,
            request("codex", "model-a"),
            CancellationToken::new(),
        )
        .await
        .expect("automatic invocation");
    assert_eq!(result.provider, "codex");
    assert_eq!(result.model, "model-b");
    assert_eq!(second_calls.lock().expect("calls lock").len(), 1);
}

#[tokio::test]
async fn automatic_selects_pool_through_existing_round_robin_admission() {
    let session = SessionExecutionPolicyState::new().expect("session");
    session.begin_run().expect("run generation");
    session
        .cooldown_state()
        .lock()
        .expect("cooldown lock")
        .record_cooldown(
            ProviderCooldownKey::new(AccountPoolTarget::native_codex(
                CodexAccountProfileId::new("account-cooling").expect("profile"),
            )),
            ProviderFailureClass::RateLimited,
            Duration::from_secs(60),
            Instant::now(),
        )
        .expect("cooldown");
    let rotation = session.rotation_state();
    let pool_id = PoolId::new("pool-automatic").expect("pool id");
    let pool_binding = round_robin_binding(
        pool_id.as_str(),
        &[("member-a", "account-a"), ("member-b", "account-b")],
    );
    let direct_candidate = RoutingStrategyCandidate::new(RoutingStrategyCandidateId::new(
        RoutingProfileId::new("automatic").expect("profile id"),
        RoutingRole::Planner,
        RoutingStrategyCandidateTarget::direct(
            AccountPoolTarget::native_codex(
                CodexAccountProfileId::new("account-cooling").expect("profile"),
            ),
            "codex",
            "model-a",
        )
        .expect("direct candidate"),
    ));
    let pool_candidate = RoutingStrategyCandidate::new(RoutingStrategyCandidateId::new(
        RoutingProfileId::new("automatic").expect("profile id"),
        RoutingRole::Planner,
        RoutingStrategyCandidateTarget::pool(pool_id.clone(), "codex", "model-a")
            .expect("pool candidate"),
    ));
    let dispatcher = ProductionRoleDispatcher::with_round_robin([], [], Arc::clone(&rotation))
        .expect("dispatcher")
        .with_session_state(session)
        .with_automatic_candidates(BTreeMap::from([(
            RoutingRole::Planner,
            vec![
                ProductionAutomaticRoleCandidate::new(
                    direct_candidate,
                    ProductionAutomaticRoleBinding::Direct(ProductionRoleBinding::new(
                        route("account-cooling", "codex", "model-a"),
                        RecordingProvider {
                            result: Ok(()),
                            calls: Arc::new(Mutex::new(Vec::new())),
                        },
                    )),
                ),
                ProductionAutomaticRoleCandidate::new(
                    pool_candidate,
                    ProductionAutomaticRoleBinding::RoundRobin(pool_binding),
                ),
            ],
        )]))
        .begin_turn();

    let selected = dispatcher
        .prepare_role_binding(RoutingRole::Planner)
        .await
        .expect("automatic pool selection");
    assert_eq!(selected.route().selection().connection_id, "account-a");
    assert_eq!(
        rotation
            .lock()
            .expect("rotation lock")
            .cursor_generation(&pool_id, RoutingRole::Planner),
        Some(1)
    );

    let next_turn = dispatcher.begin_turn();
    let next_selected = next_turn
        .prepare_role_binding(RoutingRole::Planner)
        .await
        .expect("next automatic pool selection");
    assert_eq!(next_selected.route().selection().connection_id, "account-b");
}

#[tokio::test]
async fn automatic_concurrent_first_use_commits_one_pool_member() {
    let session = SessionExecutionPolicyState::new().expect("session");
    session.begin_run().expect("run generation");
    let rotation = session.rotation_state();
    let pool_id = PoolId::new("pool-concurrent-automatic").expect("pool id");
    let candidate = RoutingStrategyCandidate::new(RoutingStrategyCandidateId::new(
        RoutingProfileId::new("automatic").expect("profile id"),
        RoutingRole::Planner,
        RoutingStrategyCandidateTarget::pool(pool_id.clone(), "codex", "model-a")
            .expect("pool candidate"),
    ));
    let dispatcher = ProductionRoleDispatcher::with_round_robin([], [], Arc::clone(&rotation))
        .expect("dispatcher")
        .with_session_state(session)
        .with_automatic_candidates(BTreeMap::from([(
            RoutingRole::Planner,
            vec![ProductionAutomaticRoleCandidate::new(
                candidate,
                ProductionAutomaticRoleBinding::RoundRobin(round_robin_binding(
                    pool_id.as_str(),
                    &[("member-a", "account-a"), ("member-b", "account-b")],
                )),
            )],
        )]))
        .begin_turn();

    let (first, second) = tokio::join!(
        dispatcher.prepare_role_binding(RoutingRole::Planner),
        dispatcher.prepare_role_binding(RoutingRole::Planner),
    );
    let first = first.expect("first automatic binding");
    let second = second.expect("second automatic binding");
    assert_eq!(first.route(), second.route());
    assert_eq!(first.route().selection().connection_id, "account-a");

    let next_turn = dispatcher.begin_turn();
    let next = next_turn
        .prepare_role_binding(RoutingRole::Planner)
        .await
        .expect("next automatic binding");
    assert_eq!(next.route().selection().connection_id, "account-b");
    assert_eq!(
        rotation
            .lock()
            .expect("rotation lock")
            .cursor_generation(&pool_id, RoutingRole::Planner),
        Some(2)
    );
}

#[tokio::test]
async fn automatic_rejects_all_cooled_candidates_without_selecting_a_route() {
    let session = SessionExecutionPolicyState::new().expect("session");
    session.begin_run().expect("run generation");
    for connection in ["account-a", "account-b"] {
        session
            .cooldown_state()
            .lock()
            .expect("cooldown lock")
            .record_cooldown(
                ProviderCooldownKey::new(AccountPoolTarget::native_codex(
                    CodexAccountProfileId::new(connection).expect("profile"),
                )),
                ProviderFailureClass::RateLimited,
                Duration::from_secs(60),
                Instant::now(),
            )
            .expect("cooldown");
    }
    let candidate = |connection: &str| {
        let target = RoutingStrategyCandidateTarget::direct(
            AccountPoolTarget::native_codex(
                CodexAccountProfileId::new(connection).expect("profile"),
            ),
            "codex",
            "model-a",
        )
        .expect("candidate target");
        RoutingStrategyCandidate::new(RoutingStrategyCandidateId::new(
            RoutingProfileId::new("automatic").expect("profile id"),
            RoutingRole::Planner,
            target,
        ))
    };
    let dispatcher = ProductionRoleDispatcher::new([])
        .expect("dispatcher")
        .with_session_state(session)
        .with_automatic_candidates(BTreeMap::from([(
            RoutingRole::Planner,
            vec![
                ProductionAutomaticRoleCandidate::new(
                    candidate("account-a"),
                    ProductionAutomaticRoleBinding::Direct(ProductionRoleBinding::new(
                        route("account-a", "codex", "model-a"),
                        RecordingProvider {
                            result: Ok(()),
                            calls: Arc::new(Mutex::new(Vec::new())),
                        },
                    )),
                ),
                ProductionAutomaticRoleCandidate::new(
                    candidate("account-b"),
                    ProductionAutomaticRoleBinding::Direct(ProductionRoleBinding::new(
                        route("account-b", "codex", "model-a"),
                        RecordingProvider {
                            result: Ok(()),
                            calls: Arc::new(Mutex::new(Vec::new())),
                        },
                    )),
                ),
            ],
        )]))
        .begin_turn();
    assert!(matches!(
        dispatcher.prepare_role_binding(RoutingRole::Planner).await,
        Err(ProductionRoleDispatchError::AutomaticSelection {
            reason: AutomaticRoutingUnavailableReason::AllCandidatesCoolingDown,
            ..
        })
    ));
}

#[tokio::test]
async fn direct_omniroute_target_cooling_fails_before_provider_invocation() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let route = route("connection-a", "omniroute", "model-a");
    let session = SessionExecutionPolicyState::new().expect("session");
    session
        .cooldown_state()
        .lock()
        .expect("cooldown lock")
        .record_cooldown(
            ProviderCooldownKey::new(AccountPoolTarget::omniroute("connection-a").unwrap()),
            ProviderFailureClass::RateLimited,
            Duration::from_secs(60),
            Instant::now(),
        )
        .expect("cooldown");
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
    .expect("dispatcher")
    .with_session_state(session);
    assert!(matches!(
        dispatcher
            .invoke(
                invocation(
                    RoutingRole::Planner,
                    &route,
                    request("omniroute", "model-a")
                ),
                CancellationToken::new(),
            )
            .await,
        Err(ProductionRoleDispatchError::TargetCoolingDown { .. })
    ));
    assert!(calls.lock().expect("calls lock").is_empty());
}

#[tokio::test]
async fn same_turn_cooling_keeps_the_exact_target_pinned() {
    let session = SessionExecutionPolicyState::new().expect("session");
    session.begin_run().expect("run generation");
    let dispatcher = ProductionRoleDispatcher::with_round_robin(
        [],
        [(
            RoutingRole::Planner,
            round_robin_binding(
                "pool-a",
                &[("member-a", "account-a"), ("member-b", "account-b")],
            ),
        )],
        session.rotation_state(),
    )
    .expect("dispatcher")
    .with_session_state(session.clone())
    .begin_turn();
    dispatcher
        .prepare_role_binding(RoutingRole::Planner)
        .await
        .expect("first binding");
    session
        .cooldown_state()
        .lock()
        .expect("cooldown lock")
        .record_cooldown(
            ProviderCooldownKey::new(AccountPoolTarget::native_codex(
                CodexAccountProfileId::new("account-a").expect("profile"),
            )),
            ProviderFailureClass::RateLimited,
            Duration::from_secs(60),
            Instant::now(),
        )
        .expect("cooldown");
    assert!(matches!(
        dispatcher.prepare_role_binding(RoutingRole::Planner).await,
        Err(ProductionRoleDispatchError::SameTurnSelectedTargetCoolingDown { .. })
    ));
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
