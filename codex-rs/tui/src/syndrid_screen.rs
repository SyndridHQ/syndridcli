//! Focused, full-screen Syndrid session surfaces.
//!
//! These views are presentation-only. They consume cached values supplied by the
//! TUI owner and deliberately render an em dash for data that is not available.

use crate::bottom_pane::BottomPaneView;
use crate::bottom_pane::SyndridStatusSnapshot;
use crate::bottom_pane::ViewCompletion;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::render::renderable::Renderable;
use crate::slash_command::SlashCommand;
use crate::syndrid_live_state::ActivityStatus;
use crate::syndrid_live_state::DataQuality;
use crate::syndrid_live_state::LifecycleState;
use crate::syndrid_live_state::LiveSessionState;
use crate::syndrid_live_state::LiveView;
use crate::syndrid_live_state::VerificationStatus;
use crate::syndrid_visuals;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthChar;

#[cfg(test)]
#[path = "syndrid_screen_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyndridScreenKind {
    Status,
    Usage,
    Live(LiveView),
    Commands { all: bool },
}

pub(crate) struct SyndridScreen {
    kind: SyndridScreenKind,
    status_snapshot: Option<SyndridStatusSnapshot>,
    live: LiveSessionState,
    complete: Option<ViewCompletion>,
    selected: usize,
    scroll_offset: usize,
    commands: Vec<SlashCommand>,
    filter: String,
}

impl SyndridScreen {
    #[cfg(test)]
    pub(crate) fn status() -> Self {
        Self::new(SyndridScreenKind::Status)
    }

    pub(crate) fn status_with_snapshot(snapshot: Option<SyndridStatusSnapshot>) -> Self {
        let mut screen = Self::new(SyndridScreenKind::Status);
        screen.status_snapshot = snapshot;
        screen
    }

    #[cfg(test)]
    pub(crate) fn usage() -> Self {
        Self::new(SyndridScreenKind::Usage)
    }

    pub(crate) fn usage_with_snapshot(snapshot: Option<SyndridStatusSnapshot>) -> Self {
        let mut screen = Self::new(SyndridScreenKind::Usage);
        screen.status_snapshot = snapshot;
        screen
    }

    pub(crate) fn live(state: LiveSessionState) -> Self {
        Self {
            kind: SyndridScreenKind::Live(state.view),
            status_snapshot: None,
            live: state,
            complete: None,
            selected: 0,
            scroll_offset: 0,
            commands: Vec::new(),
            filter: String::new(),
        }
    }

    fn new(kind: SyndridScreenKind) -> Self {
        Self {
            kind,
            status_snapshot: None,
            live: LiveSessionState::default(),
            complete: None,
            selected: 0,
            scroll_offset: 0,
            commands: Vec::new(),
            filter: String::new(),
        }
    }

    pub(crate) fn command_browser(all: bool) -> Self {
        let curated = vec![
            SlashCommand::Model,
            SlashCommand::Effort,
            SlashCommand::Plan,
            SlashCommand::Permissions,
            SlashCommand::Status,
            SlashCommand::Usage,
            SlashCommand::Session,
            SlashCommand::Activity,
            SlashCommand::Changes,
            SlashCommand::Verification,
            SlashCommand::Goal,
            SlashCommand::Review,
            SlashCommand::Diff,
            SlashCommand::Resume,
            SlashCommand::New,
            SlashCommand::Compact,
            SlashCommand::Mcp,
        ];
        let commands = if all {
            crate::slash_command::built_in_slash_commands()
                .into_iter()
                .map(|(_, command)| command)
                .filter(|command| !matches!(command, SlashCommand::Quit | SlashCommand::Btw))
                .collect()
        } else {
            curated
        };
        Self {
            kind: SyndridScreenKind::Commands { all },
            status_snapshot: None,
            live: LiveSessionState::default(),
            complete: None,
            selected: 0,
            scroll_offset: 0,
            commands,
            filter: String::new(),
        }
    }

