//! Syndrid-owned observable state shared by live session presentations.
//!
//! This adapter deliberately contains no execution or policy logic.  Producers
//! populate it from existing ChatWidget/app-server notifications; renderers
//! consume the cached values without doing network or Git work in a frame.

#![allow(dead_code)]

use crate::bottom_pane::SyndridContextUsage;
use crate::token_usage::TokenUsage;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DataQuality {
    Exact,
    Derived,
    Estimated,
    #[default]
    Unavailable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LifecycleState {
    #[default]
    Unavailable,
    Working,
    Ready,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ActivityStatus {
    #[default]
    Unavailable,
    Running,
    Passed,
    Failed,
    Cancelled,
    Blocked,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum VerificationStatus {
    #[default]
    NotRun,
    Running,
    Passed,
    Failed,
    Cancelled,
    Blocked,
    Unavailable,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ActivityEvent {
    pub(crate) event_id: Option<String>,
    pub(crate) elapsed_seconds: Option<u64>,
    pub(crate) event_type: String,
    pub(crate) actor: Option<String>,
    pub(crate) summary: String,
    pub(crate) status: ActivityStatus,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) correlation_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChangeEntry {
    pub(crate) path: String,
    pub(crate) change_type: Option<String>,
    pub(crate) additions: Option<i64>,
    pub(crate) deletions: Option<i64>,
    pub(crate) state: Option<String>,
    pub(crate) actor: Option<String>,
    pub(crate) changed_at: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChangesProjection {
    pub(crate) modified: Option<usize>,
    pub(crate) added: Option<usize>,
    pub(crate) deleted: Option<usize>,
    pub(crate) untracked: Option<usize>,
    pub(crate) additions: Option<i64>,
    pub(crate) deletions: Option<i64>,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: Option<String>,
    pub(crate) commit_state: Option<String>,
    pub(crate) files: Vec<ChangeEntry>,
    pub(crate) diff_summary: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct VerificationItem {
    pub(crate) name: String,
    pub(crate) status: VerificationStatus,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) evidence: Option<String>,
    pub(crate) actor: Option<String>,
    pub(crate) retry_count: u32,
    pub(crate) evidence_quality: DataQuality,
}

/// Stable usage fields reserved for a future orchestration view.
///
/// Producers should populate these only from measured workflow events or an explicit
/// user-selected budget; renderers must keep absent fields unavailable.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct WorkflowUsage {
    pub(crate) baseline_single_agent_tokens: Option<i64>,
    pub(crate) selected_multiplier: Option<f64>,
    pub(crate) budget_ceiling: Option<i64>,
    pub(crate) actual_usage: Option<i64>,
    pub(crate) remaining_budget: Option<i64>,
    pub(crate) per_agent_usage: Vec<(String, i64)>,
    pub(crate) predicted_speedup: Option<f64>,
    pub(crate) confidence: DataQuality,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LiveView {
    #[default]
    Dashboard,
    Activity,
    Changes,
    Verification,
}

impl LiveView {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Dashboard => Self::Activity,
            Self::Activity => Self::Changes,
            Self::Changes => Self::Verification,
            Self::Verification => Self::Dashboard,
        }
    }

    pub(crate) fn previous(self) -> Self {
        match self {
            Self::Dashboard => Self::Verification,
            Self::Activity => Self::Dashboard,
            Self::Changes => Self::Activity,
            Self::Verification => Self::Changes,
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Activity => "Activity",
            Self::Changes => "Changes",
            Self::Verification => "Verification",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LiveSessionState {
    pub(crate) view: LiveView,
    pub(crate) task: Option<String>,
    pub(crate) step: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) context_used: Option<i64>,
    pub(crate) context_window: Option<i64>,
    pub(crate) activity_count: usize,
    pub(crate) files_changed: Option<usize>,
    pub(crate) additions: Option<i64>,
    pub(crate) deletions: Option<i64>,
    pub(crate) last_error: Option<String>,
    pub(crate) lifecycle: LifecycleState,
    pub(crate) workflow_stage: Option<String>,
    pub(crate) wait_reason: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) workspace: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: Option<String>,
    pub(crate) identity: Option<String>,
    pub(crate) collaboration_mode: Option<String>,
    pub(crate) active_agents: Option<usize>,
    pub(crate) max_concurrency: Option<usize>,
    pub(crate) approval_mode: Option<String>,
    pub(crate) access_mode: Option<String>,
    pub(crate) command_state: Option<String>,
    pub(crate) token_usage: Option<TokenUsage>,
    pub(crate) context: Option<SyndridContextUsage>,
    pub(crate) compactions: Option<u64>,
    pub(crate) performance: PerformanceProjection,
    pub(crate) validation: ValidationSummary,
    pub(crate) activity: Vec<ActivityEvent>,
    pub(crate) changes: ChangesProjection,
    pub(crate) verifications: Vec<VerificationItem>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceProjection {
    pub(crate) output_tokens_per_second: Option<String>,
    pub(crate) first_token_latency: Option<String>,
    pub(crate) turn_latency: Option<String>,
    pub(crate) eta: Option<String>,
    pub(crate) forecast_tokens: Option<i64>,
    pub(crate) forecast_context: Option<String>,
    pub(crate) forecast_quota: Option<String>,
    pub(crate) confidence: DataQuality,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ValidationSummary {
    pub(crate) tests: Option<String>,
    pub(crate) build: Option<String>,
    pub(crate) check: Option<String>,
    pub(crate) verification: Option<String>,
    pub(crate) last_failure: Option<String>,
    pub(crate) evidence_count: Option<usize>,
}

impl LiveSessionState {
    pub(crate) fn cycle_forward(&mut self) {
        self.view = self.view.next();
    }

    pub(crate) fn cycle_backward(&mut self) {
        self.view = self.view.previous();
    }

    pub(crate) fn unavailable<T>() -> Option<T> {
        None
    }

    pub(crate) fn record_activity(&mut self, event: ActivityEvent) {
        if let Some(event_id) = event.event_id.as_deref()
            && let Some(existing) = self
                .activity
                .iter_mut()
                .find(|existing| existing.event_id.as_deref() == Some(event_id))
        {
            *existing = event;
            return;
        }
        self.activity.push(event);
        const MAX_ACTIVITY_EVENTS: usize = 200;
        if self.activity.len() > MAX_ACTIVITY_EVENTS {
            let excess = self.activity.len() - MAX_ACTIVITY_EVENTS;
            self.activity.drain(..excess);
        }
        self.activity_count = self.activity.len();
    }
}

#[cfg(test)]
mod tests {
    use super::ActivityEvent;
    use super::ActivityStatus;
    use super::LiveSessionState;
    use super::LiveView;

    #[test]
    fn live_views_cycle_in_design_order() {
        assert_eq!(LiveView::Dashboard.next(), LiveView::Activity);
        assert_eq!(LiveView::Activity.next(), LiveView::Changes);
        assert_eq!(LiveView::Changes.next(), LiveView::Verification);
        assert_eq!(LiveView::Verification.next(), LiveView::Dashboard);
    }

    #[test]
    fn live_views_cycle_backward_in_design_order() {
        assert_eq!(LiveView::Dashboard.previous(), LiveView::Verification);
        assert_eq!(LiveView::Verification.previous(), LiveView::Changes);
    }

    #[test]
    fn activity_is_deduplicated_and_bounded() {
        let mut state = LiveSessionState::default();
        for index in 0..205 {
            state.record_activity(ActivityEvent {
                event_id: Some(format!("event-{index}")),
                summary: index.to_string(),
                status: ActivityStatus::Running,
                ..Default::default()
            });
        }
        state.record_activity(ActivityEvent {
            event_id: Some("event-204".to_string()),
            summary: "updated".to_string(),
            status: ActivityStatus::Passed,
            ..Default::default()
        });
        assert_eq!(state.activity.len(), 200);
        assert_eq!(
            state.activity.last().map(|event| event.summary.as_str()),
            Some("updated")
        );
    }
}
