use super::*;
use crate::render::renderable::Renderable;
use crate::token_usage::TokenUsage;
use crate::token_usage::TokenUsageInfo;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

#[test]
fn visibility_cycles_without_conflicting_flags() {
    assert_eq!(
        DashboardVisibility::Hidden.toggle(),
        DashboardVisibility::Compact
    );
    assert_eq!(
        DashboardVisibility::Compact.toggle(),
        DashboardVisibility::Expanded
    );
    assert_eq!(
        DashboardVisibility::Expanded.toggle(),
        DashboardVisibility::Hidden
    );
}

#[test]
fn visible_dashboard_owns_the_primary_viewport() {
    assert!(!DashboardVisibility::Hidden.owns_primary_viewport());
    assert!(DashboardVisibility::Compact.owns_primary_viewport());
    assert!(DashboardVisibility::Expanded.owns_primary_viewport());
}

#[test]
fn terminal_observation_does_not_block_later_cleanup_snapshot() {
    assert!(accepts_newer_observation(Some(7), 4, 7, 5));
    assert!(!accepts_newer_observation(Some(7), 5, 7, 5));
    assert!(!accepts_newer_observation(Some(7), 5, 6, 6));
}

#[test]
fn dashboard_lifecycle_maps_every_terminal_turn_status() {
    let cases = [
        (
            TurnStatus::InProgress,
            SessionDashboardLifecycle::Active {
                turn_id: "turn-1".to_string(),
            },
            "Working",
            false,
        ),
        (
            TurnStatus::Completed,
            SessionDashboardLifecycle::Completed {
                turn_id: "turn-1".to_string(),
            },
            "Completed",
            true,
        ),
        (
            TurnStatus::Failed,
            SessionDashboardLifecycle::Failed {
                turn_id: "turn-1".to_string(),
            },
            "Failed",
            true,
        ),
        (
            TurnStatus::Interrupted,
            SessionDashboardLifecycle::Cancelled {
                turn_id: "turn-1".to_string(),
            },
            "Cancelled",
            true,
        ),
    ];

    for (status, expected, label, terminal) in cases {
        let actual = SessionDashboardLifecycle::from_turn_status("turn-1", status);
        assert_eq!(actual, expected);
        assert_eq!(actual.label(), label);
        assert_eq!(actual.is_terminal(), terminal);
    }
}

#[test]
fn stale_and_late_working_observations_are_rejected() {
    let active = SessionDashboardLifecycle::active("turn-2");
    let completed = SessionDashboardLifecycle::Completed {
        turn_id: "turn-2".to_string(),
    };

    assert!(!accepts_observation_for_lifecycle(
        &active,
        "turn-1",
        &SessionDashboardLifecycle::active("turn-1")
    ));
    assert!(accepts_observation_for_lifecycle(
        &active, "turn-2", &completed
    ));
    assert!(!accepts_observation_for_lifecycle(
        &completed, "turn-2", &active
    ));
}

#[test]
fn observation_terminal_matrix_leaves_working_for_every_result() {
    let cases = [
        (
            ObservationTerminalReason::Completed,
            Some(false),
            SessionDashboardLifecycle::Partial {
                turn_id: "turn-1".to_string(),
            },
        ),
        (
            ObservationTerminalReason::Completed,
            Some(true),
            SessionDashboardLifecycle::Completed {
                turn_id: "turn-1".to_string(),
            },
        ),
        (
            ObservationTerminalReason::ProviderFailed,
            None,
            SessionDashboardLifecycle::Failed {
                turn_id: "turn-1".to_string(),
            },
        ),
        (
            ObservationTerminalReason::Cancelled,
            None,
            SessionDashboardLifecycle::Cancelled {
                turn_id: "turn-1".to_string(),
            },
        ),
        (
            ObservationTerminalReason::TimedOut,
            None,
            SessionDashboardLifecycle::TimedOut {
                turn_id: "turn-1".to_string(),
            },
        ),
    ];

    for (reason, synthesis_permitted, expected) in cases {
        let actual = lifecycle_from_observation_values(
            "turn-1".to_string(),
            Some(OrchestrationObservationStage::Terminal),
            Some(true),
            Some(reason),
            synthesis_permitted,
            Some(SessionExecutionStatus::Failed),
        );
        assert_eq!(actual, expected);
        assert!(actual.is_terminal());
    }

    assert_eq!(
        lifecycle_from_observation_values(
            "turn-1".to_string(),
            Some(OrchestrationObservationStage::Terminal),
            Some(false),
            Some(ObservationTerminalReason::Completed),
            Some(true),
            Some(SessionExecutionStatus::Completed),
        ),
        SessionDashboardLifecycle::CleanupIncomplete {
            turn_id: "turn-1".to_string(),
        }
    );
    assert_eq!(
        lifecycle_from_observation_values(
            "turn-1".to_string(),
            Some(OrchestrationObservationStage::Preparing),
            Some(true),
            None,
            None,
            Some(SessionExecutionStatus::Preparing),
        ),
        SessionDashboardLifecycle::Active {
            turn_id: "turn-1".to_string(),
        }
    );
}

