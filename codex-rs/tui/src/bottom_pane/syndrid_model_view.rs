use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::bottom_pane_view::BottomPaneView;
use crate::bottom_pane::bottom_pane_view::ViewCompletion;
use crate::key_hint::KeyBindingListExt;
use crate::keymap::ListKeymap;
use crate::render::renderable::Renderable;
use crate::syndrid_visuals as sv;
use codex_protocol::openai_models::ModelPreset;
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
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::Wrap;

pub(crate) struct SyndridModelView {
    models: Vec<ModelPreset>,
    selected: usize,
    current_model: String,
    configured_model: Option<String>,
    app_event_tx: AppEventSender,
    keymap: ListKeymap,
    completion: Option<ViewCompletion>,
}

impl SyndridModelView {
    pub(crate) fn new(
        models: Vec<ModelPreset>,
        selected: usize,
        current_model: String,
        configured_model: Option<String>,
        app_event_tx: AppEventSender,
        keymap: ListKeymap,
    ) -> Self {
        Self {
            selected: selected.min(models.len().saturating_sub(1)),
            models,
            current_model,
            configured_model,
            app_event_tx,
            keymap,
            completion: None,
        }
    }

    fn move_by(&mut self, delta: isize) {
        if self.models.is_empty() {
            return;
        }
        self.selected = if delta.is_negative() {
            self.selected.saturating_sub(delta.unsigned_abs())
        } else {
            (self.selected + delta as usize).min(self.models.len() - 1)
        };
    }

    fn accept(&mut self) {
        let Some(model) = self.models.get(self.selected).cloned() else {
            self.completion = Some(ViewCompletion::Cancelled);
            return;
        };
        self.app_event_tx
            .send(AppEvent::OpenReasoningPopup { model });
        self.completion = Some(ViewCompletion::Accepted);
    }

    fn effort_summary(model: &ModelPreset) -> String {
        let labels = model
            .supported_reasoning_efforts
            .iter()
            .filter(|option| option.effort != codex_protocol::openai_models::ReasoningEffort::Ultra)
            .map(|option| match &option.effort {
                codex_protocol::openai_models::ReasoningEffort::None => "none".to_string(),
                codex_protocol::openai_models::ReasoningEffort::Minimal => "minimal".to_string(),
                codex_protocol::openai_models::ReasoningEffort::Low => "low".to_string(),
                codex_protocol::openai_models::ReasoningEffort::Medium => "medium".to_string(),
                codex_protocol::openai_models::ReasoningEffort::High => "high".to_string(),
                codex_protocol::openai_models::ReasoningEffort::XHigh => "xhigh".to_string(),
                codex_protocol::openai_models::ReasoningEffort::Max => "max".to_string(),
                codex_protocol::openai_models::ReasoningEffort::Ultra => unreachable!(),
                codex_protocol::openai_models::ReasoningEffort::Custom(value) => value.clone(),
            })
            .collect::<Vec<_>>();
        if labels.is_empty() {
            "effort unavailable".to_string()
        } else {
            labels.join(" · ")
        }
    }
}

impl BottomPaneView for SyndridModelView {
    fn view_id(&self) -> Option<&'static str> {
        Some("syndrid-model")
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if self.keymap.move_up.is_pressed(key_event) || self.keymap.move_left.is_pressed(key_event)
        {
            self.move_by(-1);
        } else if self.keymap.move_down.is_pressed(key_event)
            || self.keymap.move_right.is_pressed(key_event)
        {
            self.move_by(1);
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
    fn on_ctrl_c(&mut self) -> crate::bottom_pane::CancellationEvent {
        self.completion = Some(ViewCompletion::Cancelled);
        crate::bottom_pane::CancellationEvent::Handled
    }
}

impl Renderable for SyndridModelView {
    fn desired_height(&self, width: u16) -> u16 {
        let rows = self.models.len() as u16;
        if width < 36 {
            10u16.saturating_add(rows).min(16)
        } else {
            10u16.saturating_add(rows.saturating_mul(2)).min(18)
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let panel_width = area.width.saturating_sub(4).max(1);
        let panel_area = Rect::new(area.x.saturating_add(2), area.y, panel_width, area.height);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(sv::BORDER))
            .style(Style::default().bg(sv::BACKGROUND));
        let inner = block.inner(panel_area);
        block.render(panel_area, buf);
        let content = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(inner);
        Paragraph::new(vec![
            sv::page_title("Select model"),
            Line::from(sv::secondary(
                "Choose a model for this session, then set its reasoning effort.",
            )),
        ])
        .render(content[0], buf);

        let row_width = usize::from(content[1].width.saturating_sub(2));
        let compact = content[1].width < 60;
        let row_height = if compact { 1 } else { 2 };
        let visible_rows = usize::from(content[1].height).max(1) / row_height;
        let start = self
            .selected
            .saturating_sub(visible_rows.saturating_sub(1) / 2);
        let end = (start + visible_rows.max(1)).min(self.models.len());
        let mut lines = Vec::new();
        for (idx, model) in self.models.iter().enumerate().skip(start).take(end - start) {
            let active = idx == self.selected;
            let marker = if active { "◆" } else { "◇" };
            let state = if model.model == self.current_model {
                " current"
            } else if self.configured_model.as_deref() == Some(model.model.as_str()) {
                " configured"
            } else {
                ""
            };
            let name = format!("{marker} {}{state}", model.model);
            let name = sv::fit_text(&name, row_width);
            let style = if active {
                Style::default().fg(sv::GOLD).bold()
            } else {
                Style::default().fg(sv::PRIMARY_TEXT)
            };
            lines.push(Line::from(Span::styled(name, style)));
            if !compact {
                let description = if model.description.is_empty() {
                    Self::effort_summary(model)
                } else {
                    format!("{} · {}", model.description, Self::effort_summary(model))
                };
                let desc_width = row_width.saturating_sub(2);
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    sv::muted(sv::fit_text(&description, desc_width)),
                ]));
            }
        }
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .render(content[1], buf);
        Paragraph::new(Line::from(vec![
            sv::secondary(" Effort: "),
            sv::active("select after model"),
            sv::muted("  ·  Enter continue  ·  Esc cancel"),
        ]))
        .render(content[2], buf);
    }
}
