use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::syndrid_visuals as sv;
use crate::token_usage::TokenUsage;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyndridStatusSnapshot {
    pub(crate) identity: String,
    pub(crate) session_id: Option<String>,
    pub(crate) workspace: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) current_task: Option<String>,
    pub(crate) model: String,
    pub(crate) reasoning: Option<String>,
    pub(crate) profile: Option<String>,
    pub(crate) sandbox: String,
    pub(crate) approval: String,
    pub(crate) plan_mode: bool,
    pub(crate) context: Option<SyndridContextUsage>,
    /// Cumulative account token activity from `GetAccountTokenUsageResponse.summary.lifetime_tokens`.
    /// This is the account lifetime scope returned by `/usage cumulative`, not thread usage.
    pub(crate) tokens_sparked: Option<i64>,
    pub(crate) running_subagents: usize,
    /// The exact per-session accounting received from the active thread.
    pub(crate) token_usage: Option<TokenUsage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyndridContextUsage {
    pub(crate) used_tokens: i64,
    pub(crate) context_window: i64,
}

pub(crate) fn status_line(
    snapshot: &SyndridStatusSnapshot,
    width: usize,
    _is_task_running: bool,
    _active_agent_label: Option<&str>,
    _waiting: bool,
) -> Option<Line<'static>> {
    status_lines(
        snapshot,
        width,
        _is_task_running,
        _active_agent_label,
        _waiting,
    )
    .into_iter()
    .next()
}

pub(crate) fn status_lines(
    snapshot: &SyndridStatusSnapshot,
    width: usize,
    _is_task_running: bool,
    _active_agent_label: Option<&str>,
    _waiting: bool,
) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let model_name = if width < 72 {
        sv::fit_text(&snapshot.model, width.saturating_sub(28).max(1))
    } else {
        snapshot.model.clone()
    };
    let model = labeled(if width < 50 { "M:" } else { "Model:" }, &model_name);
    let effort = labeled(
        if width < 50 { "Eff:" } else { "Effort:" },
        snapshot.reasoning.as_deref().unwrap_or("—"),
    );
    let approval = labeled("Approval:", &snapshot.approval);
    let access = labeled("Access:", &snapshot.sandbox);
    let tokens = labeled(
        if width < 72 {
            "Tokens:"
        } else {
            "Tokens Sparked:"
        },
        &snapshot
            .tokens_sparked
            .map(format_tokens)
            .unwrap_or_else(|| "—".to_string()),
    );
    let context = labeled(
        if width < 72 { "Ctx:" } else { "Context:" },
        &context_display(snapshot.context.as_ref(), width),
    );

    let plan = snapshot
        .plan_mode
        .then(|| Line::from(sv::active("PLAN MODE (SHIFT+TAB TO CYCLE)")));
    let mut segments = vec![model, effort, approval, access, tokens];
    if let Some(plan) = plan {
        segments.push(plan);
    }
    segments.push(context);
    let mut lines = Vec::new();
    let mut current = Vec::new();
    for segment in segments {
        let mut candidate = current.clone();
        candidate.push(segment.clone());
        if !current.is_empty() && line_segments_width(&candidate) > width {
            lines.push(compact_status_line(current, width));
            current = vec![segment];
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(compact_status_line(current, width));
    }
    lines
        .into_iter()
        .map(|line| truncate_line_with_ellipsis_if_overflow(line, width))
        .collect()
}

fn context_display(context: Option<&SyndridContextUsage>, width: usize) -> String {
    let Some(context) = context else {
        return "—".to_string();
    };
    let used = context.used_tokens.max(0);
    let window = context.context_window.max(0);
    if width >= 100 {
        format!("{} / {}", format_tokens(used), format_tokens(window))
    } else if window > 0 {
        format!(
            "{}%",
            ((used as f64 / window as f64) * 100.0).round() as i64
        )
    } else {
        "—".to_string()
    }
}

fn compact_status_line(segments: Vec<Line<'static>>, width: usize) -> Line<'static> {
    let mut line = Line::default();
    for segment in segments {
        let separator_width = usize::from(!line.spans.is_empty()) * 2;
        if line.width() + separator_width + segment.width() > width {
            continue;
        }
        if !line.spans.is_empty() {
            line.spans.push(sv::muted("  "));
        }
        line.spans.extend(segment.spans);
    }
    if line.spans.is_empty() {
        Line::from(Span::styled("—", Style::default().fg(sv::MUTED_TEXT)))
    } else {
        line
    }
}

fn line_segments_width(segments: &[Line<'static>]) -> usize {
    segments.iter().map(Line::width).sum::<usize>() + segments.len().saturating_sub(1) * 2
}

fn labeled(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        sv::secondary(format!("{label} ")),
        sv::active(value.to_string()),
    ])
}

