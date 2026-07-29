//! The compact, privacy-safe in-session Syndrid orchestration dashboard.
//!
//! The Phase 8A dashboard consumer and rendering foundation is implemented.
//! Production orchestration observation delivery is not yet connected to the
//! normal Syndrid user-turn runtime and is tracked as a separate backend
//! integration milestone.

use super::ChatWidget;
use crate::legacy_core::ExecutionModeSelection;
use crate::legacy_core::ObservationQuality;
use crate::legacy_core::Observed;
use crate::legacy_core::ObservedActiveRole;
use crate::legacy_core::OrchestrationObservationSnapshot;
use crate::legacy_core::OrchestrationObservationStage;
use crate::token_usage::TokenUsageInfo;
use std::time::Duration;

#[path = "session_dashboard_rendering.rs"]
mod session_dashboard_rendering;
pub(crate) use session_dashboard_rendering::DashboardRenderable;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DashboardVisibility {
    Hidden,
    Compact,
    Expanded,
}

impl DashboardVisibility {
    pub(crate) fn owns_primary_viewport(self) -> bool {
        self != Self::Hidden
    }

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

fn exact_count(value: i64) -> DashboardField<u64> {
    if value < 0 {
        return DashboardField::unavailable();
    }
    DashboardField {
        value: Some(value as u64),
        quality: ObservationQuality::Exact,
    }
}

fn checked_sum<I>(values: I) -> DashboardField<usize>
where
    I: IntoIterator<Item = Option<usize>>,
{
    let mut total: usize = 0;
    for value in values {
        let Some(value) = value else {
            return DashboardField::unavailable();
        };
        let Some(next) = total.checked_add(value) else {
            return DashboardField::unavailable();
        };
        total = next;
    }
    DashboardField {
        value: Some(total),
        quality: ObservationQuality::Derived,
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
    pub(crate) turn_input_tokens: DashboardField<u64>,
    pub(crate) session_input_tokens: DashboardField<u64>,
    pub(crate) session_output_tokens: DashboardField<u64>,
    pub(crate) session_cached_input_tokens: DashboardField<u64>,
    pub(crate) turn_total_tokens: DashboardField<u64>,
    pub(crate) session_total_tokens: DashboardField<u64>,
    pub(crate) context_used: DashboardField<u64>,
    pub(crate) context_capacity: DashboardField<u64>,
    pub(crate) context_percent: DashboardField<u8>,
    pub(crate) provider_reserved: DashboardField<usize>,
    pub(crate) provider_started: DashboardField<usize>,
    pub(crate) provider_active: DashboardField<usize>,
    pub(crate) provider_completed: DashboardField<usize>,
    pub(crate) provider_rejected: DashboardField<usize>,
    pub(crate) tool_reserved: DashboardField<usize>,
    pub(crate) tool_started: DashboardField<usize>,
    pub(crate) tool_active: DashboardField<usize>,
    pub(crate) tool_completed: DashboardField<usize>,
    pub(crate) tool_rejected: DashboardField<usize>,
    pub(crate) tool_output_bytes: DashboardField<usize>,
    pub(crate) tasks_queued: DashboardField<usize>,
    pub(crate) tasks_active: DashboardField<usize>,
    pub(crate) tasks_completed: DashboardField<usize>,
    pub(crate) tasks_failed: DashboardField<usize>,
    pub(crate) cleanup_requested: DashboardField<bool>,
    pub(crate) cleanup_in_progress: DashboardField<bool>,
    pub(crate) cleanup_complete: DashboardField<bool>,
    pub(crate) unresolved_reservations: DashboardField<usize>,
    pub(crate) active_role_children: DashboardField<usize>,
    pub(crate) unresolved_provider_reservations: DashboardField<usize>,
    pub(crate) unresolved_tool_reservations: DashboardField<usize>,
    pub(crate) budget_exhausted: DashboardField<String>,
    pub(crate) cancellation: DashboardField<bool>,
    pub(crate) timeout: DashboardField<bool>,
    pub(crate) synthesis_permitted: DashboardField<bool>,
    pub(crate) repair: DashboardField<String>,
    pub(crate) repair_attempts: DashboardField<usize>,
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
            turn_input_tokens: DashboardField::unavailable(),
            session_input_tokens: DashboardField::unavailable(),
            session_output_tokens: DashboardField::unavailable(),
            session_cached_input_tokens: DashboardField::unavailable(),
            turn_total_tokens: DashboardField::unavailable(),
            session_total_tokens: DashboardField::unavailable(),
            context_used: DashboardField::unavailable(),
            context_capacity: DashboardField::unavailable(),
            context_percent: DashboardField::unavailable(),
            provider_reserved: DashboardField::unavailable(),
            provider_started: DashboardField::unavailable(),
            provider_active: DashboardField::unavailable(),
            provider_completed: DashboardField::unavailable(),
            provider_rejected: DashboardField::unavailable(),
            tool_reserved: DashboardField::unavailable(),
            tool_started: DashboardField::unavailable(),
            tool_active: DashboardField::unavailable(),
            tool_completed: DashboardField::unavailable(),
            tool_rejected: DashboardField::unavailable(),
            tool_output_bytes: DashboardField::unavailable(),
            tasks_queued: DashboardField::unavailable(),
            tasks_active: DashboardField::unavailable(),
            tasks_completed: DashboardField::unavailable(),
            tasks_failed: DashboardField::unavailable(),
            cleanup_requested: DashboardField::unavailable(),
            cleanup_in_progress: DashboardField::unavailable(),
            cleanup_complete: DashboardField::unavailable(),
            unresolved_reservations: DashboardField::unavailable(),
            active_role_children: DashboardField::unavailable(),
            unresolved_provider_reservations: DashboardField::unavailable(),
            unresolved_tool_reservations: DashboardField::unavailable(),
            budget_exhausted: DashboardField::unavailable(),
            cancellation: DashboardField::unavailable(),
            timeout: DashboardField::unavailable(),
            synthesis_permitted: DashboardField::unavailable(),
            repair: DashboardField::unavailable(),
            repair_attempts: DashboardField::unavailable(),
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
        pending_mode: Option<ExecutionModeSelection>,
        token_info: Option<&TokenUsageInfo>,
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
        let unresolved = checked_sum([
            snapshot.cleanup.unresolved_provider_reservations.value,
            snapshot.cleanup.unresolved_tool_reservations.value,
        ]);
        let active_role_children = checked_sum([
            snapshot.cleanup.active_planner_children.value,
            snapshot.cleanup.active_executor_children.value,
            snapshot.cleanup.active_verifier_children.value,
            snapshot.cleanup.active_repair_children.value,
        ]);
        let mut view = Self {
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
            turn_input_tokens: DashboardField::unavailable(),
            session_input_tokens: DashboardField::unavailable(),
            session_output_tokens: DashboardField::unavailable(),
            session_cached_input_tokens: DashboardField::unavailable(),
            turn_total_tokens: DashboardField::unavailable(),
            session_total_tokens: DashboardField::unavailable(),
            context_used: DashboardField::unavailable(),
            context_capacity: DashboardField::unavailable(),
            context_percent: DashboardField::unavailable(),
            provider_reserved: DashboardField::from_observed(&snapshot.provider.reserved),
            provider_started: DashboardField::from_observed(&snapshot.provider.started),
            provider_active: DashboardField::from_observed(&snapshot.current_provider_count),
            provider_completed: DashboardField::from_observed(&snapshot.provider.completed),
            provider_rejected: DashboardField::from_observed(
                &snapshot.provider.rejected_before_start,
            ),
            tool_reserved: DashboardField::from_observed(&snapshot.tools.reserved),
            tool_started: DashboardField::from_observed(&snapshot.tools.started),
            tool_active: DashboardField::from_observed(&snapshot.current_tool_count),
            tool_completed: DashboardField::from_observed(&snapshot.tools.completed),
            tool_rejected: DashboardField::from_observed(&snapshot.tools.rejected),
            tool_output_bytes: DashboardField::from_observed(&snapshot.tools.output_bytes),
            tasks_queued: DashboardField::from_observed(&snapshot.tasks.queued),
            tasks_active: DashboardField::from_observed(&snapshot.tasks.active),
            tasks_completed: DashboardField::from_observed(&snapshot.tasks.completed),
            tasks_failed: DashboardField::from_observed(&snapshot.tasks.failed),
            cleanup_requested: DashboardField::from_observed(&snapshot.cleanup.requested),
            cleanup_in_progress: DashboardField::from_observed(&snapshot.cleanup.in_progress),
            cleanup_complete: DashboardField::from_observed(&snapshot.cleanup.complete),
            unresolved_reservations: unresolved,
            active_role_children,
            unresolved_provider_reservations: DashboardField::from_observed(
                &snapshot.cleanup.unresolved_provider_reservations,
            ),
            unresolved_tool_reservations: DashboardField::from_observed(
                &snapshot.cleanup.unresolved_tool_reservations,
            ),
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
            repair_attempts: DashboardField::from_observed(&snapshot.repair.attempts),
            capture_health: DashboardField::unavailable(),
            generation: DashboardField::from_observed(&snapshot.generation),
            sequence: DashboardField {
                value: Some(sequence),
                quality: ObservationQuality::Exact,
            },
            forecast: DashboardField::unavailable(),
            forecast_confidence: None,
        };
        if let Some(token_info) = token_info {
            view.apply_token_info(token_info);
        }
        if snapshot.stage.value == Some(OrchestrationObservationStage::Idle)
            && let Some(pending_mode) = pending_mode
        {
            view.mode = DashboardField {
                value: Some(pending_mode),
                quality: ObservationQuality::Exact,
            };
        }
        view
    }

    fn apply_token_info(&mut self, token_info: &TokenUsageInfo) {
        let turn = &token_info.last_token_usage;
        let session = &token_info.total_token_usage;
        self.turn_input_tokens = exact_count(turn.input_tokens);
        self.output_tokens = exact_count(turn.output_tokens);
        self.cached_input_tokens = exact_count(turn.cached_input_tokens);
        self.session_input_tokens = exact_count(session.input_tokens);
        self.session_output_tokens = exact_count(session.output_tokens);
        self.session_cached_input_tokens = exact_count(session.cached_input_tokens);
        self.turn_total_tokens = exact_count(turn.total_tokens);
        self.session_total_tokens = exact_count(session.total_tokens);
        // `last_token_usage.total_tokens` is provider usage for the turn, not the
        // retained prompt context. The provider does not expose retained-context
        // usage through this TUI model, so keep that field unavailable rather than
        // presenting turn usage as context usage.
        self.context_used = DashboardField::unavailable();
        self.context_capacity = token_info
            .model_context_window
            .map_or_else(DashboardField::unavailable, exact_count);
        self.context_percent = match (self.context_used.value, self.context_capacity.value) {
            (Some(used), Some(capacity)) if capacity > 0 => DashboardField {
                value: Some(((used.saturating_mul(100) / capacity).min(100)) as u8),
                quality: ObservationQuality::Derived,
            },
            _ => DashboardField::unavailable(),
        };
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
        if !accepts_newer_observation(
            self.dashboard_generation,
            self.dashboard_sequence,
            generation,
            sequence,
        ) {
            return;
        }
        if self.dashboard_generation != Some(generation) {
            self.dashboard_sequence = 0;
        }
        self.dashboard_generation = Some(generation);
        self.dashboard_sequence = sequence;
        self.dashboard_observation = Some(snapshot);
        self.request_redraw();
    }
}

fn accepts_newer_observation(
    current_generation: Option<u64>,
    current_sequence: u64,
    generation: u64,
    sequence: u64,
) -> bool {
    !current_generation.is_some_and(|current| {
        generation < current || (generation == current && sequence <= current_sequence)
    })
}

#[cfg(test)]
#[path = "session_dashboard_tests.rs"]
mod tests;
