use super::execution_modes::BuiltInExecutionMode;
use super::execution_modes::EXECUTION_MAX_CONTEXT_BYTES;
use super::execution_modes::EXECUTION_MAX_OUTPUT_TOKENS;
use super::execution_modes::ExecutionModeSelection;
use super::execution_modes::ExecutionPolicy;
use super::execution_modes::ExecutionPolicyError;
use super::execution_modes::ExecutionShape;
use super::execution_modes::RepairPolicyDecision;
use super::execution_modes::RoleActivation;
use super::execution_modes::RoleExecutionPolicy;
use super::provider_connection::ConnectionValidationStatus;
use super::routing_profiles::RoutingAssignment;
use super::routing_profiles::RoutingConnectionDirectory;
use super::routing_profiles::RoutingConnectionInfo;
use super::routing_profiles::RoutingProfile;
use super::routing_profiles::RoutingProfileId;
use super::routing_profiles::RoutingProfileRegistry;
use super::routing_profiles::RoutingRole;
use super::subagent_batch::SubagentFailurePolicy;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::collections::BTreeMap;
use std::time::Duration;

fn role(activation: RoleActivation, effort: ReasoningEffort) -> RoleExecutionPolicy {
    RoleExecutionPolicy { activation, effort }
}

fn custom_policy() -> ExecutionPolicy {
    ExecutionPolicy {
        roles: BTreeMap::from([
            (
                RoutingRole::Main,
                role(RoleActivation::Required, ReasoningEffort::Low),
            ),
            (
                RoutingRole::Planner,
                role(RoleActivation::Optional, ReasoningEffort::Medium),
            ),
            (
                RoutingRole::Executor,
                role(RoleActivation::Required, ReasoningEffort::Medium),
            ),
            (
                RoutingRole::Verifier,
                role(RoleActivation::Optional, ReasoningEffort::Medium),
            ),
            (
                RoutingRole::Repair,
                role(RoleActivation::Disabled, ReasoningEffort::Low),
            ),
        ]),
        max_subagents: 2,
        max_concurrency: 1,
        max_provider_invocations: 2,
        max_tool_calls: 4,
        max_tool_output_bytes: 64 * 1024,
        max_repair_attempts: 0,
        task_timeout: Duration::from_secs(20),
        batch_timeout: Duration::from_secs(30),
        repair_timeout: Duration::ZERO,
        context_budget_bytes: 8 * 1024,
        output_budget_tokens: 1_000,
        max_final_response_tokens: 1_000,
        optional_roles_may_skip: true,
        shape: ExecutionShape::SinglePass,
    }
}

fn profile(roles: &[RoutingRole]) -> (RoutingProfile, RoutingConnectionDirectory) {
    let mut profile =
        RoutingProfile::new(RoutingProfileId::new("test").unwrap(), "test", 1).unwrap();
    let mut directory = RoutingConnectionDirectory::default();
    for (index, role) in roles.iter().enumerate() {
        let connection_id = format!("connection-{index}");
        profile
            .assign(
                *role,
                RoutingAssignment {
                    connection_id: connection_id.clone(),
                    provider_id: "provider".to_string(),
                    model_id: "model".to_string(),
                    enabled: true,
                    label: None,
                },
            )
            .unwrap();
        directory.insert(RoutingConnectionInfo {
            connection_id,
            provider_id: "provider".to_string(),
            enabled: true,
            validation: ConnectionValidationStatus::Valid,
            authentication_supported: true,
            models: Some(vec!["model".to_string()]),
        });
    }
    (profile, directory)
}

#[test]
fn built_in_modes_resolve_deterministically() {
    for selection in [
        ExecutionModeSelection::Fast,
        ExecutionModeSelection::Balanced,
        ExecutionModeSelection::UsageSaver,
        ExecutionModeSelection::Deep,
    ] {
        assert_eq!(selection.resolve().unwrap(), selection.resolve().unwrap());
    }
}

#[test]
fn unsupported_mode_is_rejected_without_resolution() {
    assert_eq!(
        ExecutionModeSelection::parse("auto"),
        Err(ExecutionPolicyError::UnknownMode)
    );
    assert_eq!(
        ExecutionModeSelection::parse("quota-aware"),
        Err(ExecutionPolicyError::UnknownMode)
    );
    assert_eq!(
        ExecutionModeSelection::parse("usage_saver").unwrap(),
        ExecutionModeSelection::UsageSaver
    );
}

