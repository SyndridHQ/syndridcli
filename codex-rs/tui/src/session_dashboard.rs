//! The compact, privacy-safe in-session Syndrid orchestration dashboard.

use super::ChatWidget;
use crate::legacy_core::ExecutionModeSelection;
use crate::legacy_core::ObservationQuality;
use crate::legacy_core::ObservationTerminalReason;
use crate::legacy_core::Observed;
use crate::legacy_core::ObservedActiveRole;
use crate::legacy_core::OrchestrationObservationSnapshot;
use crate::legacy_core::OrchestrationObservationStage;
use crate::legacy_core::SessionExecutionStatus;
use crate::token_usage::TokenUsageInfo;
use codex_app_server_protocol::TurnStatus;
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

/// The bounded lifecycle projection owned by the TUI for one real user turn.
///
/// This state controls presentation only. The orchestration coordinator and app-server remain
/// the authorities that decide the actual turn result; observations and turn notifications are
/// translated into this projection at the ChatWidget boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SessionDashboardLifecycle {
    Inactive,
    Active { turn_id: String },
    Completed { turn_id: String },
    Partial { turn_id: String },
    Failed { turn_id: String },
    Cancelled { turn_id: String },
    TimedOut { turn_id: String },
    BudgetExhausted { turn_id: String },
    CleanupIncomplete { turn_id: String },
}

impl SessionDashboardLifecycle {
    pub(crate) fn active(turn_id: impl Into<String>) -> Self {
        Self::Active {
            turn_id: turn_id.into(),
        }
    }

    pub(crate) fn from_observation(snapshot: &OrchestrationObservationSnapshot) -> Self {
        let Some(turn_id) = snapshot.run_id.value.clone() else {
            return Self::Inactive;
        };
        lifecycle_from_observation_values(
            turn_id,
            snapshot.stage.value,
            snapshot.cleanup.complete.value,
            snapshot.terminal_reason.value.flatten(),
            snapshot.synthesis_permitted.value,
            snapshot.lifecycle.value,
        )
    }

    pub(crate) fn from_turn_status(turn_id: impl Into<String>, status: TurnStatus) -> Self {
        let turn_id = turn_id.into();
        match status {
            TurnStatus::Completed => Self::Completed { turn_id },
            TurnStatus::Interrupted => Self::Cancelled { turn_id },
            TurnStatus::Failed => Self::Failed { turn_id },
            TurnStatus::InProgress => Self::Active { turn_id },
        }
    }

    pub(crate) fn turn_id(&self) -> Option<&str> {
        match self {
            Self::Inactive => None,
            Self::Active { turn_id }
            | Self::Completed { turn_id }
            | Self::Partial { turn_id }
            | Self::Failed { turn_id }
            | Self::Cancelled { turn_id }
            | Self::TimedOut { turn_id }
            | Self::BudgetExhausted { turn_id }
            | Self::CleanupIncomplete { turn_id } => Some(turn_id),
        }
    }

    pub(crate) fn is_terminal(&self) -> bool {
        !matches!(self, Self::Inactive | Self::Active { .. })
    }

    pub(crate) fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Inactive => "Unavailable",
            Self::Active { .. } => "Working",
            Self::Completed { .. } => "Completed",
            Self::Partial { .. } => "Partial",
            Self::Failed { .. } => "Failed",
            Self::Cancelled { .. } => "Cancelled",
            Self::TimedOut { .. } => "Timed out",
            Self::BudgetExhausted { .. } => "Budget exhausted",
            Self::CleanupIncomplete { .. } => "Cleanup incomplete",
        }
    }
}