    fn title(&self) -> &'static str {
        match self.kind {
            SyndridScreenKind::Status => "STATUS · CURRENT SESSION",
            SyndridScreenKind::Usage => "USAGE · ACCOUNT ACTIVITY",
            SyndridScreenKind::Live(view) => view.label(),
            SyndridScreenKind::Commands { all: true } => "ALL COMMANDS",
            SyndridScreenKind::Commands { all: false } => "SYNDRID COMMANDS",
        }
    }

    fn value(value: Option<impl ToString>) -> String {
        value.map_or_else(|| "—".to_string(), |value| value.to_string())
    }

    fn metric(label: &'static str, value: String) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                format!("{label:<18}"),
                Style::default().fg(syndrid_visuals::SECONDARY_TEXT),
            ),
            Span::styled(value, Style::default().fg(syndrid_visuals::PRIMARY_TEXT)),
        ])
    }

    fn lines(&self, width: u16) -> Vec<Line<'static>> {
        match self.kind {
            SyndridScreenKind::Status => self.status_dashboard_lines(width),
            SyndridScreenKind::Usage => self.usage_dashboard_lines(width),
            SyndridScreenKind::Live(LiveView::Dashboard) => self.dashboard_lines(width),
            SyndridScreenKind::Live(LiveView::Activity) => self.activity_lines(width),
            SyndridScreenKind::Live(LiveView::Changes) => self.changes_lines(width),
            SyndridScreenKind::Live(LiveView::Verification) => self.verification_lines(width),
            SyndridScreenKind::Commands { all } => self.command_lines(width, all),
        }
    }

    fn filtered_commands(&self) -> Vec<SlashCommand> {
        let filter = self.filter.to_ascii_lowercase();
        self.commands
            .iter()
            .copied()
            .filter(|command| filter.is_empty() || command.command().contains(&filter))
            .collect()
    }

    fn command_lines(&self, width: u16, all: bool) -> Vec<Line<'static>> {
        let commands = self.filtered_commands();
        if commands.is_empty() {
            return vec![Line::from("No matching commands".dim())];
        }
        if all {
            return self.all_command_grid(width, commands);
        }

        let compact = width < 50;
        let name_width = 12;
        let descriptions = commands
            .iter()
            .map(|command| default_command_description(*command).to_string())
            .collect::<Vec<_>>();
        let description_width = if compact {
            usize::from(width.saturating_sub(8)).max(1)
        } else {
            descriptions
                .iter()
                .map(|description| unicode_width::UnicodeWidthStr::width(description.as_str()))
                .max()
                .unwrap_or(1)
                .min(usize::from(width.saturating_sub(name_width as u16 + 5)))
        };
        let top_spacing = if compact {
            0
        } else if width >= 80 {
            3
        } else {
            1
        };
        let mut lines = vec![Line::from(""); top_spacing];
        for (index, (command, description)) in commands.into_iter().zip(descriptions).enumerate() {
            let selected = index == self.selected;
            let marker = if selected { "#" } else { " " };
            let command_style = if selected {
                Style::default().fg(syndrid_visuals::BRIGHT_GOLD).bold()
            } else {
                Style::default().fg(syndrid_visuals::PRIMARY_TEXT)
            };
            if compact {
                lines.push(center_line(
                    Line::from(Span::styled(
                        format!("{marker} {}", command.command().to_uppercase()),
                        command_style,
                    )),
                    usize::from(width),
                ));
                for wrapped in textwrap::wrap(&description, description_width) {
                    lines.push(center_line(
                        Line::from(Span::styled(
                            format!("    {wrapped}"),
                            Style::default().fg(syndrid_visuals::SECONDARY_TEXT),
                        )),
                        usize::from(width),
                    ));
                }
            } else {
                let name = command.command().to_uppercase();
                let row = Line::from(vec![
                    Span::styled(format!("{marker} {name:<name_width$} │ "), command_style),
                    Span::styled(
                        crate::syndrid_visuals::padded(&description, description_width),
                        Style::default().fg(syndrid_visuals::SECONDARY_TEXT),
                    ),
                ]);
                lines.push(center_line(row, usize::from(width)));
            }
            if matches!(
                command,
                SlashCommand::Permissions | SlashCommand::Diff | SlashCommand::Compact
            ) {
                lines.push(Line::from(""));
            }
        }
        lines
    }

    fn all_command_grid(&self, width: u16, commands: Vec<SlashCommand>) -> Vec<Line<'static>> {
        let category_names = all_category_names();
        if !self.filter.is_empty() {
            return self.compact_all_command_results(width, &commands, &category_names);
        }
        let columns = if width >= 120 {
            3
        } else if width >= 80 {
            2
        } else {
            1
        };
        let groups = all_command_groups(&commands);
        let cell_gap = if columns == 3 { 4 } else { 3 };
        let preferred_grid_width = match columns {
            3 => 84,
            2 => 54,
            _ => usize::from(width),
        };
        let grid_width = preferred_grid_width.min(usize::from(width));
        let cell_width = (grid_width.saturating_sub(cell_gap * (columns - 1)) / columns).max(1);
        let left_padding = " ".repeat(usize::from(width).saturating_sub(grid_width) / 2);
        let mut lines = vec![Line::from(""), Line::from("")];
        for (chunk_index, chunk) in groups.chunks(columns).enumerate() {
            let row_count = chunk.iter().map(Vec::len).max().unwrap_or(0);
            let mut heading_spans = vec![Span::raw(left_padding.clone())];
            for (column, _) in chunk.iter().enumerate() {
                if column > 0 {
                    heading_spans.push(Span::raw(" ".repeat(cell_gap)));
                }
                heading_spans.push(Span::styled(
                    syndrid_visuals::centered(
                        category_names[chunk_index * columns + column],
                        cell_width,
                    ),
                    Style::default().fg(syndrid_visuals::SECONDARY_TEXT).bold(),
                ));
            }
            lines.push(Line::from(heading_spans));
            for row in 0..row_count {
                let mut spans = vec![Span::raw(left_padding.clone())];
                for column in 0..chunk.len() {
                    if column > 0 {
                        spans.push(Span::raw(" ".repeat(cell_gap)));
                    }
                    let command = chunk[column].get(row).copied();
                    let selected = command.is_some_and(|command| {
                        self.filtered_commands()
                            .get(self.selected)
                            .is_some_and(|selected| *selected == command)
                    });
                    let text = command.map_or_else(
                        || " ".repeat(cell_width),
                        |command| {
                            let marker = if selected { "#" } else { " " };
                            syndrid_visuals::padded(
                                &format!("{marker} {}", display_command_name(command)),
                                cell_width,
                            )
                        },
                    );
                    let style = if selected {
                        Style::default().fg(syndrid_visuals::BRIGHT_GOLD).bold()
                    } else {
                        Style::default().fg(syndrid_visuals::PRIMARY_TEXT)
                    };
                    spans.push(Span::styled(text, style));
                }
                lines.push(Line::from(spans));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(""));
        }
        lines
    }

    fn compact_all_command_results(
        &self,
        width: u16,
        commands: &[SlashCommand],
        category_names: &[&str; 6],
    ) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from("")];
        for (index, command) in commands.iter().enumerate() {
            let selected = index == self.selected;
            let marker = if selected { "#" } else { " " };
            let text = format!(
                "{marker} {}  ·  {}",
                display_command_name(*command),
                category_names[command_category(*command)]
            );
            let style = if selected {
                Style::default().fg(syndrid_visuals::BRIGHT_GOLD).bold()
            } else {
                Style::default().fg(syndrid_visuals::PRIMARY_TEXT)
            };
            lines.push(center_line(
                Line::from(Span::styled(
                    syndrid_visuals::fit_text(&text, usize::from(width)),
                    style,
                )),
                usize::from(width),
            ));
        }
        lines
    }

    fn context(&self) -> String {
        match (self.live.context_used, self.live.context_window) {
            (Some(used), Some(window)) => format!("{used} / {window}"),
            _ => "—".to_string(),
        }
    }

    fn status_panel(
        title: &'static str,
        rows: Vec<(&'static str, String)>,
        width: usize,
    ) -> Vec<Line<'static>> {
        let label_width = 13;
        let value_width = width.saturating_sub(label_width + 1).max(1);
        let mut lines = vec![Self::pad_status_line(
            center_line(Line::from(title.bold().fg(syndrid_visuals::GOLD)), width),
            width,
        )];
        for (label, value) in rows {
            lines.push(Self::pad_status_line(
                Line::from(vec![
                    Span::styled(
                        syndrid_visuals::padded(label, label_width),
                        Style::default().fg(syndrid_visuals::SECONDARY_TEXT),
                    ),
                    syndrid_visuals::border("│ "),
                    Span::styled(
                        Self::fit_status_value(label, &value, value_width),
                        Style::default().fg(syndrid_visuals::PRIMARY_TEXT),
                    ),
                ]),
                width,
            ));
        }
        lines
    }

    fn fit_status_value(label: &str, value: &str, width: usize) -> String {
        if label == "ID" {
            middle_truncate(value, width)
        } else {
            syndrid_visuals::fit_text(value, width)
        }
    }

    fn pad_status_line(mut line: Line<'static>, width: usize) -> Line<'static> {
        let used = line.width();
        if used < width {
            line.spans.push(Span::raw(" ".repeat(width - used)));
        }
        line
    }

    fn combine_status_panels(
        left: Vec<Line<'static>>,
        right: Vec<Line<'static>>,
        panel_width: usize,
        gap: usize,
        padding: usize,
    ) -> Vec<Line<'static>> {
        let height = left.len().max(right.len());
        let mut lines = Vec::with_capacity(height + 1);
        for row in 0..height {
            let mut spans = vec![Span::raw(" ".repeat(padding))];
            spans.extend(
                left.get(row)
                    .cloned()
                    .unwrap_or_else(|| Line::from(" ".repeat(panel_width)))
                    .spans,
            );
            spans.push(Span::raw(" ".repeat(gap)));
            spans.extend(
                right
                    .get(row)
                    .cloned()
                    .unwrap_or_else(|| Line::from(" ".repeat(panel_width)))
                    .spans,
            );
            lines.push(Line::from(spans));
        }
        lines
    }

    fn status_dashboard_lines(&self, width: u16) -> Vec<Line<'static>> {
        let snapshot = self.status_snapshot.as_ref();
        let value = |value: Option<&String>| value.cloned().unwrap_or_else(|| "—".to_string());
        let context = snapshot
            .and_then(|snapshot| snapshot.context.as_ref())
            .map(|context| format!("{} / {}", context.used_tokens, context.context_window))
            .unwrap_or_else(|| "—".to_string());
        let account_total = snapshot
            .and_then(|snapshot| snapshot.tokens_sparked)
            .map(|tokens| tokens.to_string())
            .unwrap_or_else(|| "—".to_string());
        let session_rows = vec![
            (
                "ID",
                value(snapshot.and_then(|snapshot| snapshot.session_id.as_ref())),
            ),
            (
                "Workspace",
                value(snapshot.and_then(|snapshot| snapshot.workspace.as_ref())),
            ),
            (
                "Branch",
                value(snapshot.and_then(|snapshot| snapshot.branch.as_ref())),
            ),
            ("Elapsed", "—".to_string()),
            (
                "State",
                value(snapshot.and_then(|snapshot| snapshot.state.as_ref())),
            ),
            (
                "Current task",
                value(snapshot.and_then(|snapshot| snapshot.current_task.as_ref())),
            ),
        ];
        let execution_rows = [
            ("Tools", "—"),
            ("Commands", "—"),
            ("Files changed", "—"),
            ("Lines", "—"),
            ("Tests", "—"),
            ("Build", "—"),
            ("Approvals", "—"),
        ];
        let model_rows = [
            (
                "Model",
                snapshot
                    .map(|snapshot| snapshot.model.as_str())
                    .unwrap_or("—"),
            ),
            (
                "Effort",
                snapshot
                    .and_then(|snapshot| snapshot.reasoning.as_deref())
                    .unwrap_or("—"),
            ),
            (
                "Mode",
                if snapshot.is_some_and(|snapshot| snapshot.plan_mode) {
                    "Plan"
                } else if snapshot.is_some() {
                    "Default"
                } else {
                    "—"
                },
            ),
            ("Context", context.as_str()),
            ("Compactions", "—"),
        ];
        let usage = snapshot.and_then(|snapshot| snapshot.token_usage.as_ref());
        let token_rows = vec![
            (
                "Session total",
                usage
                    .map(|usage| usage.total_tokens.to_string())
                    .unwrap_or_else(|| "—".to_string()),
            ),
            (
                "Input",
                usage
                    .map(|usage| usage.input_tokens.to_string())
                    .unwrap_or_else(|| "—".to_string()),
            ),
            (
                "Cached input",
                usage
                    .map(|usage| usage.cached_input_tokens.to_string())
                    .unwrap_or_else(|| "—".to_string()),
            ),
            (
                "Output",
                usage
                    .map(|usage| usage.output_tokens.to_string())
                    .unwrap_or_else(|| "—".to_string()),
            ),
            ("Account total", account_total),
        ];
        let policy_rows = [
            (
                "Approval",
                snapshot
                    .map(|snapshot| snapshot.approval.as_str())
                    .unwrap_or("—"),
            ),
            (
                "Access",
                snapshot
                    .map(|snapshot| snapshot.sandbox.as_str())
                    .unwrap_or("—"),
            ),
            ("Network", "—"),
            ("Sandbox roots", "—"),
        ];
        let health_rows = [
            ("Git", "—"),
            ("MCP", "—"),
            ("Capture", "—"),
            ("Last error", "—"),
        ];
        let to_rows = |rows: &[(&'static str, &str)]| {
            rows.iter()
                .map(|(label, value)| (*label, (*value).to_string()))
                .collect::<Vec<_>>()
        };
        if width >= 80 {
            let matrix_width = usize::from(width).min(110);
            let gap = 6;
            let panel_width = matrix_width.saturating_sub(gap) / 2;
            let padding = usize::from(width).saturating_sub(panel_width * 2 + gap) / 2;
            let section_gap = if width >= 100 { 2 } else { 1 };
            let panels = [
                (
                    Self::status_panel("SESSION", session_rows.clone(), panel_width),
                    Self::status_panel("EXECUTION", to_rows(&execution_rows), panel_width),
                ),
                (
                    Self::status_panel(
                        "MODEL",
                        model_rows
                            .iter()
                            .map(|(l, v)| (*l, (*v).to_string()))
                            .collect(),
                        panel_width,
                    ),
                    Self::status_panel("TOKENS", token_rows.clone(), panel_width),
                ),
                (
                    Self::status_panel("POLICY", to_rows(&policy_rows), panel_width),
                    Self::status_panel("HEALTH", to_rows(&health_rows), panel_width),
                ),
            ];
            let mut lines = vec![Line::default()];
            for (index, (left, right)) in panels.into_iter().enumerate() {
                if index > 0 {
                    lines.extend(std::iter::repeat_n(Line::default(), section_gap));
                }
                lines.extend(Self::combine_status_panels(
                    left,
                    right,
                    panel_width,
                    gap,
                    padding,
                ));
            }
            lines
        } else {
            let panel_width = usize::from(width);
            let mut lines = vec![Line::default()];
            lines.extend(
                [
                    Self::status_panel("SESSION", session_rows, panel_width),
                    Self::status_panel("EXECUTION", to_rows(&execution_rows), panel_width),
                    Self::status_panel(
                        "MODEL",
                        model_rows
                            .iter()
                            .map(|(l, v)| (*l, (*v).to_string()))
                            .collect(),
                        panel_width,
                    ),
                    Self::status_panel("TOKENS", token_rows, panel_width),
                    Self::status_panel("POLICY", to_rows(&policy_rows), panel_width),
                    Self::status_panel("HEALTH", to_rows(&health_rows), panel_width),
                ]
                .into_iter()
                .flat_map(|mut panel| {
                    panel.push(Line::default());
                    panel
                }),
            );
            lines
        }
    }

    fn usage_dashboard_lines(&self, width: u16) -> Vec<Line<'static>> {
        let snapshot = self.status_snapshot.as_ref();
        let usage = snapshot.and_then(|snapshot| snapshot.token_usage.as_ref());
        let exact = |value: Option<String>| quality_value(value, DataQuality::Exact);
        let unavailable = || quality_value(None, DataQuality::Unavailable);
        let token = |value: Option<i64>| exact(value.map(format_count));
        let context_used = snapshot
            .and_then(|snapshot| snapshot.context.as_ref())
            .map(|context| context.used_tokens);
        let context_max = snapshot
            .and_then(|snapshot| snapshot.context.as_ref())
            .map(|context| context.context_window);
        let context_percent = context_used
            .zip(context_max)
            .filter(|(_, maximum)| *maximum > 0)
            .map(|(used, maximum)| format!("{}%", used.saturating_mul(100) / maximum));
        let session_rows = vec![
            (
                "Session total",
                token(usage.map(|usage| usage.total_tokens)),
            ),
            ("Input tokens", token(usage.map(|usage| usage.input_tokens))),
            (
                "Cached input",
                token(usage.map(|usage| usage.cached_input_tokens)),
            ),
            (
                "Output tokens",
                token(usage.map(|usage| usage.output_tokens)),
            ),
            (
                "Reasoning tokens",
                token(
                    usage
                        .map(|usage| usage.reasoning_output_tokens)
                        .filter(|value| *value > 0),
                ),
            ),
            ("Context used", token(context_used)),
            ("Context maximum", token(context_max)),
            ("Context percentage", exact(context_percent.clone())),
            ("Compactions", unavailable()),
            ("Session elapsed", unavailable()),
        ];
        let rate_rows = vec![
            ("Output / second", unavailable()),
            ("First-token latency", unavailable()),
            ("Turn latency", unavailable()),
            ("Latest throughput", unavailable()),
            ("Average throughput", unavailable()),
        ];
        let account_rows = vec![
            ("Provider", unavailable()),
            ("Account", unavailable()),
            ("Plan", unavailable()),
            ("Used quota", unavailable()),
            ("Remaining quota", unavailable()),
            ("Reset time", unavailable()),
            ("Rate-limit window", unavailable()),
        ];
        let forecast_rows = vec![
            ("Final session tokens", unavailable()),
            ("Context usage", unavailable()),
            ("Quota impact", unavailable()),
            ("ETA", unavailable()),
            ("Confidence", unavailable()),
        ];
        let quality_rows = vec![
            (
                "Session accounting",
                quality_value(usage.map(|_| "Exact".to_string()), DataQuality::Exact),
            ),
            (
                "Context calculation",
                quality_value(context_percent, DataQuality::Derived),
            ),
            ("Account / quota", unavailable()),
            ("Forecast", unavailable()),
            (
                "Cached input rule",
                Line::from(
                    "included once in provider total"
                        .to_string()
                        .fg(syndrid_visuals::PRIMARY_TEXT),
                ),
            ),
        ];
        let panels = [
            ("ACCOUNT", account_rows),
            ("SESSION TOKENS", session_rows),
            ("RATE / PERFORMANCE", rate_rows),
            ("FORECAST", forecast_rows),
            ("QUALITY / ACCOUNTING", quality_rows),
        ];
        if width >= 80 {
            let matrix_width = usize::from(width).min(110);
            let gap = 6;
            let panel_width = matrix_width.saturating_sub(gap) / 2;
            let padding = usize::from(width).saturating_sub(panel_width * 2 + gap) / 2;
            let mut lines = vec![Line::default()];
            for (index, chunk) in panels.chunks(2).enumerate() {
                if index > 0 {
                    lines.extend(std::iter::repeat_n(Line::default(), 2));
                }
                let left = Self::usage_panel(chunk[0].0, &chunk[0].1, panel_width);
                let right = chunk
                    .get(1)
                    .map(|panel| Self::usage_panel(panel.0, &panel.1, panel_width))
                    .unwrap_or_default();
                lines.extend(Self::combine_status_panels(
                    left,
                    right,
                    panel_width,
                    gap,
                    padding,
                ));
            }
            lines
        } else {
            let mut lines = vec![Line::default()];
            for (title, rows) in panels {
                lines.extend(Self::usage_panel(title, &rows, usize::from(width)));
                lines.push(Line::default());
            }
            lines
        }
    }

    fn usage_panel(
        title: &'static str,
        rows: &[(&'static str, Line<'static>)],
        width: usize,
    ) -> Vec<Line<'static>> {
        let label_width = 20;
        let value_width = width.saturating_sub(label_width + 1).max(1);
        let mut lines = vec![Self::pad_status_line(
            center_line(Line::from(title.bold().fg(syndrid_visuals::GOLD)), width),
            width,
        )];
        for (label, value) in rows {
            let value = value.clone();
            let fitted = truncate_line_with_ellipsis_if_overflow(value, value_width);
            let mut spans = vec![
                Span::styled(
                    syndrid_visuals::padded(label, label_width),
                    Style::default().fg(syndrid_visuals::SECONDARY_TEXT),
                ),
                syndrid_visuals::border("│ "),
            ];
            spans.extend(fitted.spans);
            lines.push(Self::pad_status_line(Line::from(spans), width));
        }
        lines
    }

    fn dashboard_lines(&self, width: u16) -> Vec<Line<'static>> {
        let verification_count = |status| {
            self.live
                .verifications
                .iter()
                .filter(|item| item.status == status)
                .count()
        };
        let verification_summary = if self.live.verifications.is_empty() {
            None
        } else {
            Some(format!(
                "{} passed / {} failed / {} running / {} not run",
                verification_count(VerificationStatus::Passed),
                verification_count(VerificationStatus::Failed),
                verification_count(VerificationStatus::Running),
                verification_count(VerificationStatus::NotRun)
            ))
        };
        let latest_failure = self
            .live
            .verifications
            .iter()
            .rev()
            .find(|item| item.status == VerificationStatus::Failed)
            .map(|item| item.name.clone());
        let session = vec![
            (
                "Session ID",
                live_value(self.live.session_id.clone(), DataQuality::Exact),
            ),
            (
                "Current task",
                live_value(self.live.task.clone(), DataQuality::Exact),
            ),
            ("Lifecycle", lifecycle_value(self.live.lifecycle)),
            (
                "Workflow stage",
                live_value(self.live.workflow_stage.clone(), DataQuality::Exact),
            ),
            (
                "Wait reason",
                live_value(self.live.wait_reason.clone(), DataQuality::Exact),
            ),
            ("Elapsed", live_value(None, DataQuality::Unavailable)),
            (
                "Workspace",
                live_value(self.live.workspace.clone(), DataQuality::Exact),
            ),
            (
                "Branch",
                live_value(self.live.branch.clone(), DataQuality::Exact),
            ),
            (
                "Worktree",
                live_value(self.live.worktree.clone(), DataQuality::Exact),
            ),
            (
                "Identity",
                live_value(self.live.identity.clone(), DataQuality::Exact),
            ),
        ];
        let execution = vec![
            (
                "Main model",
                live_value(self.live.model.clone(), DataQuality::Exact),
            ),
            (
                "Main effort",
                live_value(self.live.effort.clone(), DataQuality::Exact),
            ),
            (
                "Agent mode",
                live_value(self.live.collaboration_mode.clone(), DataQuality::Exact),
            ),
            (
                "Active agents",
                live_value(
                    self.live.active_agents.map(|value| value.to_string()),
                    DataQuality::Exact,
                ),
            ),
            (
                "Max concurrency",
                live_value(
                    self.live.max_concurrency.map(|value| value.to_string()),
                    DataQuality::Exact,
                ),
            ),
            (
                "Approval",
                live_value(self.live.approval_mode.clone(), DataQuality::Exact),
            ),
            (
                "Access",
                live_value(self.live.access_mode.clone(), DataQuality::Exact),
            ),
            (
                "Command/tool",
                live_value(self.live.command_state.clone(), DataQuality::Exact),
            ),
        ];
        let tokens = vec![
            (
                "This-turn input",
                token_value(
                    self.live
                        .token_usage
                        .as_ref()
                        .map(|usage| usage.input_tokens),
                ),
            ),
            (
                "Cached input",
                token_value(
                    self.live
                        .token_usage
                        .as_ref()
                        .map(|usage| usage.cached_input_tokens),
                ),
            ),
            (
                "This-turn output",
                token_value(
                    self.live
                        .token_usage
                        .as_ref()
                        .map(|usage| usage.output_tokens),
                ),
            ),
            (
                "Session total",
                token_value(
                    self.live
                        .token_usage
                        .as_ref()
                        .map(|usage| usage.total_tokens),
                ),
            ),
            (
                "Context used",
                token_value(
                    self.live
                        .context
                        .as_ref()
                        .map(|context| context.used_tokens),
                ),
            ),
            (
                "Context maximum",
                token_value(
                    self.live
                        .context
                        .as_ref()
                        .map(|context| context.context_window),
                ),
            ),
            (
                "Context percentage",
                context_percent_value(self.live.context.as_ref()),
            ),
            (
                "Compactions",
                live_value(
                    self.live.compactions.map(|value| value.to_string()),
                    DataQuality::Exact,
                ),
            ),
        ];
        let performance = vec![
            (
                "Output / second",
                live_value(
                    self.live.performance.output_tokens_per_second.clone(),
                    DataQuality::Exact,
                ),
            ),
            (
                "First-token latency",
                live_value(
                    self.live.performance.first_token_latency.clone(),
                    DataQuality::Exact,
                ),
            ),
            (
                "Turn latency",
                live_value(
                    self.live.performance.turn_latency.clone(),
                    DataQuality::Exact,
                ),
            ),
            ("Session elapsed", live_value(None, DataQuality::Derived)),
            (
                "ETA",
                live_value(self.live.performance.eta.clone(), DataQuality::Derived),
            ),
            (
                "Forecast tokens",
                token_value(self.live.performance.forecast_tokens),
            ),
            (
                "Forecast context",
                live_value(
                    self.live.performance.forecast_context.clone(),
                    DataQuality::Estimated,
                ),
            ),
            (
                "Confidence",
                live_value(
                    Some(quality_name(self.live.performance.confidence).to_string()),
                    self.live.performance.confidence,
                ),
            ),
        ];
        let validation = vec![
            (
                "Tests",
                live_value(self.live.validation.tests.clone(), DataQuality::Derived),
            ),
            (
                "Build",
                live_value(self.live.validation.build.clone(), DataQuality::Exact),
            ),
            (
                "Check/lint",
                live_value(self.live.validation.check.clone(), DataQuality::Exact),
            ),
            (
                "Verification",
                live_value(verification_summary, DataQuality::Derived),
            ),
            (
                "Last failure",
                live_value(
                    latest_failure
                        .or_else(|| self.live.validation.last_failure.clone())
                        .or_else(|| self.live.last_error.clone()),
                    DataQuality::Derived,
                ),
            ),
            (
                "Evidence count",
                live_value(
                    self.live
                        .validation
                        .evidence_count
                        .or_else(|| {
                            (!self.live.verifications.is_empty())
                                .then_some(self.live.verifications.len())
                        })
                        .map(|value| value.to_string()),
                    DataQuality::Derived,
                ),
            ),
        ];
        let activity = vec![
            (
                "Tool calls",
                live_value(
                    Some(
                        self.live
                            .activity
                            .iter()
                            .filter(|event| event.event_type.contains("tool"))
                            .count()
                            .to_string(),
                    ),
                    DataQuality::Derived,
                ),
            ),
            (
                "Commands",
                live_value(
                    Some(
                        self.live
                            .activity
                            .iter()
                            .filter(|event| event.event_type.contains("command"))
                            .count()
                            .to_string(),
                    ),
                    DataQuality::Derived,
                ),
            ),
            (
                "Files changed",
                live_value(
                    self.live.files_changed.map(|value| value.to_string()),
                    DataQuality::Exact,
                ),
            ),
            (
                "Lines",
                live_value(
                    self.live
                        .additions
                        .zip(self.live.deletions)
                        .map(|(additions, deletions)| format!("+{additions} / -{deletions}")),
                    DataQuality::Exact,
                ),
            ),
            ("Agents started", live_value(None, DataQuality::Unavailable)),
            (
                "Approvals",
                live_value(
                    Some(
                        self.live
                            .activity
                            .iter()
                            .filter(|event| event.event_type == "approval")
                            .count()
                            .to_string(),
                    ),
                    DataQuality::Derived,
                ),
            ),
            (
                "Active operation",
                live_value(self.live.command_state.clone(), DataQuality::Exact),
            ),
        ];
        live_matrix(
            width,
            vec![
                ("SESSION", session),
                ("MODEL / EXECUTION", execution),
                ("TOKENS / CONTEXT", tokens),
                ("PERFORMANCE", performance),
                ("VALIDATION SUMMARY", validation),
                ("ACTIVITY SUMMARY", activity),
            ],
        )
    }

    fn activity_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::default(),
            Line::from("ACTIVITY".bold().fg(syndrid_visuals::GOLD)),
        ];
        if self.live.activity.is_empty() {
            lines.push(Line::from(
                "No observed activity is available yet · Unavailable".dim(),
            ));
        } else {
            lines.extend(self.live.activity.iter().map(|event| {
                let elapsed = event
                    .elapsed_seconds
                    .map_or_else(|| "—".to_string(), |value| format!("{value:>4}s"));
                let actor = event.actor.as_deref().unwrap_or("—");
                let status = activity_status_name(event.status);
                let duration = event
                    .duration_ms
                    .map_or_else(|| "—".to_string(), |value| format!("{value}ms"));
                let detail = format!(
                    "{elapsed}  {status:<9}  {actor:<12}  {duration:>8}  {}",
                    event.summary
                );
                Line::from(crate::syndrid_visuals::fit_text(
                    &detail,
                    usize::from(width),
                ))
            }));
        }
        lines.push(Line::default());
        lines.push(Line::from(
            "Observed workflow/tool events only · no hidden reasoning".dim(),
        ));
        lines
    }

    fn changes_lines(&self, width: u16) -> Vec<Line<'static>> {
        let summary = vec![
            ("Modified", count_value(self.live.changes.modified)),
            ("Added", count_value(self.live.changes.added)),
            ("Deleted", count_value(self.live.changes.deleted)),
            ("Untracked", count_value(self.live.changes.untracked)),
            (
                "Lines",
                live_value(
                    self.live
                        .changes
                        .additions
                        .zip(self.live.changes.deletions)
                        .map(|(additions, deletions)| format!("+{additions} / -{deletions}")),
                    DataQuality::Exact,
                ),
            ),
            (
                "Branch",
                live_value(self.live.changes.branch.clone(), DataQuality::Exact),
            ),
            (
                "Worktree",
                live_value(self.live.changes.worktree.clone(), DataQuality::Exact),
            ),
            (
                "Commit state",
                live_value(self.live.changes.commit_state.clone(), DataQuality::Exact),
            ),
        ];
        let mut files = self
            .live
            .changes
            .files
            .iter()
            .map(|file| {
                let kind = file.change_type.as_deref().unwrap_or("—");
                let counts = file.additions.zip(file.deletions).map_or_else(
                    || "—".to_string(),
                    |(additions, deletions)| format!("+{additions}/-{deletions}"),
                );
                Line::from(crate::syndrid_visuals::fit_text(
                    &format!(
                        "{kind:<9} {counts:<12} {:<14} {}",
                        file.state.as_deref().unwrap_or("—"),
                        file.path
                    ),
                    usize::from(width),
                ))
            })
            .collect::<Vec<_>>();
        if files.is_empty() {
            files.push(Line::from(
                "No structured file changes available · Unavailable".dim(),
            ));
        }
        let mut lines = live_matrix(width, vec![("SUMMARY", summary), ("FILES", Vec::new())]);
        lines.extend(files);
        lines.push(Line::default());
        lines.push(Line::from(
            format!(
                "DIFF / EVIDENCE  {}",
                self.live
                    .changes
                    .diff_summary
                    .as_deref()
                    .unwrap_or("— · Unavailable")
            )
            .fg(syndrid_visuals::GOLD)
            .bold(),
        ));
        lines
    }

    fn verification_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::default(),
            Line::from("VERIFICATION".bold().fg(syndrid_visuals::GOLD)),
        ];
        let items = if self.live.verifications.is_empty() {
            vec![Line::from(
                "No observed verification evidence · Unavailable".dim(),
            )]
        } else {
            self.live
                .verifications
                .iter()
                .map(|item| {
                    let status = verification_status_name(item.status);
                    let duration = item
                        .duration_ms
                        .map_or_else(|| "—".to_string(), |value| format!("{value}ms"));
                    let exit = item
                        .exit_code
                        .map_or_else(|| "—".to_string(), |value| value.to_string());
                    let evidence = item.evidence.as_deref().unwrap_or("—");
                    Line::from(crate::syndrid_visuals::fit_text(
                        &format!(
                            "{status:<10} {duration:>8} exit={exit:<4} {} · {} · {}",
                            item.name,
                            evidence,
                            quality_name(item.evidence_quality)
                        ),
                        usize::from(width),
                    ))
                })
                .collect()
        };
        lines.extend(items);
        lines.push(Line::default());
        lines.push(Line::from(
            "Success requires observed command/tool evidence; claims alone do not verify work"
                .dim(),
        ));
        lines
    }

    fn selected_command_line(&self, width: u16) -> u16 {
        if matches!(self.kind, SyndridScreenKind::Commands { all: true }) {
            if !self.filter.is_empty() {
                return self.selected.saturating_add(1) as u16;
            }
            let filtered = self.filtered_commands();
            let groups = all_command_groups(&filtered);
            let columns = if width >= 120 {
                3
            } else if width >= 80 {
                2
            } else {
                1
            };
            let mut line = 2usize;
            for chunk in groups.chunks(columns) {
                let row_count = chunk.iter().map(Vec::len).max().unwrap_or(0);
                if filtered.get(self.selected).is_some_and(|selected| {
                    chunk.iter().any(|commands| commands.contains(selected))
                }) {
                    let selected = filtered[self.selected];
                    let row = chunk
                        .iter()
                        .find_map(|commands| {
                            commands.iter().position(|command| *command == selected)
                        })
                        .unwrap_or(0);
                    return (line + 1 + row) as u16;
                }
                line += row_count + 3;
            }
            return line as u16;
        }
        let commands = self.filtered_commands();
        let compact = width < 50;
        let description_width = if compact {
            usize::from(width.saturating_sub(8)).max(1)
        } else {
            32.min(usize::from(width.saturating_sub(17)))
        };
        let mut line = if compact {
            0
        } else if width >= 80 {
            3
        } else {
            1
        };
        for (index, command) in commands.iter().enumerate() {
            if index == self.selected {
                return line as u16;
            }
            line += 1;
            if compact {
                line +=
                    textwrap::wrap(default_command_description(*command), description_width).len();
            }
            if matches!(
                *command,
                SlashCommand::Permissions | SlashCommand::Diff | SlashCommand::Compact
            ) {
                line += 1;
            }
        }
        line as u16
    }

    fn cycle(&mut self, forward: bool) {
        let SyndridScreenKind::Live(view) = self.kind else {
            return;
        };
        let next = if forward {
            view.next()
        } else {
            view.previous()
        };
        self.kind = SyndridScreenKind::Live(next);
        self.live.view = next;
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.filtered_commands().len();
        if len > 0 {
            self.selected = (self.selected as isize + delta).rem_euclid(len as isize) as usize;
        }
    }

    fn move_category(&mut self, delta: isize) {
        let commands = self.filtered_commands();
        let Some(current) = commands.get(self.selected).copied() else {
            return;
        };
        let current_category = command_category(current) as isize;
        for offset in 1..=6 {
            let category = (current_category + delta * offset).rem_euclid(6) as usize;
            let groups = all_command_groups(&commands);
            if let Some(next) = groups[category].first() {
                self.selected = commands
                    .iter()
                    .position(|command| command == next)
                    .expect("group command comes from filtered commands");
                return;
            }
        }
    }
}

