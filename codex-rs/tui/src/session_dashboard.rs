//! The compact, privacy-safe in-session Syndrid orchestration dashboard.

use super::ChatWidget;
use crate::legacy_core::ExecutionModeSelection;
use crate::legacy_core::ObservationQuality;
use crate::legacy_core::Observed;
use crate::legacy_core::ObservedActiveRole;
use crate::legacy_core::OrchestrationObservationSnapshot;
use crate::legacy_core::OrchestrationObservationStage;
use crate::render::renderable::Renderable;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DashboardVisibility {
    Hidden,
    Compact,
    Expanded,
}

impl DashboardVisibility {
    pub(crate) fn toggle(self) -> Self {
        match self {
            Self::Hidden => Self::Compact,
            Self::Compact => Self::Expanded,
            Self::Expanded => Self::Hidden,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DashboardConfidence {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DashboardField<T> {
    pub(crate) value: Option<T>,
    pub(crate) quality: ObservationQuality,
}

impl<T> DashboardField<T> {
    fn from_observed(value: &Observed<T>) -> Self
    where
        T: Clone,
    {
        Self {
            value: value.value.clone(),
            quality: value.quality,
        }
    }

    fn unavailable() -> Self {
        Self {
            value: None,
            quality: ObservationQuality::Unavailable,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SessionDashboardView {
    pub(crate) mode: DashboardField<ExecutionModeSelection>,
    pub(crate) lifecycle: DashboardField<String>,
    pub(crate) stage: DashboardField<String>,
    pub(crate) role: DashboardField<String>,
    pub(crate) terminal: DashboardField<String>,
    pub(crate) elapsed: DashboardField<Duration>,
    pub(crate) output_tokens: DashboardField<u64>,
    pub(crate) cached_input_tokens: DashboardField<u64>,
    pub(crate) context_used: DashboardField<u64>,
    pub(crate) context_capacity: DashboardField<u64>,
    pub(crate) provider_active: DashboardField<usize>,
    pub(crate) provider_completed: DashboardField<usize>,
    pub(crate) tool_active: DashboardField<usize>,
    pub(crate) tool_completed: DashboardField<usize>,
    pub(crate) tasks_active: DashboardField<usize>,
    pub(crate) tasks_completed: DashboardField<usize>,
    pub(crate) tasks_failed: DashboardField<usize>,
    pub(crate) cleanup_complete: DashboardField<bool>,
    pub(crate) unresolved_reservations: DashboardField<usize>,
    pub(crate) budget_exhausted: DashboardField<String>,
    pub(crate) cancellation: DashboardField<bool>,
    pub(crate) timeout: DashboardField<bool>,
    pub(crate) synthesis_permitted: DashboardField<bool>,
    pub(crate) repair: DashboardField<String>,
    pub(crate) capture_health: DashboardField<String>,
    pub(crate) generation: DashboardField<u64>,
    pub(crate) sequence: DashboardField<u64>,
    pub(crate) forecast: DashboardField<Duration>,
    pub(crate) forecast_confidence: Option<DashboardConfidence>,
}

impl SessionDashboardView {
    pub(crate) fn unavailable() -> Self {
        Self {
            mode: DashboardField::unavailable(),
            lifecycle: DashboardField::unavailable(),
            stage: DashboardField::unavailable(),
            role: DashboardField::unavailable(),
            terminal: DashboardField::unavailable(),
            elapsed: DashboardField::unavailable(),
            output_tokens: DashboardField::unavailable(),
            cached_input_tokens: DashboardField::unavailable(),
            context_used: DashboardField::unavailable(),
            context_capacity: DashboardField::unavailable(),
            provider_active: DashboardField::unavailable(),
            provider_completed: DashboardField::unavailable(),
            tool_active: DashboardField::unavailable(),
            tool_completed: DashboardField::unavailable(),
            tasks_active: DashboardField::unavailable(),
            tasks_completed: DashboardField::unavailable(),
            tasks_failed: DashboardField::unavailable(),
            cleanup_complete: DashboardField::unavailable(),
            unresolved_reservations: DashboardField::unavailable(),
            budget_exhausted: DashboardField::unavailable(),
            cancellation: DashboardField::unavailable(),
            timeout: DashboardField::unavailable(),
            synthesis_permitted: DashboardField::unavailable(),
            repair: DashboardField::unavailable(),
            capture_health: DashboardField::unavailable(),
            generation: DashboardField::unavailable(),
            sequence: DashboardField::unavailable(),
            forecast: DashboardField::unavailable(),
            forecast_confidence: None,
        }
    }

    pub(crate) fn from_snapshot(
        snapshot: &OrchestrationObservationSnapshot,
        sequence: u64,
    ) -> Self {
        let budget_exhausted = snapshot
            .budgets
            .iter()
            .find(|budget| budget.exhausted.value == Some(true))
            .map(|budget| DashboardField {
                value: Some(format!("{:?}", budget.category)),
                quality: budget.exhausted.quality,
            })
            .unwrap_or_else(DashboardField::unavailable);
        let unresolved = match (
            snapshot.cleanup.unresolved_provider_reservations.value,
            snapshot.cleanup.unresolved_tool_reservations.value,
        ) {
            (Some(provider), Some(tool)) => DashboardField {
                value: Some(provider + tool),
                quality: ObservationQuality::Derived,
            },
            _ => DashboardField::unavailable(),
        };
        Self {
            mode: DashboardField::from_observed(&snapshot.selected_mode),
            lifecycle: DashboardField::from_observed(&snapshot.lifecycle)
                .map_value(|value| format!("{value:?}")),
            stage: DashboardField::from_observed(&snapshot.stage).map_value(stage_name),
            role: DashboardField::from_observed(&snapshot.active_role).map_value(role_name),
            terminal: DashboardField::from_observed(&snapshot.terminal_reason)
                .map_value(|value| format!("{value:?}")),
            elapsed: DashboardField::from_observed(&snapshot.elapsed),
            output_tokens: DashboardField::from_observed(&snapshot.provider.output_tokens),
            cached_input_tokens: DashboardField::from_observed(
                &snapshot.provider.cached_input_tokens,
            ),
            context_used: DashboardField::unavailable(),
            context_capacity: DashboardField::unavailable(),
            provider_active: DashboardField::from_observed(&snapshot.current_provider_count),
            provider_completed: DashboardField::from_observed(&snapshot.provider.completed),
            tool_active: DashboardField::from_observed(&snapshot.current_tool_count),
            tool_completed: DashboardField::from_observed(&snapshot.tools.completed),
            tasks_active: DashboardField::from_observed(&snapshot.tasks.active),
            tasks_completed: DashboardField::from_observed(&snapshot.tasks.completed),
            tasks_failed: DashboardField::from_observed(&snapshot.tasks.failed),
            cleanup_complete: DashboardField::from_observed(&snapshot.cleanup.complete),
            unresolved_reservations: unresolved,
            budget_exhausted,
            cancellation: DashboardField::from_observed(&snapshot.cancelled),
            timeout: DashboardField::from_observed(&snapshot.timed_out),
            synthesis_permitted: DashboardField::from_observed(&snapshot.synthesis_permitted),
            repair: DashboardField {
                value: snapshot
                    .repair
                    .result
                    .value
                    .as_ref()
                    .map(|result| format!("{result:?}")),
                quality: snapshot.repair.result.quality,
            },
            capture_health: DashboardField {
                value: Some("Healthy".to_string()),
                quality: ObservationQuality::Derived,
            },
            generation: DashboardField::from_observed(&snapshot.generation),
            sequence: DashboardField {
                value: Some(sequence),
                quality: ObservationQuality::Exact,
            },
            forecast: DashboardField::unavailable(),
            forecast_confidence: None,
        }
    }
}

impl<T> DashboardField<T> {
    fn map_value<U>(self, map: impl FnOnce(T) -> U) -> DashboardField<U> {
        DashboardField {
            value: self.value.map(map),
            quality: self.quality,
        }
    }
}

pub(crate) struct DashboardRenderable {
    visibility: DashboardVisibility,
    view: SessionDashboardView,
}

impl DashboardRenderable {
    pub(crate) fn new(
        visibility: DashboardVisibility,
        snapshot: Option<&OrchestrationObservationSnapshot>,
        generation: Option<u64>,
        sequence: u64,
    ) -> Self {
        let mut view = snapshot
            .map(|snapshot| SessionDashboardView::from_snapshot(snapshot, sequence))
            .unwrap_or_else(SessionDashboardView::unavailable);
        if let Some(generation) = generation {
            view.generation = DashboardField {
                value: Some(generation),
                quality: ObservationQuality::Exact,
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
            "out {}  cached {}",
            field_text(&self.view.output_tokens),
            field_text(&self.view.cached_input_tokens)
        );
        let activity = format!(
            "tasks {}/{}  provider {}/{}  tools {}/{}",
            field_text(&self.view.tasks_active),
            field_text(&self.view.tasks_completed),
            field_text(&self.view.provider_active),
            field_text(&self.view.provider_completed),
            field_text(&self.view.tool_active),
            field_text(&self.view.tool_completed),
        );
        let mut rows = vec![
            format!("mode {mode} | lifecycle {lifecycle} | stage {stage} | role {role}"),
            format!(
                "tokens {tokens} | context {} / {}",
                field_text(&self.view.context_used),
                field_text(&self.view.context_capacity)
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
                    "repair {} | capture {}",
                    field_text(&self.view.repair),
                    field_text(&self.view.capture_health)
                ),
                format!(
                    "forecast {} ({:?})",
                    field_text(&self.view.forecast),
                    self.view.forecast_confidence
                ),
                "quota, context capacity, latency, and forecasts require authoritative sources"
                    .to_string(),
                "workflow is observed runtime activity; private reasoning is never displayed"
                    .to_string(),
                "Esc: close   /dashboard: toggle".to_string(),
            ]);
        }
        rows.into_iter()
            .map(|row| {
                Line::from(crate::syndrid_visuals::fit_text(
                    &row,
                    width.saturating_sub(2),
                ))
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
            11
        } else {
            6
        }
    }
}

fn field_text<T: std::fmt::Debug>(field: &DashboardField<T>) -> String {
    field
        .value
        .as_ref()
        .map(|value| format!("{value:?}"))
        .unwrap_or_else(|| "—".to_string())
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

fn stage_name(stage: OrchestrationObservationStage) -> String {
    format!("{stage:?}")
}

fn role_name(role: ObservedActiveRole) -> String {
    format!("{role:?}")
}

impl ChatWidget {
    pub(crate) fn toggle_dashboard(&mut self) {
        self.dashboard_visibility = self.dashboard_visibility.toggle();
        self.request_redraw();
    }

    pub(crate) fn close_dashboard(&mut self) {
        self.dashboard_visibility = DashboardVisibility::Hidden;
        self.request_redraw();
    }

    pub(crate) fn update_orchestration_observation(
        &mut self,
        generation: u64,
        sequence: u64,
        snapshot: OrchestrationObservationSnapshot,
    ) {
        if self.dashboard_frozen {
            return;
        }
        if self
            .dashboard_generation
            .is_some_and(|current| generation < current)
        {
            return;
        }
        if self.dashboard_generation == Some(generation) && sequence <= self.dashboard_sequence {
            return;
        }
        if self.dashboard_generation != Some(generation) {
            self.dashboard_sequence = 0;
        }
        self.dashboard_generation = Some(generation);
        self.dashboard_sequence = sequence;
        self.dashboard_frozen = snapshot.terminal.value.is_some();
        self.dashboard_observation = Some(snapshot);
        self.request_redraw();
    }
}

#[cfg(test)]
#[path = "session_dashboard_tests.rs"]
mod tests;