fn format_tokens(tokens: i64) -> String {
    crate::status::format_tokens_compact(tokens.max(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> SyndridStatusSnapshot {
        SyndridStatusSnapshot {
            identity: "SyndridCLI".to_string(),
            session_id: None,
            workspace: None,
            branch: None,
            state: None,
            current_task: None,
            model: "gpt-5.1-codex".to_string(),
            reasoning: Some("high".to_string()),
            profile: Some("strict".to_string()),
            sandbox: "Workspace".to_string(),
            approval: "Ask for approval".to_string(),
            context: Some(SyndridContextUsage {
                used_tokens: 72_000,
                context_window: 100_000,
            }),
            tokens_sparked: Some(12_000),
            running_subagents: 2,
            plan_mode: false,
            token_usage: None,
        }
    }

    fn text(line: Option<Line<'static>>) -> Option<String> {
        line.map(|line| {
            line.spans
                .into_iter()
                .map(|span| span.content.into_owned())
                .collect()
        })
    }

    #[test]
    fn expanded_line_matches_approved_footer_fields() {
        let rendered =
            text(status_line(&snapshot(), 200, true, Some("explorer"), false)).expect("line");

        assert!(rendered.contains("Model: gpt-5.1-codex"));
        assert!(rendered.contains("Effort: high"));
        assert!(rendered.contains("Approval: Ask for approval"));
        assert!(rendered.contains("Access: Workspace"));
        assert!(rendered.contains("Tokens Sparked: 12K"));
        assert!(rendered.contains("Context: 72K / 100K"));
    }

    #[test]
    fn missing_context_uses_honest_placeholder() {
        let mut snapshot = snapshot();
        snapshot.context = None;

        let rendered = text(status_line(&snapshot, 120, false, None, false)).expect("line");
        assert!(rendered.contains("—"));
    }

    #[test]
    fn narrow_line_prioritizes_complete_segments() {
        let lines = status_lines(&snapshot(), 48, true, None, true);
        assert!(lines.iter().all(|line| line.width() <= 48));
        assert!(
            lines
                .iter()
                .filter_map(|line| text(Some(line.clone())))
                .any(|line| line.contains("Ctx:"))
        );
    }

    #[test]
    fn medium_and_narrow_layouts_keep_context_visible() {
        let medium = status_lines(&snapshot(), 90, false, None, false);
        assert!(
            medium
                .iter()
                .filter_map(|line| text(Some(line.clone())))
                .any(|line| line.contains("72%"))
        );

        let narrow = status_lines(&snapshot(), 48, false, None, false);
        assert_eq!(narrow.len(), 3);
        assert!(
            narrow
                .iter()
                .filter_map(|line| text(Some(line.clone())))
                .any(|line| line.contains("Ctx:"))
        );
        assert!(
            narrow
                .iter()
                .filter_map(|line| text(Some(line.clone())))
                .any(|line| line.contains("Tokens:"))
        );
    }

    #[test]
    fn context_denominator_tracks_the_active_model_window() {
        let mut snapshot = snapshot();
        snapshot.context = Some(SyndridContextUsage {
            used_tokens: 13_500,
            context_window: 258_000,
        });
        let before = text(status_line(&snapshot, 200, false, None, false)).unwrap();
        assert!(before.contains("Context: 13.5K / 258K"));

        snapshot.context.as_mut().unwrap().context_window = 128_000;
        let after = text(status_line(&snapshot, 200, false, None, false)).unwrap();
        assert!(after.contains("Context: 13.5K / 128K"));
    }

    #[test]
    fn tiny_widths_never_panic_or_wrap() {
        assert_eq!(text(status_line(&snapshot(), 0, true, None, true)), None);
        for width in 1..=8 {
            let line = status_line(&snapshot(), width, true, None, true).expect("line");
            assert!(line.width() <= width);
            let rendered = text(Some(line)).expect("text");
            assert!(!rendered.contains('\n'));
        }
    }
}