fn all_category_names() -> [&'static str; 6] {
    [
        "SYNDRID", "SESSION", "WORKFLOW", "TOOLS", "SETTINGS", "ADVANCED",
    ]
}

fn display_command_name(command: SlashCommand) -> String {
    if command == SlashCommand::SandboxReadRoot {
        "SANDBOX READ DIR".to_string()
    } else {
        command.command().replace('-', " ").to_uppercase()
    }
}

fn command_category(command: SlashCommand) -> usize {
    match command {
        SlashCommand::Model
        | SlashCommand::Effort
        | SlashCommand::Permissions
        | SlashCommand::Status
        | SlashCommand::Usage
        | SlashCommand::Session
        | SlashCommand::Activity
        | SlashCommand::Changes
        | SlashCommand::Verification => 0,
        SlashCommand::New
        | SlashCommand::Resume
        | SlashCommand::Fork
        | SlashCommand::Rename
        | SlashCommand::Archive
        | SlashCommand::Delete
        | SlashCommand::Exit
        | SlashCommand::Quit => 1,
        SlashCommand::Plan
        | SlashCommand::Goal
        | SlashCommand::Review
        | SlashCommand::Diff
        | SlashCommand::Compact
        | SlashCommand::Init
        | SlashCommand::Mention => 2,
        SlashCommand::Mcp
        | SlashCommand::Plugins
        | SlashCommand::Skills
        | SlashCommand::Memories
        | SlashCommand::Hooks
        | SlashCommand::Ide
        | SlashCommand::Import => 3,
        SlashCommand::Keymap
        | SlashCommand::Vim
        | SlashCommand::Theme
        | SlashCommand::Personality
        | SlashCommand::Title
        | SlashCommand::Statusline => 4,
        _ => 5,
    }
}

