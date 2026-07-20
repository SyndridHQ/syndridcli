//! Focused, full-screen Syndrid session surfaces.
//!
//! These views are presentation-only. They consume cached values supplied by the
//! TUI owner and deliberately render an em dash for data that is not available.

use crate::bottom_pane::BottomPaneView;
use crate::bottom_pane::ViewCompletion;
use crate::render::renderable::Renderable;
use crate::slash_command::SlashCommand;
use crate::syndrid_live_state::LiveSessionState;
use crate::syndrid_live_state::LiveView;
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
    live: LiveSessionState,
    complete: Option<ViewCompletion>,
    selected: usize,
    commands: Vec<SlashCommand>,
    filter: String,
}

impl SyndridScreen {
    pub(crate) fn status() -> Self {
        Self::new(SyndridScreenKind::Status)
    }

    pub(crate) fn usage() -> Self {
        Self::new(SyndridScreenKind::Usage)
    }

    pub(crate) fn live(state: LiveSessionState) -> Self {
        Self {
            kind: SyndridScreenKind::Live(state.view),
            live: state,
            complete: None,
            selected: 0,
            commands: Vec::new(),
            filter: String::new(),
        }
    }

    fn new(kind: SyndridScreenKind) -> Self {
        Self {
            kind,
            live: LiveSessionState::default(),
            complete: None,
            selected: 0,
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
            live: LiveSessionState::default(),
            complete: None,
            selected: 0,
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
            SyndridScreenKind::Status => vec![
                Line::from("SESSION".bold().fg(syndrid_visuals::GOLD)),
                Self::metric("State", Self::value(self.live.status.clone())),
                Self::metric("Current task", Self::value(self.live.task.clone())),
                Self::metric("Current step", Self::value(self.live.step.clone())),
                Line::from(""),
                Line::from("EXECUTION".bold().fg(syndrid_visuals::GOLD)),
                Self::metric("Activity", self.live.activity_count.to_string()),
                Self::metric("Approvals", "—".to_string()),
                Line::from(""),
                Line::from("MODEL".bold().fg(syndrid_visuals::GOLD)),
                Self::metric("Model", Self::value(self.live.model.clone())),
                Self::metric("Effort", Self::value(self.live.effort.clone())),
                Self::metric("Context", self.context()),
                Line::from(""),
                Line::from("POLICY · HEALTH".bold().fg(syndrid_visuals::GOLD)),
                Self::metric("Approval", "—".to_string()),
                Self::metric("Access", "—".to_string()),
                Self::metric("Last error", Self::value(self.live.last_error.clone())),
            ],
            SyndridScreenKind::Usage => vec![
                Line::from("ACCOUNT".bold().fg(syndrid_visuals::GOLD)),
                Self::metric("Account", "—".to_string()),
                Self::metric("Plan", "—".to_string()),
                Line::from(""),
                Line::from("LIMITS".bold().fg(syndrid_visuals::GOLD)),
                Self::metric("Today", "—".to_string()),
                Self::metric("This week", "—".to_string()),
                Self::metric("Reset", "—".to_string()),
                Line::from(""),
                Line::from("TOKENS".bold().fg(syndrid_visuals::GOLD)),
                Self::metric("Input", "—".to_string()),
                Self::metric("Cached input", "—".to_string()),
                Self::metric("Output", "—".to_string()),
                Self::metric("Lifetime", "—".to_string()),
                Line::from(""),
                Line::from("Unavailable account fields are shown as —".dim()),
            ],
            SyndridScreenKind::Live(LiveView::Dashboard) => self.dashboard_lines(width),
            SyndridScreenKind::Live(LiveView::Activity) => vec![
                Line::from("RAW CODEX ACTIVITY".bold().fg(syndrid_visuals::GOLD)),
                Line::from("No activity has been recorded for this focused view yet.".dim()),
                Line::from("Tool calls, commands, edits, approvals, warnings, and errors remain copyable in the transcript.".dim()),
            ],
            SyndridScreenKind::Live(LiveView::Changes) => vec![
                Line::from("WORKSPACE CHANGES".bold().fg(syndrid_visuals::GOLD)),
                Self::metric("Files changed", Self::value(self.live.files_changed)),
                Self::metric("Additions", Self::value(self.live.additions)),
                Self::metric("Deletions", Self::value(self.live.deletions)),
                Line::from(""),
                Line::from("Latest patch".bold().fg(syndrid_visuals::SECONDARY_TEXT)),
                Line::from("—".dim()),
            ],
            SyndridScreenKind::Live(LiveView::Verification) => vec![
                Line::from("VERIFICATION".bold().fg(syndrid_visuals::GOLD)),
                Self::metric("Format", "—".to_string()),
                Self::metric("Lint", "—".to_string()),
                Self::metric("Cargo check", "—".to_string()),
                Self::metric("Focused tests", "—".to_string()),
                Self::metric("Build", "—".to_string()),
                Line::from(""),
                Self::metric("Last failure", Self::value(self.live.last_error.clone())),
            ],
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

    fn dashboard_lines(&self, width: u16) -> Vec<Line<'static>> {
        let state = Self::value(self.live.status.clone());
        let heading = format!("{state}  ·  Elapsed: —");
        let current = vec![
            Self::metric("Current task", Self::value(self.live.task.clone())),
            Self::metric("Current step", Self::value(self.live.step.clone())),
            Self::metric("Progress", "—".to_string()),
        ];
        let columns = if width >= 96 { 3 } else { 1 };
        let mut lines = vec![Line::from(heading.fg(syndrid_visuals::BRIGHT_GOLD).bold())];
        lines.push(Line::from(""));
        lines.extend(current);
        lines.push(Line::from(""));
        if columns == 3 {
            lines.push(center_line(
                Line::from(
                    "EXECUTION                 CHANGES                 VERIFICATION"
                        .fg(syndrid_visuals::GOLD)
                        .bold(),
                ),
                usize::from(width),
            ));
            lines.push(center_line(
                Line::from(format!(
                    "Tools: —                 Files: {}                 Format: —",
                    Self::value(self.live.files_changed)
                )),
                usize::from(width),
            ));
            lines.push(center_line(
                Line::from(format!(
                    "Commands: —             Lines: +{} / -{}          Check: —",
                    Self::value(self.live.additions),
                    Self::value(self.live.deletions)
                )),
                usize::from(width),
            ));
            lines.push(center_line(
                Line::from("Approvals: —            Latest: —                Tests: —"),
                usize::from(width),
            ));
        } else {
            lines.push(Line::from("EXECUTION".fg(syndrid_visuals::GOLD).bold()));
            lines.push(Self::metric("Tools", "—".to_string()));
            lines.push(Self::metric("Commands", "—".to_string()));
            lines.push(Self::metric("Approvals", "—".to_string()));
            lines.push(Line::from("CHANGES".fg(syndrid_visuals::GOLD).bold()));
            lines.push(Self::metric("Files", Self::value(self.live.files_changed)));
            lines.push(Self::metric(
                "Lines",
                format!(
                    "+{} / -{}",
                    Self::value(self.live.additions),
                    Self::value(self.live.deletions)
                ),
            ));
            lines.push(Line::from("VERIFICATION".fg(syndrid_visuals::GOLD).bold()));
            lines.push(Self::metric("Format", "—".to_string()));
            lines.push(Self::metric("Check", "—".to_string()));
            lines.push(Self::metric("Tests", "—".to_string()));
        }
        lines.extend([
            Line::from(""),
            Self::metric("Model", Self::value(self.live.model.clone())),
            Self::metric("Effort", Self::value(self.live.effort.clone())),
            Self::metric("Context", self.context()),
            Self::metric("Tokens", "—".to_string()),
            Line::from(""),
            Line::from("PLAN".fg(syndrid_visuals::GOLD).bold()),
            Self::metric("Declared plan", "—".to_string()),
        ]);
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
        | SlashCommand::Usage => 0,
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
        let scroll = if matches!(self.kind, SyndridScreenKind::Commands { .. }) {
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
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Esc => self.complete = Some(ViewCompletion::Cancelled),
            KeyCode::Enter if matches!(self.kind, SyndridScreenKind::Commands { .. }) => {
                self.complete = Some(ViewCompletion::Accepted)
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
