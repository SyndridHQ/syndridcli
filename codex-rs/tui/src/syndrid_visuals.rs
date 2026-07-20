//! Small presentation primitives shared by Syndrid-only TUI surfaces.

#![allow(dead_code)]

use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

// Syndrid's three surface levels are intentionally kept in one module.  The
// values are presentation-only: Codex's semantic colors and terminal palette
// remain untouched.
pub(crate) const BACKGROUND: Color = Color::Rgb(0x0D, 0x0B, 0x09);
pub(crate) const PANEL: Color = Color::Rgb(0x18, 0x14, 0x10);
pub(crate) const FOCUSED_SURFACE: Color = Color::Rgb(0x24, 0x1C, 0x14);
pub(crate) const RAISED_SURFACE: Color = PANEL;
pub(crate) const SOFT_SURFACE: Color = FOCUSED_SURFACE;
pub(crate) const PRIMARY_TEXT: Color = Color::Rgb(0xF2, 0xEE, 0xE4);
pub(crate) const SECONDARY_TEXT: Color = Color::Rgb(0xB2, 0xA9, 0x9A);
pub(crate) const MUTED_TEXT: Color = Color::Rgb(0x77, 0x6B, 0x5D);
pub(crate) const INACTIVE_TEXT: Color = Color::Rgb(0x5A, 0x4D, 0x40);
pub(crate) const BORDER: Color = Color::Rgb(0x49, 0x39, 0x2B);
pub(crate) const GOLD: Color = Color::Rgb(0xD8, 0xA8, 0x3A);
pub(crate) const BRIGHT_GOLD: Color = Color::Rgb(0xF1, 0xC5, 0x55);
pub(crate) const SOFT_GOLD: Color = Color::Rgb(0xAF, 0x7E, 0x2B);
pub(crate) const DIM_GOLD: Color = Color::Rgb(0x76, 0x54, 0x24);
pub(crate) const SUCCESS: Color = Color::Rgb(0x78, 0xB8, 0x7A);
pub(crate) const ERROR: Color = Color::Rgb(0xD9, 0x70, 0x70);
pub(crate) const INFO: Color = Color::Rgb(0x7F, 0xA6, 0xC9);

pub(crate) fn page_title(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(
        text.into(),
        Style::default().fg(PRIMARY_TEXT).bold(),
    ))
}

pub(crate) fn muted(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(MUTED_TEXT))
}

pub(crate) fn secondary(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(SECONDARY_TEXT))
}

pub(crate) fn active(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(GOLD).bold())
}

pub(crate) fn border(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(BORDER))
}

pub(crate) fn canvas_style() -> Style {
    Style::default().bg(BACKGROUND).fg(PRIMARY_TEXT)
}

pub(crate) fn panel_style() -> Style {
    Style::default().bg(PANEL).fg(PRIMARY_TEXT)
}

pub(crate) fn focused_style() -> Style {
    Style::default().bg(FOCUSED_SURFACE).fg(PRIMARY_TEXT)
}

pub(crate) fn fit_text(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    crate::text_formatting::center_truncate_path(text, width)
}

pub(crate) fn padded(text: &str, width: usize) -> String {
    let fitted = fit_text(text, width);
    let used = UnicodeWidthStr::width(fitted.as_str());
    format!("{fitted}{}", " ".repeat(width.saturating_sub(used)))
}

pub(crate) fn centered(text: &str, width: usize) -> String {
    let fitted = fit_text(text, width);
    let used = UnicodeWidthStr::width(fitted.as_str());
    let left = width.saturating_sub(used) / 2;
    let right = width.saturating_sub(used + left);
    format!("{}{}{}", " ".repeat(left), fitted, " ".repeat(right))
}

pub(crate) fn horizontal_rule(width: usize) -> Line<'static> {
    Line::from(border("─".repeat(width)))
}