fn all_command_groups(commands: &[SlashCommand]) -> [Vec<SlashCommand>; 6] {
    let mut groups = std::array::from_fn(|_| Vec::new());
    for command in commands {
        groups[command_category(*command)].push(*command);
    }
    for (category, commands) in groups.iter_mut().enumerate() {
        commands.sort_by_key(|command| command_order(category, *command));
    }
    groups
}

fn command_order(category: usize, command: SlashCommand) -> usize {
    match category {
        0 => match command {
            SlashCommand::Model => 0,
            SlashCommand::Effort => 1,
            SlashCommand::Status => 2,
            SlashCommand::Usage => 3,
            SlashCommand::Session => 4,
            SlashCommand::Activity => 5,
            SlashCommand::Changes => 6,
            SlashCommand::Verification => 7,
            SlashCommand::Permissions => 4,
            _ => usize::MAX,
        },
        1 => match command {
            SlashCommand::New => 0,
            SlashCommand::Resume => 1,
            SlashCommand::Fork => 2,
            SlashCommand::Rename => 3,
            SlashCommand::Archive => 4,
            SlashCommand::Delete => 5,
            SlashCommand::Exit => 6,
            SlashCommand::Quit => 7,
            _ => usize::MAX,
        },
        2 => match command {
            SlashCommand::Plan => 0,
            SlashCommand::Goal => 1,
            SlashCommand::Review => 2,
            SlashCommand::Diff => 3,
            SlashCommand::Compact => 4,
            SlashCommand::Init => 5,
            SlashCommand::Mention => 6,
            _ => usize::MAX,
        },
        3 => match command {
            SlashCommand::Mcp => 0,
            SlashCommand::Plugins => 1,
            SlashCommand::Skills => 2,
            SlashCommand::Memories => 3,
            SlashCommand::Hooks => 4,
            SlashCommand::Ide => 5,
            SlashCommand::Import => 6,
            _ => usize::MAX,
        },
        4 => match command {
            SlashCommand::Keymap => 0,
            SlashCommand::Vim => 1,
            SlashCommand::Theme => 2,
            SlashCommand::Personality => 3,
            SlashCommand::Pets => 4,
            SlashCommand::Title => 5,
            SlashCommand::Statusline => 6,
            SlashCommand::Experimental => 7,
            _ => usize::MAX,
        },
        _ => match command {
            SlashCommand::Agent => 0,
            SlashCommand::MultiAgents => 1,
            SlashCommand::Ps => 2,
            SlashCommand::Stop => 3,
            SlashCommand::Raw => 4,
            SlashCommand::Rollout => 5,
            SlashCommand::SandboxReadRoot => 6,
            _ => usize::MAX,
        },
    }
}