#[test]
fn built_in_relative_behavior_stays_bounded() {
    let fast = ExecutionModeSelection::Fast.resolve().unwrap();
    let balanced = ExecutionModeSelection::Balanced.resolve().unwrap();
    let saver = ExecutionModeSelection::UsageSaver.resolve().unwrap();
    let deep = ExecutionModeSelection::Deep.resolve().unwrap();
    assert!(saver.policy().max_subagents <= balanced.policy().max_subagents);
    assert!(saver.policy().max_provider_invocations <= balanced.policy().max_provider_invocations);
    assert!(fast.policy().max_subagents <= balanced.policy().max_subagents);
    assert!(deep.policy().max_subagents >= balanced.policy().max_subagents);
    assert!(deep.policy().max_provider_invocations >= balanced.policy().max_provider_invocations);
    assert!(deep.policy().max_concurrency <= 4);
    assert!(deep.policy().max_repair_attempts <= 1);
}

#[test]
fn balanced_is_the_default_without_routing_selection() {
    assert_eq!(
        ExecutionModeSelection::default(),
        ExecutionModeSelection::Balanced
    );
    let resolved = ExecutionModeSelection::default().resolve().unwrap();
    assert_eq!(
        resolved.source(),
        super::execution_modes::PolicySource::BuiltIn(BuiltInExecutionMode::Balanced)
    );
    let debug = format!("{resolved:?}");
    assert!(!debug.contains("provider_id"));
    assert!(!debug.contains("connection_id"));
    assert!(!debug.contains("model_id"));
}

#[test]
fn fast_and_usage_saver_are_single_pass_and_disable_optional_roles() {
    for selection in [
        ExecutionModeSelection::Fast,
        ExecutionModeSelection::UsageSaver,
    ] {
        let resolved = selection.resolve().unwrap();
        assert_eq!(resolved.policy().shape, ExecutionShape::SinglePass);
        for role in [
            RoutingRole::Planner,
            RoutingRole::Verifier,
            RoutingRole::Repair,
        ] {
            assert_eq!(resolved.role(role).activation, RoleActivation::Disabled);
        }
    }
    let saver = ExecutionModeSelection::UsageSaver.resolve().unwrap();
    assert_eq!(saver.policy().max_concurrency, 1);
    assert!(saver.policy().context_budget_bytes <= 4 * 1024);
}

#[test]
fn deep_enables_all_bounded_stages() {
    let resolved = ExecutionModeSelection::Deep.resolve().unwrap();
    for role in [
        RoutingRole::Planner,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ] {
        assert_eq!(resolved.role(role).activation, RoleActivation::Required);
        assert_eq!(resolved.role(role).effort, ReasoningEffort::High);
    }
    assert_eq!(resolved.policy().max_repair_attempts, 1);
    assert_eq!(resolved.policy().max_concurrency, 4);
}

#[test]
fn valid_custom_policy_is_preserved() {
    let policy = custom_policy();
    let resolved = ExecutionModeSelection::custom(policy.clone())
        .resolve()
        .unwrap();
    assert_eq!(resolved.policy(), &policy);
}

