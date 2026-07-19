use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::bottom_pane_view::BottomPaneView;
use crate::bottom_pane::bottom_pane_view::ViewCompletion;
use crate::key_hint::KeyBindingListExt;
use crate::keymap::ListKeymap;
use crate::render::renderable::Renderable;
use crate::syndrid_visuals as sv;
use codex_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

pub(crate) struct SyndridEffortView {
    model: String,
    efforts: Vec<ReasoningEffortConfig>,
    selected: usize,
    app_event_tx: AppEventSender,
    keymap: ListKeymap,
    plan_mode: bool,
    update_model: bool,
    completion: Option<ViewCompletion>,
}

impl SyndridEffortView {
    pub(crate) fn new(
        model: String,
        efforts: Vec<ReasoningEffortConfig>,
        selected: usize,
        app_event_tx: AppEventSender,
        keymap: ListKeymap,
        plan_mode: bool,
        update_model: bool,
    ) -> Self {
        let efforts = efforts
            .into_iter()
            .filter(|effort| *effort != ReasoningEffortConfig::Ultra)
            .collect::<Vec<_>>();
        Self {
            model,
            selected: selected.min(efforts.len().saturating_sub(1)),
            efforts,
            app_event_tx,
            keymap,
            plan_mode,
            update_model,
            completion: None,
        }
    }

    fn move_left(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn move_right(&mut self) {
        if self.selected + 1 < self.efforts.len() {
            self.selected += 1;
        }
    }

    fn accept(&mut self) {
        let Some(effort) = self.efforts.get(self.selected).cloned() else {
            self.completion = Some(ViewCompletion::Cancelled);
            return;
        };
        if self.update_model {
            self.app_event_tx
                .send(AppEvent::UpdateModel(self.model.clone()));
        }
        if self.plan_mode {
            self.app_event_tx
                .send(AppEvent::UpdatePlanModeReasoningEffort(Some(effort)));
        } else {
            self.app_event_tx
                .send(AppEvent::UpdateReasoningEffort(Some(effort)));
        }
        self.completion = Some(ViewCompletion::Accepted);
    }

    fn effort_label(effort: &ReasoningEffortConfig) -> String {
        match effort {
            ReasoningEffortConfig::None => "none".to_string(),
            ReasoningEffortConfig::Minimal => "minimal".to_string(),
            ReasoningEffortConfig::Low => "low".to_string(),
            ReasoningEffortConfig::Medium => "medium".to_string(),
            ReasoningEffortConfig::High => "high".to_string(),
            ReasoningEffortConfig::XHigh => "xhigh".to_string(),
            ReasoningEffortConfig::Max => "max".to_string(),
            ReasoningEffortConfig::Ultra => "ultracode".to_string(),
            ReasoningEffortConfig::Custom(value) => value.clone(),
        }
    }

    fn scale_lines(&self, width: usize) -> Vec<Line<'static>> {
        let labels = self
            .efforts
            .iter()
            .map(Self::effort_label)
            .collect::<Vec<_>>();
        let compact = width < 48;
        let scale_label_width =
            UnicodeWidthStr::width("Faster") + UnicodeWidthStr::width("Smarter");
        let mut lines = vec![Line::from(vec![
            sv::secondary("Faster"),
            Span::raw(" ".repeat(width.saturating_sub(scale_label_width))),
            sv::secondary("Smarter"),
        ])];
        if compact {
            for (idx, label) in labels.iter().enumerate() {
                let selected = idx == self.selected;
                let text = format!("{} {}", if selected { "◆" } else { "◇" }, label);
                lines.push(Line::from(if selected {
                    sv::active(sv::fit_text(&text, width))
                } else {
                    sv::secondary(sv::fit_text(&text, width))
                }));
            }
            return lines;
        }
        let item_width = labels
            .iter()
            .map(|label| UnicodeWidthStr::width(label.as_str()) + 2)
            .sum::<usize>();
        let gap = width.saturating_sub(item_width) / labels.len().saturating_sub(1).max(1);
        lines.push(Line::from(
            labels
                .iter()
                .enumerate()
                .flat_map(|(idx, label)| {
                    let span = if idx == self.selected {
                        sv::active(format!("◆ {label}"))
                    } else {
                        sv::secondary(format!("◇ {label}"))
                    };
                    let spacer = (idx + 1 < labels.len()).then(|| Span::raw(" ".repeat(gap)));
                    [Some(span), spacer]
                })
                .flatten()
                .collect::<Vec<_>>(),
        ));
        lines
    }
}

impl BottomPaneView for SyndridEffortView {
    fn view_id(&self) -> Option<&'static str> {
        Some("syndrid-effort")
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if self.keymap.move_left.is_pressed(key_event) || self.keymap.move_up.is_pressed(key_event)
        {
            self.move_left();
        } else if self.keymap.move_right.is_pressed(key_event)
            || self.keymap.move_down.is_pressed(key_event)
        {
            self.move_right();
        } else if self.keymap.accept.is_pressed(key_event) {
            self.accept();
        } else if self.keymap.cancel.is_pressed(key_event) {
            self.completion = Some(ViewCompletion::Cancelled);
        }
    }

    fn is_complete(&self) -> bool {
        self.completion.is_some()
    }

    fn completion(&self) -> Option<ViewCompletion> {
        self.completion
    }

    fn prefer_esc_to_handle_key_event(&self) -> bool {
        true
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.completion = Some(ViewCompletion::Cancelled);
        CancellationEvent::Handled
    }
}

impl Renderable for SyndridEffortView {
    fn desired_height(&self, width: u16) -> u16 {
        if width < 48 {
            12 + self.efforts.len() as u16
        } else {
            12
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        Block::default()
            .style(Style::default().bg(sv::BACKGROUND).fg(sv::PRIMARY_TEXT))
            .render(area, buf);
        let panel_area = Rect::new(
            area.x.saturating_add(2),
            area.y,
            area.width.saturating_sub(4),
            area.height,
        );
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(sv::BORDER))
            .style(Style::default().bg(sv::BACKGROUND));
        let inner = block.inner(panel_area);
        block.render(panel_area, buf);
        let width = usize::from(inner.width);
        let selected = self
            .efforts
            .get(self.selected)
            .map(Self::effort_label)
            .unwrap_or_else(|| "—".to_string());
        let description = match selected.as_str() {
            "low" => "Faster responses with lighter reasoning.",
            "medium" => "Balanced speed and reasoning depth.",
            "high" => "More deliberate reasoning for difficult work.",
            "xhigh" => "Deep reasoning when quality matters most.",
            "max" => "Maximum provider reasoning for this session.",
            _ => "Provider-supported reasoning for this session.",
        };
        let mut lines = vec![
            sv::page_title("Select effort"),
            Line::from(sv::secondary(
                "Change reasoning effort for the current session.",
            )),
            Line::from(vec![
                sv::muted("Model  "),
                sv::active(sv::fit_text(&self.model, width)),
            ]),
            Line::default(),
        ];
        lines.extend(self.scale_lines(width));
        lines.push(Line::default());
        lines.push(Line::from(vec![
            sv::muted("Selected  "),
            sv::active(selected),
            sv::muted("  ·  "),
            sv::secondary(sv::fit_text(&description, width.saturating_sub(24))),
        ]));
        lines.push(Line::default());
        lines.push(Line::from(vec![
            sv::secondary("←/→"),
            sv::muted(" adjust  ·  "),
            sv::secondary("Enter"),
            sv::muted(" confirm  ·  "),
            sv::secondary("Esc"),
            sv::muted(" cancel"),
        ]));
        Paragraph::new(lines).render(inner, buf);
    }
}
