//! Render composition for the main chat widget surface.

use super::*;

impl ChatWidget {
    pub(super) fn as_renderable(&self) -> RenderableItem<'_> {
        if self.bottom_pane.has_syndrid_focused_owner() {
            return RenderableItem::Owned(Box::new(SyndridFullscreenRenderable {
                bottom_pane: &self.bottom_pane,
            }));
        }
        if self.bottom_pane.has_fullscreen_syndrid_command_browser()
            || self.bottom_pane.has_fullscreen_syndrid_selector()
        {
            return RenderableItem::Owned(Box::new(SyndridFullscreenRenderable {
                bottom_pane: &self.bottom_pane,
            }));
        }
        if self.bottom_pane.has_fullscreen_syndrid_view() {
            let mut flex = FlexRenderable::new();
            flex.push(
                /*flex*/ 1,
                RenderableItem::Owned(Box::new(SyndridFullscreenRenderable {
                    bottom_pane: &self.bottom_pane,
                })),
            );
            flex.push(
                /*flex*/ 0,
                RenderableItem::Owned(Box::new(SyndridComposerReserveRenderable {
                    bottom_pane: &self.bottom_pane,
                    right_reserve: self.ambient_pet_wrap_reserved_cols(),
                }))
                .inset(Insets::tlbr(
                    /*top*/ 1, /*left*/ 0, /*bottom*/ 0, /*right*/ 0,
                )),
            );
            return RenderableItem::Owned(Box::new(flex));
        }
        if self.should_render_syndrid_home() {
            let mut flex = FlexRenderable::new();
            flex.push(
                /*flex*/ 0,
                RenderableItem::Owned(Box::new(SyndridHomeRenderable { chat: self })),
            );
            flex.push(
                /*flex*/ 0,
                RenderableItem::Owned(Box::new(SyndridComposerReserveRenderable {
                    bottom_pane: &self.bottom_pane,
                    right_reserve: self.ambient_pet_wrap_reserved_cols(),
                })),
            );
            return RenderableItem::Owned(Box::new(flex));
        }
        let active_cell_right_reserve = self.ambient_pet_wrap_reserved_cols();
        let active_cell_renderable = match &self.transcript.active_cell {
            Some(cell) => RenderableItem::Owned(Box::new(TranscriptAreaRenderable {
                child: cell.as_ref(),
                top: 1,
                right: active_cell_right_reserve,
            })),
            None => RenderableItem::Owned(Box::new(())),
        };
        let active_hook_cell_renderable = match &self.active_hook_cell {
            Some(cell) if cell.should_render() => {
                RenderableItem::Owned(Box::new(TranscriptAreaRenderable {
                    child: cell,
                    top: 1,
                    right: active_cell_right_reserve,
                }))
            }
            _ => RenderableItem::Owned(Box::new(())),
        };
        let mut flex = FlexRenderable::new();
        flex.push(/*flex*/ 1, active_cell_renderable);
        flex.push(/*flex*/ 0, active_hook_cell_renderable);
        if let Some(cell) = self.pending_token_activity_output() {
            flex.push(
                /*flex*/ 1,
                RenderableItem::Owned(Box::new(TranscriptAreaRenderable {
                    child: cell,
                    top: 1,
                    right: active_cell_right_reserve,
                })),
            );
        }
        if let Some(cell) = self.pending_rate_limit_reset_hint() {
            flex.push(
                /*flex*/ 1,
                RenderableItem::Owned(Box::new(TranscriptAreaRenderable {
                    child: cell,
                    top: 1,
                    right: active_cell_right_reserve,
                })),
            );
        }
        flex.push(
            /*flex*/ 0,
            RenderableItem::Owned(Box::new(BottomPaneComposerReserveRenderable {
                bottom_pane: &self.bottom_pane,
                right_reserve: active_cell_right_reserve,
            }))
            .inset(Insets::tlbr(
                /*top*/ 1, /*left*/ 0, /*bottom*/ 0, /*right*/ 0,
            )),
        );
        RenderableItem::Owned(Box::new(flex))
    }

    fn should_render_syndrid_home(&self) -> bool {
        self.public_brand == codex_utils_cli::PublicBrand::Syndrid
            && !self.bottom_pane.is_task_running()
            && !self.bottom_pane.has_active_view()
            && self.transcript.visible_user_turn_count == 0
    }
}