fn default_command_description(command: SlashCommand) -> &'static str {
    match command {
        SlashCommand::Model => "CHOOSE THE ACTIVE MODEL",
        SlashCommand::Effort => "CHANGE REASONING EFFORT",
        SlashCommand::Plan => "SWITCH TO PLAN MODE",
        SlashCommand::Permissions => "CONFIGURE APPROVAL AND ACCESS",
        SlashCommand::Status => "VIEW THE CURRENT SESSION",
        SlashCommand::Usage => "VIEW ACCOUNT ACTIVITY AND LIMITS",
        SlashCommand::Session => "VIEW THE ACTIVE SESSION DASHBOARD",
        SlashCommand::Activity => "VIEW OBSERVED SESSION ACTIVITY",
        SlashCommand::Changes => "VIEW STRUCTURED WORKSPACE CHANGES",
        SlashCommand::Verification => "VIEW VERIFICATION EVIDENCE",
        SlashCommand::Goal => "SET OR VIEW THE ACTIVE GOAL",
        SlashCommand::Review => "REVIEW CURRENT CHANGES",
        SlashCommand::Diff => "VIEW WORKSPACE CHANGES",
        SlashCommand::Resume => "RESUME A PREVIOUS SESSION",
        SlashCommand::New => "START A NEW SESSION",
        SlashCommand::Compact => "REDUCE CONTEXT USAGE",
        SlashCommand::Mcp => "VIEW CONNECTED TOOLS",
        _ => "",
    }
}

