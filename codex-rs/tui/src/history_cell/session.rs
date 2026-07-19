//! Session headers, onboarding guidance, and transcript cards.

use super::*;

pub(crate) const SESSION_HEADER_MAX_INNER_WIDTH: usize = 56; // Just an eyeballed value

pub(crate) fn card_inner_width(width: u16, max_inner_width: usize) -> Option<usize> {
    if width < 4 {
        return None;
    }
    let inner_width = std::cmp::min(width.saturating_sub(4) as usize, max_inner_width);
    Some(inner_width)
}

/// Render `lines` inside a border sized to the widest span in the content.
pub(crate) fn with_border(lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    with_border_internal(lines, /*forced_inner_width*/ None)
}

/// Render `lines` inside a border whose inner width is at least `inner_width`.
///
/// This is useful when callers have already clamped their content to a
/// specific width and want the border math centralized here instead of
/// duplicating padding logic in the TUI widgets themselves.
pub(crate) fn with_border_with_inner_width(
    lines: Vec<Line<'static>>,
    inner_width: usize,
) -> Vec<Line<'static>> {
    with_border_internal(lines, Some(inner_width))
}

fn with_border_internal(
    lines: Vec<Line<'static>>,
    forced_inner_width: Option<usize>,
) -> Vec<Line<'static>> {
    let max_line_width = lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0);
    let content_width = forced_inner_width
        .unwrap_or(max_line_width)
        .max(max_line_width);

    let mut out = Vec::with_capacity(lines.len() + 2);
    let border_inner_width = content_width + 2;
    out.push(vec![format!("╭{}╮", "─".repeat(border_inner_width)).dim()].into());

    for line in lines.into_iter() {
        let used_width: usize = line
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum();
        let span_count = line.spans.len();
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(span_count + 4);
        spans.push(Span::from("│ ").dim());
        spans.extend(line);
        if used_width < content_width {
            spans.push(Span::from(" ".repeat(content_width - used_width)).dim());
        }
        spans.push(Span::from(" │").dim());
        out.push(Line::from(spans));
    }

    out.push(vec![format!("╰{}╯", "─".repeat(border_inner_width)).dim()].into());

    out
}

/// Return the emoji followed by a hair space (U+200A).
/// Using only the hair space avoids excessive padding after the emoji while
/// still providing a small visual gap across terminals.
pub(crate) fn padded_emoji(emoji: &str) -> String {
    format!("{emoji}\u{200A}")
}

#[derive(Debug)]
struct TooltipHistoryCell {
    tip: String,
    cwd: PathBuf,
}

impl TooltipHistoryCell {
    fn new(tip: String, cwd: &Path) -> Self {
        Self {
            tip,
            cwd: cwd.to_path_buf(),
        }
    }
}

impl HistoryCell for TooltipHistoryCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let indent = "  ";
        let indent_width = UnicodeWidthStr::width(indent);
        let wrap_width = usize::from(width.max(1))
            .saturating_sub(indent_width)
            .max(1);
        let mut lines: Vec<Line<'static>> = Vec::new();
        append_markdown(
            &format!("**Tip:** {}", self.tip),
            Some(wrap_width),
            Some(self.cwd.as_path()),
            &mut lines,
        );

        prefix_lines(lines, indent.into(), indent.into())
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        vec![Line::from(format!("Tip: {}", self.tip))]
    }
}

#[derive(Debug)]
pub struct SessionInfoCell(CompositeHistoryCell, Arc<RwLock<SessionHeaderLiveState>>);

impl HistoryCell for SessionInfoCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.0.display_lines(width)
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.0.desired_height(width)
    }

    fn transcript_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.0.transcript_lines(width)
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        self.0.raw_lines()
    }
}

impl SessionInfoCell {
    pub(crate) fn live_state_handle(&self) -> Arc<RwLock<SessionHeaderLiveState>> {
        Arc::clone(&self.1)
    }
}