struct SyndridHomeRenderable<'a> {
    chat: &'a ChatWidget,
}

impl Renderable for SyndridHomeRenderable<'_> {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let width = usize::from(area.width);
        let session_id = self
            .chat
            .thread_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "—".to_string());
        let workspace = self.chat.config.cwd.as_path().display().to_string();
        let model = self.chat.current_model().to_string();
        let effort = self
            .chat
            .effective_reasoning_effort()
            .as_ref()
            .map(|effort| effort.as_str().to_string())
            .unwrap_or_else(|| "—".to_string());
        let lifetime = self
            .chat
            .syndrid_account_lifetime_tokens
            .map(format_lifetime_tokens)
            .unwrap_or_else(|| "—".to_string());
        let lines = syndrid_home_lines(width, session_id, workspace, model, effort, lifetime);
        Paragraph::new(lines)
            .style(crate::syndrid_visuals::canvas_style())
            .render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        syndrid_home_height(usize::from(width))
    }
}

fn format_lifetime_tokens(tokens: i64) -> String {
    let digits = tokens.max(0).to_string();
    let first_group = digits.len() % 3;
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    if first_group > 0 {
        grouped.push_str(&digits[..first_group]);
    }
    for (index, chunk) in digits[first_group..].as_bytes().chunks(3).enumerate() {
        if first_group > 0 || index > 0 {
            grouped.push(',');
        }
        grouped.push_str(std::str::from_utf8(chunk).expect("token digits are ASCII"));
    }
    grouped
}

fn syndrid_home_height(width: usize) -> u16 {
    if width < 40 { 16 } else { 11 }
}

fn syndrid_home_lines(
    width: usize,
    session_id: String,
    workspace: String,
    model: String,
    effort: String,
    lifetime: String,
) -> Vec<Line<'static>> {
    let workspace = crate::syndrid_visuals::fit_text(&workspace, width.saturating_sub(2));
    let mut lines = vec![crate::syndrid_visuals::horizontal_rule(width)];
    if width < 40 {
        lines.push(center_home_line(
            &format!("Session ID: {session_id}"),
            width,
        ));
        lines.push(center_home_line("# SYNDRID CONNECTED", width));
    } else {
        let left_width = width / 2;
        let right_width = width.saturating_sub(left_width);
        let right_content_width = right_width.saturating_sub(1);
        let connection =
            crate::syndrid_visuals::fit_text("# SYNDRID CONNECTED", right_content_width);
        let connection_left = right_width
            .saturating_sub(1)
            .saturating_sub(unicode_width::UnicodeWidthStr::width(connection.as_str()));
        lines.push(Line::from(vec![
            crate::syndrid_visuals::secondary(crate::syndrid_visuals::padded(
                &format!("Session ID: {session_id}"),
                left_width,
            )),
            crate::syndrid_visuals::secondary(" ".repeat(connection_left)),
            crate::syndrid_visuals::active(connection),
            crate::syndrid_visuals::secondary(" "),
        ]));
    }
    lines.extend([
        Line::from(""),
        center_home_line("Welcome back!", width),
        Line::from(""),
        center_home_line(
            &format!(
                "You are currently running Syndrid CLI on v{}",
                crate::version::CODEX_CLI_VERSION
            ),
            width,
        ),
    ]);
    let metadata = [
        ("Directory", workspace),
        ("Model", model),
        ("Effort", effort),
        ("Lifetime Tokens", lifetime),
    ];
    if width < 40 {
        for (label, value) in metadata {
            lines.push(center_home_line(label, width));
            lines.push(center_home_line(&value, width));
        }
    } else {
        let label_width = metadata
            .iter()
            .map(|(label, _)| unicode_width::UnicodeWidthStr::width(*label))
            .max()
            .unwrap_or(0);
        let value_width = metadata
            .iter()
            .map(|(_, value)| unicode_width::UnicodeWidthStr::width(value.as_str()))
            .max()
            .unwrap_or(0)
            .min(width.saturating_sub(label_width + 3));
        for (label, value) in metadata {
            let value = crate::syndrid_visuals::fit_text(&value, value_width);
            let row = format!("{label:<label_width$} │ {value:<value_width$}",);
            lines.push(Line::from(crate::syndrid_visuals::centered(&row, width)));
        }
    }
    lines.push(Line::from(""));
    lines
}

