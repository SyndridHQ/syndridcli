//! Small presentation primitives shared by Syndrid-only TUI surfaces.

#![allow(dead_code)]

use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

pub(crate) const BACKGROUND: Color = Color::Rgb(0x0B, 0x0B, 0x0D);
pub(crate) const RAISED_SURFACE: Color = Color::Rgb(0x14, 0x14, 0x16);
pub(crate) const SOFT_SURFACE: Color = Color::Rgb(0x1A, 0x1A, 0x1E);
pub(crate) const PRIMARY_TEXT: Color = Color::Rgb(0xF2, 0xF0, 0xEA);
pub(crate) const SECONDARY_TEXT: Color = Color::Rgb(0xA8, 0xA6, 0xA0);
pub(crate) const MUTED_TEXT: Color = Color::Rgb(0x6F, 0x6D, 0x68);
pub(crate) const BORDER: Color = Color::Rgb(0x34, 0x32, 0x38);
pub(crate) const GOLD: Color = Color::Rgb(0xD6, 0xA8, 0x3A);
pub(crate) const BRIGHT_GOLD: Color = Color::Rgb(0xF2, 0xC9, 0x4C);
pub(crate) const SOFT_GOLD: Color = Color::Rgb(0xB8, 0x8A, 0x2A);
pub(crate) const DIM_GOLD: Color = Color::Rgb(0x78, 0x5D, 0x25);
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