#[cfg(test)]
pub(crate) fn new_session_info(
    config: &Config,
    requested_model: &str,
    session: &ThreadSessionState,
    is_first_event: bool,
    tooltip_override: Option<String>,
    auth_plan: Option<PlanType>,
    show_fast_status: bool,
) -> SessionInfoCell {
    new_session_info_with_brand(
        config,
        requested_model,
        session,
        is_first_event,
        tooltip_override,
        auth_plan,
        show_fast_status,
        codex_utils_cli::PublicBrand::Codex,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn new_session_info_with_brand(
    config: &Config,
    requested_model: &str,
    session: &ThreadSessionState,
    is_first_event: bool,
    tooltip_override: Option<String>,
    auth_plan: Option<PlanType>,
    show_fast_status: bool,
    public_brand: codex_utils_cli::PublicBrand,
) -> SessionInfoCell {
    // Header box rendered as history (so it appears at the very top)
    let header = SessionHeaderHistoryCell::new(
        session.model.clone(),
        session.reasoning_effort.clone(),
        show_fast_status,
        config.cwd.to_path_buf(),
        CODEX_CLI_VERSION,
    )
    .with_public_brand(public_brand)
    .with_session_id(session.thread_id.to_string())
    .with_yolo_mode(has_yolo_permissions(
        session.approval_policy,
        &session.permission_profile,
    ));
    let live_state = header.live_state_handle();
    let mut parts: Vec<Box<dyn HistoryCell>> = vec![Box::new(header)];

    if is_first_event && public_brand == codex_utils_cli::PublicBrand::Codex {
        // Help lines below the header (new copy and list)
        let help_lines: Vec<Line<'static>> = vec![
            "  To get started, describe a task or try one of these commands:"
                .dim()
                .into(),
            Line::from(""),
            Line::from(vec![
                "  ".into(),
                "/init".into(),
                " - create an AGENTS.md file with instructions for Codex".dim(),
            ]),
            Line::from(vec![
                "  ".into(),
                "/status".into(),
                " - show current session configuration".dim(),
            ]),
            Line::from(vec![
                "  ".into(),
                "/permissions".into(),
                " - choose what Codex is allowed to do".dim(),
            ]),
            Line::from(vec![
                "  ".into(),
                "/model".into(),
                " - choose what model and reasoning effort to use".dim(),
            ]),
            Line::from(vec![
                "  ".into(),
                "/review".into(),
                " - review any changes and find issues".dim(),
            ]),
        ];

        parts.push(Box::new(PlainHistoryCell { lines: help_lines }));
    } else if !is_first_event {
        if config.show_tooltips
            && let Some(tooltips) = tooltip_override
                .or_else(|| tooltips::get_tooltip(auth_plan, show_fast_status))
                .map(|tip| TooltipHistoryCell::new(tip, &config.cwd))
        {
            parts.push(Box::new(tooltips));
        }
        if requested_model != session.model.as_str() {
            let lines = vec![
                "model changed:".magenta().bold().into(),
                format!("requested: {requested_model}").into(),
                format!("used: {}", session.model).into(),
            ];
            parts.push(Box::new(PlainHistoryCell { lines }));
        }
    }

    SessionInfoCell(CompositeHistoryCell { parts }, live_state)
}

pub(crate) fn is_yolo_mode(config: &Config) -> bool {
    has_yolo_permissions(
        AskForApproval::from(config.permissions.approval_policy.value()),
        &config.permissions.effective_permission_profile(),
    )
}

pub(crate) fn has_yolo_permissions(
    approval_policy: AskForApproval,
    permission_profile: &PermissionProfile,
) -> bool {
    approval_policy == AskForApproval::Never
        && matches!(
            permission_profile,
            PermissionProfile::Disabled
                | PermissionProfile::Managed {
                    file_system: ManagedFileSystemPermissions::Unrestricted,
                    network: NetworkSandboxPolicy::Enabled,
                }
        )
}
#[derive(Debug)]
pub(crate) struct SessionHeaderHistoryCell {
    version: &'static str,
    live_state: Arc<RwLock<SessionHeaderLiveState>>,
    model_style: Style,
    show_fast_status: bool,
    directory: PathBuf,
    public_brand: codex_utils_cli::PublicBrand,
    session_id: Option<String>,
    yolo_mode: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionHeaderLiveState {
    pub(crate) model: String,
    pub(crate) reasoning_effort: Option<ReasoningEffortConfig>,
}

impl SessionHeaderHistoryCell {
    pub(crate) fn new(
        model: String,
        reasoning_effort: Option<ReasoningEffortConfig>,
        show_fast_status: bool,
        directory: PathBuf,
        version: &'static str,
    ) -> Self {
        Self::new_with_style(
            model,
            Style::default(),
            reasoning_effort,
            show_fast_status,
            directory,
            version,
        )
    }

    pub(crate) fn new_with_style(
        model: String,
        model_style: Style,
        reasoning_effort: Option<ReasoningEffortConfig>,
        show_fast_status: bool,
        directory: PathBuf,
        version: &'static str,
    ) -> Self {
        Self {
            version,
            live_state: Arc::new(RwLock::new(SessionHeaderLiveState {
                model,
                reasoning_effort,
            })),
            model_style,
            show_fast_status,
            directory,
            public_brand: codex_utils_cli::PublicBrand::Codex,
            session_id: None,
            yolo_mode: false,
        }
    }

    pub(crate) fn with_public_brand(mut self, public_brand: codex_utils_cli::PublicBrand) -> Self {
        self.public_brand = public_brand;
        self
    }

    pub(crate) fn with_session_id(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub(crate) fn with_yolo_mode(mut self, yolo_mode: bool) -> Self {
        self.yolo_mode = yolo_mode;
        self
    }

    fn format_directory(&self, max_width: Option<usize>) -> String {
        Self::format_directory_inner(&self.directory, max_width)
    }

    pub(crate) fn format_directory_inner(directory: &Path, max_width: Option<usize>) -> String {
        let formatted = if let Some(rel) = relativize_to_home(directory) {
            if rel.as_os_str().is_empty() {
                "~".to_string()
            } else {
                format!("~{}{}", std::path::MAIN_SEPARATOR, rel.display())
            }
        } else {
            directory.display().to_string()
        };

        if let Some(max_width) = max_width {
            if max_width == 0 {
                return String::new();
            }
            if UnicodeWidthStr::width(formatted.as_str()) > max_width {
                return crate::text_formatting::center_truncate_path(&formatted, max_width);
            }
        }

        formatted
    }

    fn live_state(&self) -> SessionHeaderLiveState {
        self.live_state
            .read()
            .expect("session header live state poisoned")
            .clone()
    }

    fn reasoning_label(&self) -> Option<String> {
        self.live_state()
            .reasoning_effort
            .as_ref()
            .map(ReasoningEffortConfig::as_str)
            .map(str::to_string)
    }

    pub(crate) fn live_state_handle(&self) -> Arc<RwLock<SessionHeaderLiveState>> {
        Arc::clone(&self.live_state)
    }

    fn syndrid_display_lines(&self, width: u16) -> Vec<Line<'static>> {
        use crate::syndrid_visuals as sv;

        let width = usize::from(width);
        if width < 4 {
            return Vec::new();
        }
        let outer_width = width.min(120);
        if outer_width < 68 {
            return self.syndrid_narrow_lines(outer_width);
        }

        let left_width = if outer_width >= 104 {
            52
        } else {
            ((outer_width.saturating_sub(3)) * 45 / 100).max(30)
        };
        let right_width = outer_width.saturating_sub(left_width + 3);
        let title = format!("Syndrid CLI v{}", self.version);
        let title_width = UnicodeWidthStr::width(title.as_str());
        let right_rule_left = right_width.saturating_sub(title_width) / 2;
        let right_rule_right = right_width.saturating_sub(title_width + right_rule_left);

        let mut lines = vec![Line::from(vec![
            sv::border(format!("╭{}┬", "─".repeat(left_width))),
            sv::border("─".repeat(right_rule_left)),
            sv::active(title),
            sv::border(format!("{}╮", "─".repeat(right_rule_right))),
        ])];

        let cwd = self.format_directory(Some(left_width.saturating_sub(2)));
        let session_id = self.session_id.as_deref().unwrap_or("—");
        let effort = self.reasoning_label().unwrap_or_else(|| "—".to_string());
        let model = self.live_state().model;
        let left_rows = [
            sv::centered(&cwd, left_width),
            "─".repeat(left_width),
            sv::centered("*   \\ /   *", left_width),
            sv::centered("*    .-(* *)-.    *", left_width),
            sv::centered(r"/    ^    \", left_width),
            sv::centered("*    \\  \\___/  /    *", left_width),
            sv::centered("\\| |/", left_width),
            "─".repeat(left_width),
            sv::centered("· https://github.com/SyndridHQ ·", left_width),
        ];
        let right_rows = [
            format!(" session id: {session_id}"),
            format!(" model: {model}"),
            format!(" effort: {effort}    Tokens Sparked: —"),
            "─".repeat(right_width),
            " Patch Notes:".to_string(),
            " —".to_string(),
            String::new(),
            String::new(),
            String::new(),
        ];

        for (idx, (left, right)) in left_rows.into_iter().zip(right_rows).enumerate() {
            if idx == 1 {
                lines.push(Line::from(vec![
                    sv::border(format!("├{}┤", "─".repeat(left_width))),
                    sv::secondary(sv::padded(&right, right_width)),
                    sv::border("│"),
                ]));
                continue;
            }
            if idx == 3 {
                lines.push(Line::from(vec![
                    sv::border("│"),
                    Span::styled(
                        sv::padded(&left, left_width),
                        Style::default().fg(sv::BRIGHT_GOLD),
                    ),
                    sv::border(format!("├{}┤", "─".repeat(right_width))),
                ]));
                continue;
            }
            if idx == 7 {
                lines.push(Line::from(vec![
                    sv::border(format!("├{}┤", "─".repeat(left_width))),
                    sv::secondary(sv::padded(&right, right_width)),
                    sv::border("│"),
                ]));
                continue;
            }
            let left_style = if (2..=6).contains(&idx) {
                Style::default().fg(sv::BRIGHT_GOLD)
            } else {
                Style::default().fg(sv::SECONDARY_TEXT)
            };
            let right_style = if idx == 1 || idx == 2 {
                Style::default().fg(sv::PRIMARY_TEXT)
            } else {
                Style::default().fg(sv::SECONDARY_TEXT)
            };
            lines.push(Line::from(vec![
                sv::border("│"),
                Span::styled(sv::padded(&left, left_width), left_style),
                sv::border("│"),
                Span::styled(sv::padded(&right, right_width), right_style),
                sv::border("│"),
            ]));
        }
        lines.push(Line::from(sv::border(format!(
            "╰{}┴{}╯",
            "─".repeat(left_width),
            "─".repeat(right_width)
        ))));
        lines.push(Line::from(vec![
            sv::muted(" type "),
            sv::active("/"),
            sv::muted(" to explore Syndrid"),
        ]));
        lines
    }

    fn syndrid_narrow_lines(&self, outer_width: usize) -> Vec<Line<'static>> {
        use crate::syndrid_visuals as sv;

        let inner = outer_width.saturating_sub(2);
        let row = |text: &str, style: Style| {
            Line::from(vec![
                sv::border("│"),
                Span::styled(sv::padded(text, inner), style),
                sv::border("│"),
            ])
        };
        let separator = || Line::from(sv::border(format!("├{}┤", "─".repeat(inner))));
        let mut lines = vec![
            Line::from(sv::border(format!("╭{}╮", "─".repeat(inner)))),
            row(
                &sv::centered(&format!("Syndrid CLI v{}", self.version), inner),
                Style::default().fg(sv::GOLD).bold(),
            ),
            row(
                &sv::centered(&self.format_directory(Some(inner.saturating_sub(2))), inner),
                Style::default().fg(sv::SECONDARY_TEXT),
            ),
            separator(),
        ];
        for mascot in [
            "*   \\ /   *",
            "*   .-(* *)-.   *",
            r"/    ^    \",
            "*   \\  \\___/  /   *",
            "\\| |/",
        ] {
            lines.push(row(
                &sv::centered(mascot, inner),
                Style::default().fg(sv::BRIGHT_GOLD),
            ));
        }
        lines.extend([
            separator(),
            row(
                &sv::centered("· https://github.com/SyndridHQ ·", inner),
                Style::default().fg(sv::SECONDARY_TEXT),
            ),
            separator(),
            row(
                &format!(" session id: {}", self.session_id.as_deref().unwrap_or("—")),
                Style::default().fg(sv::PRIMARY_TEXT),
            ),
            row(
                &format!(" model: {}", self.live_state().model),
                Style::default().fg(sv::PRIMARY_TEXT),
            ),
            row(
                &format!(
                    " effort: {}",
                    self.reasoning_label().unwrap_or_else(|| "—".to_string())
                ),
                Style::default().fg(sv::PRIMARY_TEXT),
            ),
            row(
                " Tokens Sparked: —",
                Style::default().fg(sv::SECONDARY_TEXT),
            ),
            row(" Patch Notes: —", Style::default().fg(sv::SECONDARY_TEXT)),
            Line::from(sv::border(format!("╰{}╯", "─".repeat(inner)))),
        ]);
        if outer_width >= 26 {
            lines.push(Line::from(vec![
                sv::muted(" type "),
                sv::active("/"),
                sv::muted(" to explore Syndrid"),
            ]));
        }
        lines
    }
}

impl HistoryCell for SessionHeaderHistoryCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        if self.public_brand == codex_utils_cli::PublicBrand::Syndrid {
            return self.syndrid_display_lines(width);
        }

        let Some(inner_width) = card_inner_width(width, SESSION_HEADER_MAX_INNER_WIDTH) else {
            return Vec::new();
        };

        let make_row = |spans: Vec<Span<'static>>| Line::from(spans);

        // Title line rendered inside the box: ">_ OpenAI Codex (vX)"
        let title_spans: Vec<Span<'static>> = vec![
            Span::from(">_ ").dim(),
            Span::from(self.public_brand.tui_header()).bold(),
            Span::from(" ").dim(),
            Span::from(format!("(v{})", self.version)).dim(),
        ];

        const CHANGE_MODEL_HINT_COMMAND: &str = "/model";
        const CHANGE_MODEL_HINT_EXPLANATION: &str = " to change";
        const DIR_LABEL: &str = "directory:";
        const PERMISSIONS_LABEL: &str = "permissions:";
        let label_width = if self.yolo_mode {
            DIR_LABEL.len().max(PERMISSIONS_LABEL.len())
        } else {
            DIR_LABEL.len()
        };

        let model_label = format!(
            "{model_label:<label_width$}",
            model_label = "model:",
            label_width = label_width
        );
        let reasoning_label = self.reasoning_label();
        let model = self.live_state().model;
        let model_spans: Vec<Span<'static>> = {
            let mut spans = vec![
                Span::from(format!("{model_label} ")).dim(),
                Span::styled(model, self.model_style),
            ];
            if let Some(reasoning) = reasoning_label {
                spans.push(Span::from(" "));
                spans.push(Span::from(reasoning.to_owned()));
            }
            if self.show_fast_status {
                spans.push("   ".into());
                spans.push(Span::styled("fast", self.model_style.magenta()));
            }
            spans.push("   ".dim());
            spans.push(CHANGE_MODEL_HINT_COMMAND.cyan());
            spans.push(CHANGE_MODEL_HINT_EXPLANATION.dim());
            spans
        };

        let dir_label = format!("{DIR_LABEL:<label_width$}");
        let dir_prefix = format!("{dir_label} ");
        let dir_prefix_width = UnicodeWidthStr::width(dir_prefix.as_str());
        let dir_max_width = inner_width.saturating_sub(dir_prefix_width);
        let dir = self.format_directory(Some(dir_max_width));
        let dir_spans = vec![Span::from(dir_prefix).dim(), Span::from(dir)];

        let mut lines = vec![
            make_row(title_spans),
            make_row(Vec::new()),
            make_row(model_spans),
            make_row(dir_spans),
        ];

        if self.yolo_mode {
            let permissions_label = format!("{PERMISSIONS_LABEL:<label_width$}");
            lines.push(make_row(vec![
                Span::from(format!("{permissions_label} ")).dim(),
                "YOLO mode".magenta().bold(),
            ]));
        }

        with_border(lines)
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(format!(
                "{} (v{})",
                self.public_brand.tui_header(),
                self.version
            )),
            Line::from(format!(
                "model: {}{}",
                self.live_state().model,
                self.reasoning_label()
                    .map(|reasoning| format!(" {reasoning}"))
                    .unwrap_or_default()
            )),
            Line::from(format!(
                "directory: {}",
                self.format_directory(/*max_width*/ None)
            )),
        ];
        if self.yolo_mode {
            lines.push(Line::from("permissions: YOLO mode"));
        }
        lines
    }
}