fn middle_truncate(text: &str, width: usize) -> String {
    if unicode_width::UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".to_string();
    }

    let available = width - 1;
    let prefix_width = available.div_ceil(2);
    let suffix_width = available / 2;
    let mut prefix = String::new();
    let mut used = 0;
    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > prefix_width {
            break;
        }
        prefix.push(character);
        used += character_width;
    }

    let mut suffix = String::new();
    used = 0;
    for character in text.chars().rev() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > suffix_width {
            break;
        }
        suffix.push(character);
        used += character_width;
    }
    let suffix = suffix.chars().rev().collect::<String>();
    format!("{prefix}…{suffix}")
}

fn format_count(value: i64) -> String {
    value.max(0).to_string()
}

fn quality_value(value: Option<String>, quality: DataQuality) -> Line<'static> {
    let value = value.unwrap_or_else(|| "—".to_string());
    let quality = match quality {
        DataQuality::Exact => "Exact",
        DataQuality::Derived => "Derived",
        DataQuality::Estimated => "Estimated",
        DataQuality::Unavailable => "Unavailable",
    };
    Line::from(vec![
        Span::styled(value, Style::default().fg(syndrid_visuals::PRIMARY_TEXT)),
        Span::styled(
            format!(" · {quality}"),
            Style::default().fg(syndrid_visuals::MUTED_TEXT),
        ),
    ])
}