#[test]
fn custom_invalid_combinations_are_rejected_before_execution() {
    macro_rules! assert_invalid {
        ($mutation:expr, $expected:expr) => {
            let mut policy = custom_policy();
            $mutation(&mut policy);
            assert_eq!(
                ExecutionModeSelection::custom(policy)
                    .resolve()
                    .unwrap_err(),
                $expected
            );
        };
    }
    assert_invalid!(
        |policy: &mut ExecutionPolicy| policy.max_concurrency = 0,
        ExecutionPolicyError::InvalidConcurrency
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| policy.max_concurrency = 3,
        ExecutionPolicyError::InvalidConcurrency
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| policy.max_concurrency = 5,
        ExecutionPolicyError::InvalidConcurrency
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| policy.max_repair_attempts = 2,
        ExecutionPolicyError::InvalidRepairConfiguration
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| {
            policy.roles.insert(
                RoutingRole::Repair,
                role(RoleActivation::Disabled, ReasoningEffort::Low),
            );
            policy.max_repair_attempts = 1;
        },
        ExecutionPolicyError::InvalidRepairConfiguration
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| policy.max_provider_invocations = 0,
        ExecutionPolicyError::InsufficientProviderBudget
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| policy.max_tool_calls = 0,
        ExecutionPolicyError::InsufficientToolBudget
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| policy.task_timeout = Duration::ZERO,
        ExecutionPolicyError::InvalidTimeout
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| policy.batch_timeout = Duration::ZERO,
        ExecutionPolicyError::InvalidTimeout
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| {
            policy.roles.insert(
                RoutingRole::Repair,
                role(RoleActivation::Optional, ReasoningEffort::Medium),
            );
            policy.shape = ExecutionShape::BoundedVerificationRepair;
            policy.max_repair_attempts = 1;
            policy.repair_timeout = Duration::ZERO;
        },
        ExecutionPolicyError::InvalidRepairConfiguration
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| {
            policy.roles.insert(
                RoutingRole::Repair,
                role(RoleActivation::Optional, ReasoningEffort::Medium),
            );
            policy.shape = ExecutionShape::BoundedVerificationRepair;
            policy.max_repair_attempts = 1;
            policy.repair_timeout = Duration::from_secs(31);
        },
        ExecutionPolicyError::InvalidRepairConfiguration
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| policy.context_budget_bytes =
            EXECUTION_MAX_CONTEXT_BYTES + 1,
        ExecutionPolicyError::PolicyExceedsHardCeiling
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| policy.output_budget_tokens =
            EXECUTION_MAX_OUTPUT_TOKENS + 1,
        ExecutionPolicyError::PolicyExceedsHardCeiling
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| policy.task_timeout = Duration::from_secs(31),
        ExecutionPolicyError::InvalidTimeout
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| {
            policy.roles.insert(
                RoutingRole::Main,
                role(RoleActivation::Optional, ReasoningEffort::Low),
            );
        },
        ExecutionPolicyError::ContradictoryRoleSettings(RoutingRole::Main)
    );
    assert_invalid!(
        |policy: &mut ExecutionPolicy| {
            policy.roles.insert(
                RoutingRole::Planner,
                role(
                    RoleActivation::Optional,
                    ReasoningEffort::Custom("unsupported".to_string()),
                ),
            );
        },
        ExecutionPolicyError::UnsupportedEffort(RoutingRole::Planner)
    );
}

#[test]
fn routing_validation_skips_disabled_roles_and_requires_enabled_roles() {
    let fast = ExecutionModeSelection::Fast.resolve().unwrap();
    let (fast_profile, fast_directory) = profile(&[RoutingRole::Main, RoutingRole::Executor]);
    fast.validate_routing_profile(&fast_profile, &fast_directory)
        .unwrap();

    let deep = ExecutionModeSelection::Deep.resolve().unwrap();
    let (deep_profile, deep_directory) = profile(&[
        RoutingRole::Main,
        RoutingRole::Executor,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ]);
    assert_eq!(
        deep.validate_routing_profile(&deep_profile, &deep_directory),
        Err(ExecutionPolicyError::MissingRequiredRoute(
            RoutingRole::Planner
        ))
    );
}

#[test]
fn routing_validation_rejects_disabled_or_invalid_assignments() {
    let fast = ExecutionModeSelection::Fast.resolve().unwrap();
    let (mut profile, directory) = profile(&[RoutingRole::Main, RoutingRole::Executor]);
    profile
        .assignments
        .get_mut(&RoutingRole::Main)
        .unwrap()
        .enabled = false;
    assert_eq!(
        fast.validate_routing_profile(&profile, &directory),
        Err(ExecutionPolicyError::DisabledRoute(RoutingRole::Main))
    );
    profile
        .assignments
        .get_mut(&RoutingRole::Main)
        .unwrap()
        .enabled = true;
    let mut invalid_directory = directory;
    invalid_directory.insert(RoutingConnectionInfo {
        connection_id: "connection-0".to_string(),
        provider_id: "different".to_string(),
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        authentication_supported: true,
        models: Some(vec!["model".to_string()]),
    });
    assert_eq!(
        fast.validate_routing_profile(&profile, &invalid_directory),
        Err(ExecutionPolicyError::InvalidProviderConnection(
            RoutingRole::Main
        ))
    );
}

#[test]
fn active_routing_requires_an_active_profile() {
    let resolved = ExecutionModeSelection::Fast.resolve().unwrap();
    let registry = RoutingProfileRegistry::default();
    assert_eq!(
        resolved.validate_active_routing(&registry, &RoutingConnectionDirectory::default()),
        Err(ExecutionPolicyError::RoutingProfileError(
            super::routing_profiles::RoutingProfileError::MissingActiveProfile
        ))
    );
}