#[test]
fn budget_exhaustion_projection_is_terminal() {
    let lifecycle = SessionDashboardLifecycle::BudgetExhausted {
        turn_id: "turn-1".to_string(),
    };

    assert_eq!(lifecycle.label(), "Budget exhausted");
    assert!(lifecycle.is_terminal());
}

#[test]
fn unavailable_values_are_not_coerced_to_zero() {
    let view = SessionDashboardView::unavailable();
    assert_eq!(view.output_tokens.quality, ObservationQuality::Unavailable);
    assert_eq!(view.output_tokens.value, None);
    assert_eq!(view.context_capacity.value, None);
    assert_eq!(view.forecast_confidence, None);
    assert_eq!(view.capture_health.quality, ObservationQuality::Unavailable);
}

#[test]
fn derived_count_overflow_is_unavailable() {
    let view = checked_sum([Some(usize::MAX), Some(1)]);
    assert_eq!(view, DashboardField::unavailable());
}

#[test]
fn cached_input_is_independently_unavailable() {
    let view = SessionDashboardView::unavailable();
    assert_eq!(
        view.cached_input_tokens.quality,
        ObservationQuality::Unavailable
    );
    assert_eq!(view.cached_input_tokens.value, None);
}

#[test]
fn authoritative_token_usage_preserves_turn_session_and_cached_fields() {
    let mut view = SessionDashboardView::unavailable();
    let info = TokenUsageInfo {
        total_token_usage: TokenUsage {
            input_tokens: 100,
            cached_input_tokens: 20,
            output_tokens: 30,
            reasoning_output_tokens: 0,
            total_tokens: 130,
        },
        last_token_usage: TokenUsage {
            input_tokens: 40,
            cached_input_tokens: 10,
            output_tokens: 12,
            reasoning_output_tokens: 0,
            total_tokens: 52,
        },
        model_context_window: Some(200),
    };
    view.apply_token_info(&info);
    assert_eq!(view.turn_input_tokens.value, Some(40));
    assert_eq!(view.output_tokens.value, Some(12));
    assert_eq!(view.cached_input_tokens.value, Some(10));
    assert_eq!(view.session_input_tokens.value, Some(100));
    assert_eq!(view.session_output_tokens.value, Some(30));
    assert_eq!(view.session_cached_input_tokens.value, Some(20));
    assert_eq!(view.context_used.value, None);
    assert_eq!(view.context_used.quality, ObservationQuality::Unavailable);
    assert_eq!(view.context_capacity.value, Some(200));
    assert_eq!(view.context_percent.value, None);
    assert_eq!(
        view.context_percent.quality,
        ObservationQuality::Unavailable
    );
}

#[test]
fn context_capacity_without_retained_context_is_not_rendered_as_usage() {
    let mut view = SessionDashboardView::unavailable();
    let info = TokenUsageInfo {
        total_token_usage: TokenUsage::default(),
        last_token_usage: TokenUsage::default(),
        model_context_window: Some(0),
    };
    view.apply_token_info(&info);
    assert_eq!(view.context_used.value, None);
    assert_eq!(view.context_capacity.value, Some(0));
    assert_eq!(view.context_percent.value, None);
    assert_eq!(
        view.context_percent.quality,
        ObservationQuality::Unavailable
    );
}

#[test]
fn compact_dashboard_render_is_bounded_and_privacy_safe() {
    let dashboard = DashboardRenderable::new(
        DashboardVisibility::Compact,
        None,
        None,
        0,
        &SessionDashboardLifecycle::Inactive,
        Some(crate::legacy_core::ExecutionModeSelection::Balanced),
        None,
    );
    let area = Rect::new(0, 0, 60, 6);
    let mut buffer = Buffer::empty(area);
    dashboard.render(area, &mut buffer);
    let rendered = (0..area.height)
        .map(|row| {
            (0..area.width)
                .map(|column| buffer[(column, row)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("mode Balanced"));
    assert!(rendered.contains('—'));
    insta::assert_snapshot!(rendered);
}