fn live_value(value: Option<String>, quality: DataQuality) -> Line<'static> {
    quality_value(value, quality)
}

fn token_value(value: Option<i64>) -> Line<'static> {
    live_value(
        value.map(|value| value.max(0).to_string()),
        DataQuality::Exact,
    )
}

fn count_value(value: Option<usize>) -> Line<'static> {
    live_value(value.map(|value| value.to_string()), DataQuality::Exact)
}

fn context_percent_value(
    context: Option<&crate::bottom_pane::SyndridContextUsage>,
) -> Line<'static> {
    let value = context
        .filter(|context| context.context_window > 0)
        .map(|context| {
            format!(
                "{}%",
                context.used_tokens.max(0).saturating_mul(100) / context.context_window
            )
        });
    live_value(value, DataQuality::Derived)
}

fn lifecycle_value(state: LifecycleState) -> Line<'static> {
    let text = match state {
        LifecycleState::Unavailable => "—",
        LifecycleState::Working => "Working",
        LifecycleState::Ready => "Ready",
        LifecycleState::Completed => "Completed",
        LifecycleState::Failed => "Failed",
        LifecycleState::Cancelled => "Cancelled",
    };
    let quality = if state == LifecycleState::Unavailable {
        DataQuality::Unavailable
    } else {
        DataQuality::Exact
    };
    live_value(Some(text.to_string()), quality)
}

fn quality_name(quality: DataQuality) -> &'static str {
    match quality {
        DataQuality::Exact => "Exact",
        DataQuality::Derived => "Derived",
        DataQuality::Estimated => "Estimated",
        DataQuality::Unavailable => "Unavailable",
    }
}

fn activity_status_name(status: ActivityStatus) -> &'static str {
    match status {
        ActivityStatus::Unavailable => "Unavailable",
        ActivityStatus::Running => "Running",
        ActivityStatus::Passed => "Passed",
        ActivityStatus::Failed => "Failed",
        ActivityStatus::Cancelled => "Cancelled",
        ActivityStatus::Blocked => "Blocked",
    }
}

fn verification_status_name(status: VerificationStatus) -> &'static str {
    match status {
        VerificationStatus::NotRun => "Not run",
        VerificationStatus::Running => "Running",
        VerificationStatus::Passed => "Passed",
        VerificationStatus::Failed => "Failed",
        VerificationStatus::Cancelled => "Cancelled",
        VerificationStatus::Blocked => "Blocked",
        VerificationStatus::Unavailable => "Unavailable",
    }
}

fn live_matrix(
    width: u16,
    panels: Vec<(&'static str, Vec<(&'static str, Line<'static>)>)>,
) -> Vec<Line<'static>> {
    if width < 80 {
        let mut lines = vec![Line::default()];
        for (title, rows) in panels {
            lines.extend(SyndridScreen::usage_panel(title, &rows, usize::from(width)));
            lines.push(Line::default());
        }
        return lines;
    }

    let matrix_width = usize::from(width).min(110);
    let gap = 6;
    let panel_width = matrix_width.saturating_sub(gap) / 2;
    let padding = usize::from(width).saturating_sub(panel_width * 2 + gap) / 2;
    let mut lines = vec![Line::default()];
    for (index, chunk) in panels.chunks(2).enumerate() {
        if index > 0 {
            lines.extend(std::iter::repeat_n(Line::default(), 2));
        }
        let left = SyndridScreen::usage_panel(chunk[0].0, &chunk[0].1, panel_width);
        let right = chunk.get(1).map_or_else(Vec::new, |panel| {
            SyndridScreen::usage_panel(panel.0, &panel.1, panel_width)
        });
        lines.extend(SyndridScreen::combine_status_panels(
            left,
            right,
            panel_width,
            gap,
            padding,
        ));
    }
    lines
}