fn center_home_line(text: &str, width: usize) -> Line<'static> {
    Line::from(crate::syndrid_visuals::centered(text, width))
        .fg(crate::syndrid_visuals::PRIMARY_TEXT)
}

struct SyndridFullscreenRenderable<'a> {
    bottom_pane: &'a BottomPane,
}

impl Renderable for SyndridFullscreenRenderable<'_> {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.bottom_pane.render_fullscreen_syndrid_view(area, buf);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        u16::MAX
    }
}

struct SyndridComposerReserveRenderable<'a> {
    bottom_pane: &'a BottomPane,
    right_reserve: u16,
}

impl Renderable for SyndridComposerReserveRenderable<'_> {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.bottom_pane
            .render_composer_only_with_right_reserve(area, buf, self.right_reserve);
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.bottom_pane
            .desired_composer_height_with_right_reserve(width, self.right_reserve)
    }

    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.bottom_pane
            .composer_cursor_pos_with_right_reserve(area, self.right_reserve)
    }

    fn cursor_style(&self, area: Rect) -> crossterm::cursor::SetCursorStyle {
        self.bottom_pane.composer_cursor_style(area)
    }
}

struct BottomPaneComposerReserveRenderable<'a> {
    bottom_pane: &'a BottomPane,
    right_reserve: u16,
}

impl Renderable for BottomPaneComposerReserveRenderable<'_> {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.bottom_pane
            .render_with_composer_right_reserve(area, buf, self.right_reserve);
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.bottom_pane
            .desired_height_with_composer_right_reserve(width, self.right_reserve)
    }

    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.bottom_pane
            .cursor_pos_with_composer_right_reserve(area, self.right_reserve)
    }

    fn cursor_style(&self, area: Rect) -> crossterm::cursor::SetCursorStyle {
        self.bottom_pane
            .cursor_style_with_composer_right_reserve(area, self.right_reserve)
    }
}

struct TranscriptAreaRenderable<'a> {
    child: &'a dyn HistoryCell,
    top: u16,
    right: u16,
}

impl Renderable for TranscriptAreaRenderable<'_> {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let area = self.child_area(area);
        let lines = self.child.display_lines(area.width);
        let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
        let y = if area.height == 0 {
            0
        } else {
            let overflow = paragraph
                .line_count(area.width)
                .saturating_sub(usize::from(area.height));
            u16::try_from(overflow).unwrap_or(u16::MAX)
        };
        Clear.render(area, buf);
        paragraph.scroll((y, 0)).render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        let child_width = width.saturating_sub(self.right).max(1);
        HistoryCell::desired_height(self.child, child_width) + self.top
    }
}

impl TranscriptAreaRenderable<'_> {
    fn child_area(&self, area: Rect) -> Rect {
        let y = area.y.saturating_add(self.top);
        let height = area.height.saturating_sub(self.top);
        Rect::new(
            area.x,
            y,
            area.width.saturating_sub(self.right).max(1),
            height,
        )
    }
}

impl Renderable for ChatWidget {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let frame_owner = self.bottom_pane.syndrid_frame_owner();
        if self.last_rendered_syndrid_owner.get() != Some(frame_owner)
            || self.last_rendered_syndrid_area.get() != Some((area.width, area.height))
        {
            ratatui::widgets::Clear.render(area, buf);
            self.last_rendered_syndrid_owner.set(Some(frame_owner));
            self.last_rendered_syndrid_area
                .set(Some((area.width, area.height)));
        }
        if self.public_brand == codex_utils_cli::PublicBrand::Syndrid {
            buf.set_style(area, crate::syndrid_visuals::canvas_style());
        }
        self.as_renderable().render(area, buf);
        self.last_rendered_width.set(Some(area.width as usize));
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.as_renderable().desired_height(width)
    }

    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.as_renderable().cursor_pos(area)
    }

    fn cursor_style(&self, area: Rect) -> crossterm::cursor::SetCursorStyle {
        self.as_renderable().cursor_style(area)
    }
}
