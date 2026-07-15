use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;

const SEPARATOR: &str = " · ";
const SEPARATOR_WIDTH: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyndridStatusSnapshot {
    pub(crate) identity: String,
    pub(crate) model: String,
    pub(crate) reasoning: Option<String>,
    pub(crate) profile: Option<String>,
    pub(crate) sandbox: String,
    pub(crate) approval: String,
    pub(crate) context: Option<String>,
    pub(crate) running_subagents: usize,
}

pub(crate) fn status_line(
    snapshot: &SyndridStatusSnapshot,
    width: usize,
    is_task_running: bool,
    active_agent_label: Option<&str>,
    waiting: bool,
) -> Option<Line<'static>> {
    if width == 0 {
        return None;
    }

    let active_context = active_agent_label
        .filter(|label| !label.trim().is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| snapshot.profile.clone());
    let task_count = usize::from(is_task_running);

    let expanded = expanded_line(snapshot, task_count, active_context.as_deref(), waiting);
    if expanded.width() <= width {
        return Some(expanded);
    }

    let compact_segments =
        compact_segments(snapshot, task_count, active_context.as_deref(), waiting);
    let compact = pack_segments(compact_segments, width);
    Some(truncate_line_with_ellipsis_if_overflow(compact, width))
}

fn expanded_line(
    snapshot: &SyndridStatusSnapshot,
    task_count: usize,
    active_context: Option<&str>,
    waiting: bool,
) -> Line<'static> {
    let mut segments = vec![
        identity_segment(&snapshot.identity),
        labeled_segment("Model", &snapshot.model),
    ];
    if let Some(reasoning) = snapshot.reasoning.as_deref() {
        segments.push(labeled_segment("Reasoning", reasoning));
    }
    if let Some(active_context) = active_context {
        segments.push(labeled_segment("Active", active_context));
    }
    segments.push(labeled_segment("Sandbox", &snapshot.sandbox));
    segments.push(labeled_segment("Approval", &snapshot.approval));
    if let Some(context) = snapshot.context.as_deref() {
        segments.push(labeled_segment("Context", context));
    }
    segments.push(labeled_segment("Tasks", &task_count.to_string()));
    segments.push(labeled_segment(
        "Subagents",
        &snapshot.running_subagents.to_string(),
    ));
    if waiting {
        segments.push(Line::from("Waiting").yellow());
    }
    join_segments(segments)
}

fn compact_segments(
    snapshot: &SyndridStatusSnapshot,
    task_count: usize,
    active_context: Option<&str>,
    waiting: bool,
) -> Vec<Line<'static>> {
    let mut segments = vec![
        identity_segment("Syndrid"),
        Line::from(snapshot.model.clone()),
    ];
    if waiting {
        segments.push(Line::from("wait").yellow());
    }
    if task_count > 0 {
        segments.push(Line::from(format!("t{task_count}")));
    }
    if snapshot.running_subagents > 0 {
        segments.push(Line::from(format!("a{}", snapshot.running_subagents)));
    }
    if let Some(context) = snapshot.context.as_deref() {
        segments.push(Line::from(format!("ctx {context}")));
    }
    if let Some(reasoning) = snapshot.reasoning.as_deref() {
        segments.push(Line::from(format!("r {reasoning}")));
    }
    segments.push(Line::from(format!("sbx {}", snapshot.sandbox)));
    segments.push(Line::from(format!("ask {}", snapshot.approval)));
    if let Some(active_context) = active_context {
        segments.push(Line::from(format!("at {active_context}")));
    }
    segments
}

fn pack_segments(segments: Vec<Line<'static>>, width: usize) -> Line<'static> {
    let mut packed = Line::default();
    for segment in segments {
        let separator_width = if packed.spans.is_empty() {
            0
        } else {
            SEPARATOR_WIDTH
        };
        if packed.width() + separator_width + segment.width() > width {
            continue;
        }
        if !packed.spans.is_empty() {
            packed.spans.push(SEPARATOR.dim());
        }
        packed.spans.extend(segment.spans);
    }

    if packed.spans.is_empty() {
        Line::from("Syndrid").bold().cyan()
    } else {
        packed
    }
}

fn join_segments(segments: Vec<Line<'static>>) -> Line<'static> {
    let mut line = Line::default();
    for segment in segments {
        if !line.spans.is_empty() {
            line.spans.push(SEPARATOR.dim());
        }
        line.spans.extend(segment.spans);
    }
    line
}

fn identity_segment(identity: &str) -> Line<'static> {
    Line::from(identity.to_string()).bold().cyan()
}

fn labeled_segment(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::from(format!("{label} ")).dim(),
        Span::from(value.to_string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> SyndridStatusSnapshot {
        SyndridStatusSnapshot {
            identity: "SyndridCLI".to_string(),
            model: "gpt-5.1-codex".to_string(),
            reasoning: Some("high".to_string()),
            profile: Some("strict".to_string()),
            sandbox: "Workspace".to_string(),
            approval: "Ask for approval".to_string(),
            context: Some("72% left".to_string()),
            running_subagents: 2,
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
    fn expanded_line_shows_available_fields() {
        let rendered =
            text(status_line(&snapshot(), 240, true, Some("explorer"), false)).expect("line");

        assert!(rendered.contains("SyndridCLI"));
        assert!(rendered.contains("Model gpt-5.1-codex"));
        assert!(rendered.contains("Reasoning high"));
        assert!(rendered.contains("Active explorer"));
        assert!(rendered.contains("Sandbox Workspace"));
        assert!(rendered.contains("Approval Ask for approval"));
        assert!(rendered.contains("Context 72% left"));
        assert!(rendered.contains("Tasks 1"));
        assert!(rendered.contains("Subagents 2"));
    }

    #[test]
    fn compact_line_prioritizes_live_activity() {
        let line = status_line(&snapshot(), 48, true, None, true).expect("line");
        assert!(line.width() <= 48);
        let rendered = text(Some(line)).expect("text");

        assert!(rendered.starts_with("Syndrid"));
        assert!(rendered.contains("wait"));
        assert!(rendered.contains("t1"));
        assert!(rendered.contains("a2"));
    }

    #[test]
    fn unavailable_optional_fields_are_omitted() {
        let mut snapshot = snapshot();
        snapshot.reasoning = None;
        snapshot.profile = None;
        snapshot.context = None;
        snapshot.running_subagents = 0;

        let rendered = text(status_line(&snapshot, 240, false, None, false)).expect("line");

        assert!(!rendered.contains("Reasoning"));
        assert!(!rendered.contains("Active"));
        assert!(!rendered.contains("Context"));
        assert!(rendered.contains("Tasks 0"));
        assert!(rendered.contains("Subagents 0"));
    }

    #[test]
    fn tiny_widths_never_panic_or_wrap() {
        assert_eq!(text(status_line(&snapshot(), 0, true, None, true)), None);
        for width in 1..=8 {
            let rendered = text(status_line(&snapshot(), width, true, None, true)).expect("line");
            assert!(rendered.chars().count() <= width);
            assert!(!rendered.contains('\n'));
        }
    }
}