fn center_line(line: Line<'static>, width: usize) -> Line<'static> {
    let padding = width.saturating_sub(line.width()) / 2;
    if padding == 0 {
        return line;
    }
    let mut spans = vec![Span::raw(" ".repeat(padding))];
    spans.extend(line.spans);
    Line::from(spans)
}

impl Renderable for SyndridScreen {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        buf.set_style(area, syndrid_visuals::canvas_style());
        if matches!(
            self.kind,
            SyndridScreenKind::Status | SyndridScreenKind::Usage | SyndridScreenKind::Live(_)
        ) {
            let lines = match self.kind {
                SyndridScreenKind::Status => self.status_dashboard_lines(area.width),
                SyndridScreenKind::Usage => self.usage_dashboard_lines(area.width),
                SyndridScreenKind::Live(_) => self.lines(area.width),
                SyndridScreenKind::Commands { .. } => unreachable!(),
            };
            let scroll = self
                .scroll_offset
                .min(lines.len().saturating_sub(usize::from(area.height)))
                as u16;
            Paragraph::new(lines)
                .style(Style::default().fg(syndrid_visuals::PRIMARY_TEXT))
                .scroll((scroll, 0))
                .render(area, buf);
            return;
        }
        let all_commands = matches!(self.kind, SyndridScreenKind::Commands { all: true });
        let footer_height = if all_commands {
            if area.height >= 4 {
                2
            } else {
                u16::from(area.height >= 2)
            }
        } else {
            u16::from(area.height >= 3)
        };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(footer_height),
            ])
            .split(area);
        let title = chunks[0];
        let body = chunks[1];
        let footer = chunks[2];
        Paragraph::new(center_line(
            Line::from(self.title().fg(syndrid_visuals::BRIGHT_GOLD).bold()),
            usize::from(title.width),
        ))
        .render(title, buf);
        let lines = self.lines(area.width);
        let scroll = if matches!(self.kind, SyndridScreenKind::Status) {
            self.scroll_offset
                .min(lines.len().saturating_sub(usize::from(body.height))) as u16
        } else if matches!(self.kind, SyndridScreenKind::Commands { .. }) {
            let selected_line = self.selected_command_line(area.width);
            selected_line.saturating_sub(body.height.saturating_sub(1))
        } else {
            0
        };
        Paragraph::new(lines)
            .style(Style::default().fg(syndrid_visuals::PRIMARY_TEXT))
            .scroll((scroll, 0))
            .render(body, buf);
        let footer_lines = match self.kind {
            SyndridScreenKind::Commands { all: true } => vec![
                "←/→ CATEGORY  #  ↑/↓ SELECT  #  ENTER TO OPEN",
                "TAB FOR SYNDRID COMMANDS  #  TYPE TO SEARCH  #  ESC TO RETURN",
            ],
            SyndridScreenKind::Live(_) => {
                vec!["Tab next  ·  Shift+Tab previous  ·  Esc return"]
            }
            SyndridScreenKind::Commands { all: false } => {
                vec!["ENTER TO OPEN  #  TAB FOR ALL COMMANDS  #  ESC TO RETURN"]
            }
            _ => vec!["Enter apply/open  ·  Esc return"],
        };
        let footer_lines = footer_lines
            .into_iter()
            .map(|line| {
                center_line(
                    Line::from(line).fg(syndrid_visuals::MUTED_TEXT),
                    usize::from(footer.width),
                )
            })
            .collect::<Vec<_>>();
        Paragraph::new(footer_lines).render(footer, buf);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        1
    }
}

impl BottomPaneView for SyndridScreen {
    fn update_syndrid_state(&mut self, state: LiveSessionState) {
        if let SyndridScreenKind::Live(view) = self.kind {
            self.live = state;
            self.kind = SyndridScreenKind::Live(view);
        }
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Esc => self.complete = Some(ViewCompletion::Cancelled),
            KeyCode::Enter if matches!(self.kind, SyndridScreenKind::Commands { .. }) => {
                self.complete = Some(ViewCompletion::Accepted)
            }
            KeyCode::Up
                if matches!(
                    self.kind,
                    SyndridScreenKind::Status
                        | SyndridScreenKind::Usage
                        | SyndridScreenKind::Live(_)
                ) =>
            {
                self.scroll_offset = self.scroll_offset.saturating_sub(1)
            }
            KeyCode::Down
                if matches!(
                    self.kind,
                    SyndridScreenKind::Status
                        | SyndridScreenKind::Usage
                        | SyndridScreenKind::Live(_)
                ) =>
            {
                self.scroll_offset = self.scroll_offset.saturating_add(1)
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Left if matches!(self.kind, SyndridScreenKind::Commands { all: true }) => {
                self.move_category(-1)
            }
            KeyCode::Right if matches!(self.kind, SyndridScreenKind::Commands { all: true }) => {
                self.move_category(1)
            }
            KeyCode::Backspace if matches!(self.kind, SyndridScreenKind::Commands { .. }) => {
                self.filter.pop();
                self.selected = 0;
            }
            KeyCode::Char(ch) if matches!(self.kind, SyndridScreenKind::Commands { .. }) => {
                self.filter.push(ch);
                self.selected = 0;
            }
            KeyCode::Tab
                if matches!(self.kind, SyndridScreenKind::Commands { .. })
                    && key_event
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::SHIFT) =>
            {
                let all = matches!(self.kind, SyndridScreenKind::Commands { all: true });
                let mut next = Self::command_browser(!all);
                next.filter = self.filter.clone();
                *self = next;
            }
            KeyCode::Tab if matches!(self.kind, SyndridScreenKind::Commands { .. }) => {
                let all = matches!(self.kind, SyndridScreenKind::Commands { all: true });
                let mut next = Self::command_browser(!all);
                next.filter = self.filter.clone();
                *self = next;
            }
            KeyCode::Tab
                if key_event
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::SHIFT) =>
            {
                self.cycle(false)
            }
            KeyCode::Tab => self.cycle(true),
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.complete.is_some()
    }

    fn completion(&self) -> Option<ViewCompletion> {
        self.complete
    }

    fn selected_command(&self) -> Option<SlashCommand> {
        self.filtered_commands().get(self.selected).copied()
    }

    fn prefer_esc_to_handle_key_event(&self) -> bool {
        true
    }

    fn view_id(&self) -> Option<&'static str> {
        Some(match self.kind {
            SyndridScreenKind::Status => "syndrid-status",
            SyndridScreenKind::Usage => "syndrid-usage",
            SyndridScreenKind::Live(LiveView::Dashboard) => "syndrid-dashboard",
            SyndridScreenKind::Live(LiveView::Activity) => "syndrid-activity",
            SyndridScreenKind::Live(LiveView::Changes) => "syndrid-changes",
            SyndridScreenKind::Live(LiveView::Verification) => "syndrid-verification",
            SyndridScreenKind::Commands { .. } => "syndrid-commands",
        })
    }
}
