use super::*;

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
fn unavailable_values_are_not_coerced_to_zero() {
    let view = SessionDashboardView::unavailable();
    assert_eq!(view.output_tokens.quality, ObservationQuality::Unavailable);
    assert_eq!(view.output_tokens.value, None);
    assert_eq!(view.context_capacity.value, None);
    assert_eq!(view.forecast_confidence, None);
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
