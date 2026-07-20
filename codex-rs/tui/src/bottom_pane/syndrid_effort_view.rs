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
use ratatui::text::Line;
use ratatui::text::Span;
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
        if width < 60 {
            8 + self.efforts.len() as u16 * 2
        } else {
            8
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        ratatui::widgets::Block::default()
            .style(sv::canvas_style())
            .render(area, buf);
        let width = usize::from(area.width);
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
        let narrow = width < 60;
        let mut lines = vec![
            Line::default(),
            Line::from(vec![
                sv::secondary("FASTER"),
                Span::raw(" ".repeat(width.saturating_sub(14))),
                sv::secondary("SMARTER"),
            ]),
        ];
        if narrow {
            for (idx, effort) in self.efforts.iter().enumerate() {
                let label = Self::effort_label(effort).to_uppercase();
                lines.push(Line::from(if idx == self.selected {
                    sv::active(format!("# {label}"))
                } else {
                    sv::secondary(format!("  {label}"))
                }));
            }
        } else {
            let labels = self
                .efforts
                .iter()
                .map(Self::effort_label)
                .collect::<Vec<_>>();
            let rail_width = width.saturating_sub(10).max(labels.len() * 6);
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(width.saturating_sub(rail_width) / 2)),
                sv::border(format!("|{}|", "─".repeat(rail_width.saturating_sub(2)))),
            ]));
            let mut spans = Vec::new();
            let gap = rail_width.saturating_sub(
                labels
                    .iter()
                    .map(|label| UnicodeWidthStr::width(label.as_str()))
                    .sum(),
            ) / labels.len().saturating_sub(1).max(1);
            for (idx, label) in labels.iter().enumerate() {
                if idx > 0 {
                    spans.push(Span::raw(" ".repeat(gap)));
                }
                spans.push(if idx == self.selected {
                    sv::active(label.to_uppercase())
                } else {
                    sv::secondary(label.to_uppercase())
                });
            }
            lines.push(Line::from(spans));
        }
        lines.push(Line::from(vec![
            sv::secondary("LIGHT"),
            Span::raw(" ".repeat(width.saturating_sub(12))),
            sv::secondary("HEAVY"),
        ]));
        lines.push(Line::default());
        lines.push(Line::from(vec![
            sv::muted("Model "),
            sv::active(sv::fit_text(&self.model, width.saturating_sub(8))),
            sv::muted("  "),
            sv::secondary(sv::fit_text(&description, width.saturating_sub(24))),
        ]));
        lines.push(Line::default());
        let footer = if narrow {
            "←/→ ADJUST # ENTER # ESC"
        } else {
            "←/→ TO ADJUST # ENTER TO CONFIRM # ESC TO RETURN"
        };
        lines.push(Line::from(sv::secondary(footer)));
        let x = area.x + area.width.saturating_sub(width as u16) / 2;
        Paragraph::new(lines).render(Rect::new(x, area.y, width as u16, area.height), buf);
    }
}
