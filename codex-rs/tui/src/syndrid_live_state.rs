//! Syndrid-owned observable state shared by live session presentations.
//!
//! This adapter deliberately contains no execution or policy logic.  Producers
//! populate it from existing ChatWidget/app-server notifications; renderers
//! consume the cached values without doing network or Git work in a frame.

#![allow(dead_code)]

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DataQuality {
    Exact,
    Derived,
    Estimated,
    #[default]
    Unavailable,
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
}

#[cfg(test)]
mod tests {
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
}
