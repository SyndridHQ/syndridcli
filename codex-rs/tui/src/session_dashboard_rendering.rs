use super::DashboardField;
use super::DashboardVisibility;
use super::SessionDashboardLifecycle;
use super::SessionDashboardView;
use crate::legacy_core::ExecutionModeSelection;
use crate::render::renderable::Renderable;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use std::fmt::Debug;

pub(crate) struct DashboardRenderable {
    visibility: DashboardVisibility,
    view: SessionDashboardView,
}

impl DashboardRenderable {
    pub(crate) fn new(
        visibility: DashboardVisibility,
        snapshot: Option<&crate::legacy_core::OrchestrationObservationSnapshot>,
        generation: Option<u64>,
        sequence: u64,
        lifecycle: &SessionDashboardLifecycle,
        pending_mode: Option<ExecutionModeSelection>,
        token_info: Option<&crate::token_usage::TokenUsageInfo>,
    ) -> Self {
        let mut view = snapshot
            .map(|snapshot| {
                SessionDashboardView::from_snapshot(
                    snapshot,
                    sequence,
                    pending_mode.clone(),
                    token_info,
                )
            })
            .unwrap_or_else(SessionDashboardView::unavailable);
        if snapshot.is_none()
            && let Some(pending_mode) = pending_mode
        {
            view.mode = DashboardField {
                value: Some(pending_mode),
                quality: crate::legacy_core::ObservationQuality::Exact,
            };
        }
        if let Some(generation) = generation {
            view.generation = DashboardField {
                value: Some(generation),
                quality: crate::legacy_core::ObservationQuality::Exact,
            };
        }
        if !matches!(lifecycle, SessionDashboardLifecycle::Inactive) {
            view.lifecycle = DashboardField {
                value: Some(lifecycle.label().to_string()),
                quality: crate::legacy_core::ObservationQuality::Exact,
            };
        }
        Self { visibility, view }
    }

    fn lines(&self, width: usize) -> Vec<Line<'static>> {
        let mode = display_mode(&self.view.mode);
        let lifecycle = field_text(&self.view.lifecycle);
        let stage = field_text(&self.view.stage);
        let role = field_text(&self.view.role);
        let tokens = format!(
            "turn {}/{}/{} session {}/{}/{}",
            field_text(&self.view.turn_input_tokens),
            field_text(&self.view.output_tokens),
            field_text(&self.view.cached_input_tokens),
            field_text(&self.view.session_input_tokens),
            field_text(&self.view.session_output_tokens),
            field_text(&self.view.session_cached_input_tokens)
        );
        let totals = format!(
            "totals {}/{}",
            field_text(&self.view.turn_total_tokens),
            field_text(&self.view.session_total_tokens),
        );
        let activity = format!(
            "tasks {}/{}/{}  provider {}/{}/{}  tools {}/{}/{}",
            field_text(&self.view.tasks_queued),
            field_text(&self.view.tasks_active),
            field_text(&self.view.tasks_completed),
            field_text(&self.view.provider_started),
            field_text(&self.view.provider_active),
            field_text(&self.view.provider_completed),
            field_text(&self.view.tool_started),
            field_text(&self.view.tool_active),
            field_text(&self.view.tool_completed),
        );
        let mut rows = vec![
            format!("mode {mode} | lifecycle {lifecycle} | stage {stage} | role {role}"),
            format!(
                "tokens {tokens} {totals} | context {} / {} ({}%)",
                field_text(&self.view.context_used),
                field_text(&self.view.context_capacity),
                field_text(&self.view.context_percent),
            ),
            format!("{activity} | elapsed {}", field_text(&self.view.elapsed)),
            format!(
                "status {} | budget {} | cleanup {} | reservations {}",
                field_text(&self.view.terminal),
                field_text(&self.view.budget_exhausted),
                field_text(&self.view.cleanup_complete),
                field_text(&self.view.unresolved_reservations),
            ),
        ];
        if self.visibility == DashboardVisibility::Expanded {
            rows.extend([
                format!(
                    "cancelled {} | timed out {} | synthesis {}",
                    field_text(&self.view.cancellation),
                    field_text(&self.view.timeout),
                    field_text(&self.view.synthesis_permitted)
                ),
                format!(
                    "capture generation {} | event {}",
                    field_text(&self.view.generation),
                    field_text(&self.view.sequence)
                ),
                format!(
                    "repair {} attempts {} | capture {}",
                    field_text(&self.view.repair),
                    field_text(&self.view.repair_attempts),
                    field_text(&self.view.capture_health)
                ),
                format!(
                    "reservations provider {}/{} tool {}/{} | output bytes {}",
                    field_text(&self.view.provider_reserved),
                    field_text(&self.view.provider_rejected),
                    field_text(&self.view.tool_reserved),
                    field_text(&self.view.tool_rejected),
                    field_text(&self.view.tool_output_bytes)
                ),
                format!(
                    "cleanup requested {} in progress {} complete {} | active children {}",
                    field_text(&self.view.cleanup_requested),
                    field_text(&self.view.cleanup_in_progress),
                    field_text(&self.view.cleanup_complete),
                    field_text(&self.view.active_role_children)
                ),
                format!(
                    "forecast {} ({:?})",
                    field_text(&self.view.forecast),
                    self.view.forecast_confidence
                ),
                "quality: exact | derived ≈ | estimated ~ | unavailable —".to_string(),
                "quota, context capacity, latency, and forecasts require authoritative sources"
                    .to_string(),
                "workflow is observed runtime activity; private reasoning is never displayed"
                    .to_string(),
                "Esc: close   /dashboard: toggle".to_string(),
            ]);
        }
        rows.into_iter()
            .map(|row| {
                let normalized = row.replace('/', std::path::MAIN_SEPARATOR_STR);
                let fitted = crate::syndrid_visuals::fit_text(&normalized, width.saturating_sub(2));
                Line::from(fitted.replace(std::path::MAIN_SEPARATOR, "/"))
            })
            .collect()
    }
}

impl Renderable for DashboardRenderable {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 3 {
            return;
        }
        let block = Block::default()
            .borders(Borders::ALL)
            .title("SYNDRID DASHBOARD");
        let inner = block.inner(area);
        block.render(area, buf);
        Paragraph::new(self.lines(usize::from(inner.width))).render(inner, buf);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        if self.visibility == DashboardVisibility::Expanded {
            14
        } else {
            6
        }
    }
}

fn field_text<T: Debug>(field: &DashboardField<T>) -> String {
    let Some(value) = field.value.as_ref() else {
        return "—".to_string();
    };
    let value = format!("{value:?}");
    match field.quality {
        crate::legacy_core::ObservationQuality::Exact => value,
        crate::legacy_core::ObservationQuality::Derived => format!("≈{value}"),
        crate::legacy_core::ObservationQuality::Estimated => format!("~{value}"),
        crate::legacy_core::ObservationQuality::Unavailable => "—".to_string(),
    }
}

fn display_mode(field: &DashboardField<ExecutionModeSelection>) -> String {
    field
        .value
        .as_ref()
        .map(|mode| match mode {
            ExecutionModeSelection::Fast => "Fast",
            ExecutionModeSelection::Balanced => "Balanced",
            ExecutionModeSelection::UsageSaver => "Usage Saver",
            ExecutionModeSelection::Deep => "Deep",
            ExecutionModeSelection::Custom(_) => "Custom",
        })
        .unwrap_or("—")
        .to_string()
}
