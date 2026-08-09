use super::ExecutionModeSelection;
use super::OrchestrationMode;
use super::OrchestrationStrategyAvailability;
use super::OrchestrationStrategyUnavailableReason;
use super::ResolvedOrchestrationPolicy;
use pretty_assertions::assert_eq;

#[test]
fn supported_strategies_keep_the_selected_preset_distinct() {
    for strategy in [
        OrchestrationMode::Single,
        OrchestrationMode::Manual,
        OrchestrationMode::Recommended,
        OrchestrationMode::Automatic,
    ] {
        let resolved = ResolvedOrchestrationPolicy::resolve(strategy, ExecutionModeSelection::Fast)
            .expect("fast preset should resolve");
        assert_eq!(resolved.strategy(), strategy);
        assert_eq!(
            resolved.execution().selected_mode(),
            &ExecutionModeSelection::Fast
        );
        assert_eq!(
            resolved.availability(),
            OrchestrationStrategyAvailability::Available
        );
    }
}

#[test]
fn strategy_and_preset_matrix_preserves_both_dimensions() {
    let strategies = [
        OrchestrationMode::Single,
        OrchestrationMode::Manual,
        OrchestrationMode::Recommended,
        OrchestrationMode::Automatic,
        OrchestrationMode::Adaptive,
    ];
    let presets = [
        ExecutionModeSelection::Fast,
        ExecutionModeSelection::Balanced,
        ExecutionModeSelection::UsageSaver,
        ExecutionModeSelection::Deep,
    ];

    for strategy in strategies {
        for preset in &presets {
            let resolved = ResolvedOrchestrationPolicy::resolve(strategy, preset.clone())
                .expect("built-in strategy and preset should resolve");
            assert_eq!(resolved.strategy(), strategy);
            assert_eq!(resolved.execution().selected_mode(), preset);
        }
    }
}

#[test]
fn manual_fast_policy_reaches_the_canonical_fast_limits() {
    let resolved = ResolvedOrchestrationPolicy::resolve(
        OrchestrationMode::Manual,
        ExecutionModeSelection::Fast,
    )
    .expect("fast policy");
    let policy = resolved.execution().policy();
    assert_eq!(policy.max_subagents, 1);
    assert_eq!(policy.max_concurrency, 1);
    assert_eq!(policy.max_provider_invocations, 1);
    assert_eq!(policy.max_tool_calls, 4);
    assert_eq!(policy.batch_timeout, std::time::Duration::from_secs(30));
}

#[test]
fn manual_deep_policy_reaches_the_canonical_deep_limits() {
    let resolved = ResolvedOrchestrationPolicy::resolve(
        OrchestrationMode::Manual,
        ExecutionModeSelection::Deep,
    )
    .expect("deep policy");
    let policy = resolved.execution().policy();
    assert_eq!(policy.max_subagents, 8);
    assert_eq!(policy.max_concurrency, 4);
    assert_eq!(policy.max_provider_invocations, 64);
    assert_eq!(policy.max_tool_calls, 128);
    assert_eq!(policy.batch_timeout, std::time::Duration::from_secs(900));
}

#[test]
fn automatic_is_available_while_adaptive_remains_unavailable() {
    let automatic = ResolvedOrchestrationPolicy::resolve(
        OrchestrationMode::Automatic,
        ExecutionModeSelection::Deep,
    )
    .expect("automatic policy should resolve");
    assert_eq!(
        automatic.availability(),
        OrchestrationStrategyAvailability::Available
    );
    assert!(automatic.requires_syndrid_runtime());

    let adaptive = ResolvedOrchestrationPolicy::resolve(
        OrchestrationMode::Adaptive,
        ExecutionModeSelection::Deep,
    )
    .expect("adaptive preset should resolve");
    assert_eq!(
        adaptive.availability(),
        OrchestrationStrategyAvailability::Unavailable(
            OrchestrationStrategyUnavailableReason::AdaptiveUsageAuthorityUnavailable,
        )
    );
    assert!(!adaptive.requires_syndrid_runtime());
}

#[test]
fn every_builtin_preset_resolves_through_the_same_authority() {
    for preset in [
        ExecutionModeSelection::Fast,
        ExecutionModeSelection::Balanced,
        ExecutionModeSelection::UsageSaver,
        ExecutionModeSelection::Deep,
    ] {
        let resolved =
            ResolvedOrchestrationPolicy::resolve(OrchestrationMode::Manual, preset.clone())
                .expect("built-in preset should resolve");
        assert_eq!(resolved.execution().selected_mode(), &preset);
        assert!(resolved.execution().policy().max_concurrency > 0);
        assert!(resolved.execution().policy().max_provider_invocations > 0);
        assert!(resolved.execution().policy().max_tool_calls > 0);
        assert!(!resolved.execution().policy().batch_timeout.is_zero());
    }
}

#[test]
fn invalid_custom_preset_is_rejected_before_strategy_activation() {
    let mut policy = ExecutionModeSelection::Deep
        .resolve()
        .expect("deep preset should resolve")
        .policy()
        .clone();
    policy.max_concurrency = 0;
    let result = ResolvedOrchestrationPolicy::resolve(
        OrchestrationMode::Manual,
        ExecutionModeSelection::custom(policy),
    );
    assert!(result.is_err());
}