fn lifecycle_from_observation_values(
    turn_id: String,
    stage: Option<OrchestrationObservationStage>,
    cleanup_complete: Option<bool>,
    terminal_reason: Option<ObservationTerminalReason>,
    synthesis_permitted: Option<bool>,
    lifecycle: Option<SessionExecutionStatus>,
) -> SessionDashboardLifecycle {
    if stage != Some(OrchestrationObservationStage::Terminal) {
        return SessionDashboardLifecycle::Active { turn_id };
    }
    if cleanup_complete == Some(false) {
        return SessionDashboardLifecycle::CleanupIncomplete { turn_id };
    }
    match terminal_reason {
        Some(ObservationTerminalReason::Completed) if synthesis_permitted == Some(false) => {
            SessionDashboardLifecycle::Partial { turn_id }
        }
        Some(ObservationTerminalReason::Completed) => {
            SessionDashboardLifecycle::Completed { turn_id }
        }
        Some(ObservationTerminalReason::Cancelled) => {
            SessionDashboardLifecycle::Cancelled { turn_id }
        }
        Some(ObservationTerminalReason::TimedOut) => {
            SessionDashboardLifecycle::TimedOut { turn_id }
        }
        Some(ObservationTerminalReason::BudgetExhausted(_)) => {
            SessionDashboardLifecycle::BudgetExhausted { turn_id }
        }
        Some(
            ObservationTerminalReason::ValidationFailed
            | ObservationTerminalReason::ProviderFailed
            | ObservationTerminalReason::ToolFailed
            | ObservationTerminalReason::VerifierRejected
            | ObservationTerminalReason::RepairFailed
            | ObservationTerminalReason::LifecycleViolation
            | ObservationTerminalReason::RoutingFailure
            | ObservationTerminalReason::InternalCoordinatorFailure,
        ) => SessionDashboardLifecycle::Failed { turn_id },
        None => match lifecycle {
            Some(SessionExecutionStatus::Completed) if synthesis_permitted == Some(false) => {
                SessionDashboardLifecycle::Partial { turn_id }
            }
            Some(SessionExecutionStatus::Completed) => {
                SessionDashboardLifecycle::Completed { turn_id }
            }
            Some(SessionExecutionStatus::Cancelled) => {
                SessionDashboardLifecycle::Cancelled { turn_id }
            }
            Some(SessionExecutionStatus::TimedOut) => {
                SessionDashboardLifecycle::TimedOut { turn_id }
            }
            _ => SessionDashboardLifecycle::Failed { turn_id },
        },
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
        if self.dashboard_visibility == DashboardVisibility::Hidden
            && matches!(
                self.dashboard_lifecycle,
                SessionDashboardLifecycle::Inactive
            )
        {
            return;
        }
        self.dashboard_visibility = self.dashboard_visibility.toggle();
        self.request_redraw();
    }

    pub(crate) fn close_dashboard(&mut self) {
        self.dashboard_visibility = DashboardVisibility::Hidden;
        self.request_redraw();
    }

    pub(super) fn reset_dashboard_for_session(&mut self) {
        self.dashboard_visibility = DashboardVisibility::Hidden;
        self.dashboard_lifecycle = SessionDashboardLifecycle::Inactive;
        self.dashboard_observation = None;
        self.dashboard_generation = None;
        self.dashboard_sequence = 0;
    }

    pub(super) fn begin_dashboard_turn(&mut self, turn_id: Option<&str>) {
        let Some(turn_id) = turn_id else {
            return;
        };
        self.dashboard_lifecycle = SessionDashboardLifecycle::active(turn_id);
        self.dashboard_observation = None;
        self.dashboard_generation = None;
        self.dashboard_sequence = 0;
    }

    pub(super) fn finish_dashboard_turn(&mut self, turn_id: &str, status: TurnStatus) {
        if self.dashboard_lifecycle.turn_id() != Some(turn_id)
            || self.dashboard_lifecycle.is_terminal()
        {
            return;
        }
        self.dashboard_lifecycle = SessionDashboardLifecycle::from_turn_status(turn_id, status);
        self.request_redraw();
    }

    pub(super) fn finish_dashboard_turn_as_budget_exhausted(&mut self, turn_id: &str) {
        if self.dashboard_lifecycle.turn_id() != Some(turn_id)
            || self.dashboard_lifecycle.is_terminal()
        {
            return;
        }
        self.dashboard_lifecycle = SessionDashboardLifecycle::BudgetExhausted {
            turn_id: turn_id.to_string(),
        };
        self.request_redraw();
    }

    pub(crate) fn update_orchestration_observation(
        &mut self,
        generation: u64,
        sequence: u64,
        snapshot: OrchestrationObservationSnapshot,
    ) {
        let Some(turn_id) = snapshot.run_id.value.as_deref() else {
            return;
        };
        if self.dashboard_lifecycle.turn_id() != Some(turn_id) {
            return;
        }
        if !accepts_newer_observation(
            self.dashboard_generation,
            self.dashboard_sequence,
            generation,
            sequence,
        ) {
            return;
        }
        let lifecycle = SessionDashboardLifecycle::from_observation(&snapshot);
        if !accepts_observation_for_lifecycle(&self.dashboard_lifecycle, turn_id, &lifecycle) {
            return;
        }
        if self.dashboard_generation != Some(generation) {
            self.dashboard_sequence = 0;
        }
        self.dashboard_generation = Some(generation);
        self.dashboard_sequence = sequence;
        self.dashboard_lifecycle = lifecycle;
        self.dashboard_observation = Some(snapshot);
        self.bottom_pane
            .apply_syndrid_observation(self.dashboard_observation.as_ref());
        self.request_redraw();
    }
}

fn accepts_observation_for_lifecycle(
    current: &SessionDashboardLifecycle,
    turn_id: &str,
    candidate: &SessionDashboardLifecycle,
) -> bool {
    current.turn_id() == Some(turn_id) && !(current.is_terminal() && candidate.is_active())
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