#[test]
fn adapters_preserve_o6c_and_o6d_limits() {
    let balanced = ExecutionModeSelection::Balanced.resolve().unwrap();
    let batch = balanced.to_batch_policy(SubagentFailurePolicy::ContinueIndependent);
    assert_eq!(batch.max_tasks, 2);
    assert_eq!(batch.max_concurrency, 2);
    assert_eq!(batch.max_provider_turns, 8);
    assert_eq!(batch.max_tool_calls, 16);
    assert_eq!(
        batch.failure_policy,
        SubagentFailurePolicy::ContinueIndependent
    );
    let route = super::subagent_repair::SubagentRepairRoute {
        profile_id: "profile".to_string(),
        role: RoutingRole::Repair,
        provider_id: "provider".to_string(),
        connection_id: "connection".to_string(),
        model_id: "model".to_string(),
    };
    match balanced.repair_policy(route).unwrap() {
        RepairPolicyDecision::Enabled(policy) => {
            assert_eq!(policy.max_repair_attempts, 1);
            assert_eq!(policy.max_context_bytes, 16 * 1024);
            assert_eq!(policy.max_output_tokens, 4_000);
        }
        RepairPolicyDecision::Disabled => panic!("balanced repair must be enabled"),
    }
    assert_eq!(
        ExecutionModeSelection::Fast
            .resolve()
            .unwrap()
            .repair_policy(super::subagent_repair::SubagentRepairRoute {
                profile_id: "profile".to_string(),
                role: RoutingRole::Repair,
                provider_id: "provider".to_string(),
                connection_id: "connection".to_string(),
                model_id: "model".to_string(),
            })
            .unwrap(),
        RepairPolicyDecision::Disabled
    );
}

#[test]
fn repair_route_must_match_the_selected_profile_exactly() {
    let resolved = ExecutionModeSelection::Balanced.resolve().unwrap();
    let (profile, directory) = profile(&[
        RoutingRole::Main,
        RoutingRole::Executor,
        RoutingRole::Planner,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ]);
    let route = super::subagent_repair::SubagentRepairRoute {
        profile_id: profile.id.as_str().to_string(),
        role: RoutingRole::Repair,
        provider_id: "provider".to_string(),
        connection_id: "connection-4".to_string(),
        model_id: "model".to_string(),
    };
    resolved
        .validate_repair_route(&route, &profile, &directory)
        .unwrap();
    let mut mismatched = route.clone();
    mismatched.model_id = "substitute-model".to_string();
    assert_eq!(
        resolved.validate_repair_route(&mismatched, &profile, &directory),
        Err(ExecutionPolicyError::RepairRouteMismatch)
    );
}

#[test]
fn explainability_is_structured_deterministic_and_private() {
    let resolved = ExecutionModeSelection::Deep.resolve().unwrap();
    let explanation = resolved.explain();
    assert_eq!(
        explanation
            .roles
            .iter()
            .map(|(role, _)| *role)
            .collect::<Vec<_>>(),
        vec![
            RoutingRole::Main,
            RoutingRole::Planner,
            RoutingRole::Executor,
            RoutingRole::Verifier,
            RoutingRole::Repair
        ]
    );
    assert_eq!(explanation.max_repair_attempts, 1);
    let debug = format!("{explanation:?}");
    for sentinel in [
        "credential-token",
        "secret-prompt",
        "raw-context",
        "account-secret",
    ] {
        assert!(!debug.contains(sentinel));
    }
}

#[test]
fn effort_compatibility_rejects_unknown_values_without_downgrade() {
    let mut policy = custom_policy();
    policy.roles.insert(
        RoutingRole::Executor,
        role(
            RoleActivation::Required,
            ReasoningEffort::Custom("future-effort".to_string()),
        ),
    );
    assert_eq!(
        ExecutionModeSelection::custom(policy)
            .resolve()
            .unwrap_err(),
        ExecutionPolicyError::UnsupportedEffort(RoutingRole::Executor)
    );
}

#[test]
fn built_in_identifiers_and_custom_policy_round_trip() {
    assert_eq!(
        serde_json::to_value(BuiltInExecutionMode::UsageSaver).unwrap(),
        json!("usage_saver")
    );
    assert_eq!(
        serde_json::to_value(ExecutionModeSelection::Deep).unwrap(),
        json!("deep")
    );
    let policy = custom_policy();
    let encoded = serde_json::to_string(&ExecutionModeSelection::custom(policy.clone())).unwrap();
    let decoded: ExecutionModeSelection = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, ExecutionModeSelection::custom(policy));
}
