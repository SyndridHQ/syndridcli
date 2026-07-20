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
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

pub(crate) struct SyndridModelView {
    models: Vec<ModelPreset>,
    selected: usize,
    current_model: String,
    app_event_tx: AppEventSender,
    keymap: ListKeymap,
    completion: Option<ViewCompletion>,
}

impl SyndridModelView {
    pub(crate) fn new(
        models: Vec<ModelPreset>,
        selected: usize,
        current_model: String,
        _configured_model: Option<String>,
        app_event_tx: AppEventSender,
        keymap: ListKeymap,
    ) -> Self {
        Self {
            selected: selected.min(models.len().saturating_sub(1)),
            models,
            current_model,
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
        if width < 48 {
            8u16.saturating_add(self.models.len() as u16 * 2)
        } else {
            10
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        Block::default().style(sv::canvas_style()).render(area, buf);
        let narrow = area.width < 60;
        let name_width = self
            .models
            .iter()
            .map(|model| UnicodeWidthStr::width(model.model.as_str()))
            .max()
            .unwrap_or(1)
            .min(32);
        let marker_width = 1;
        let separator = 3;
        let description_width = self
            .models
            .iter()
            .map(|model| UnicodeWidthStr::width(model.description.as_str()))
            .max()
            .unwrap_or(1)
            .min(48);
        let block_width = (marker_width + 1 + name_width + separator + description_width)
            .min(usize::from(area.width));
        let left = usize::from(area.width.saturating_sub(block_width as u16)) / 2;
        let mut lines = Vec::new();
        let visible_rows = usize::from(area.height.saturating_sub(4)).max(1);
        let start = self
            .selected
            .saturating_sub(visible_rows.saturating_sub(1) / 2);
        for (idx, model) in self
            .models
            .iter()
            .enumerate()
            .skip(start)
            .take(visible_rows)
        {
            let selected = idx == self.selected;
            let current = model.model == self.current_model;
            let marker = if selected { "#" } else { " " };
            let current_label = if current { " (current)" } else { "" };
            let name = format!("{marker} {}{current_label}", model.model.to_uppercase());
            let name = sv::padded(&name, name_width + 2 + usize::from(current));
            let description = if model.description.is_empty() {
                "—"
            } else {
                model.description.as_str()
            };
            let line = if narrow {
                vec![if selected {
                    sv::active(name)
                } else {
                    Span::from(name)
                }]
            } else {
                vec![
                    if selected {
                        sv::active(name)
                    } else {
                        Span::from(name)
                    },
                    sv::secondary(" │ "),
                    sv::secondary(sv::fit_text(description, description_width)),
                ]
            };
            lines.push(Line::from(line));
            if narrow {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    sv::secondary(sv::fit_text(
                        description,
                        usize::from(area.width.saturating_sub(2)),
                    )),
                ]));
            }
        }
        let footer = Line::from(sv::secondary("PRESS ENTER TO CONFIRM # ESC TO GO BACK"));
        let content_height = lines.len() as u16;
        Paragraph::new(lines).render(
            Rect::new(
                area.x + left as u16,
                area.y + 1,
                block_width as u16,
                area.height.saturating_sub(3).max(content_height),
            ),
            buf,
        );
        let footer_width = UnicodeWidthStr::width("PRESS ENTER TO CONFIRM # ESC TO GO BACK") as u16;
        Paragraph::new(footer).render(
            Rect::new(
                area.x + area.width.saturating_sub(footer_width) / 2,
                area.bottom().saturating_sub(1),
                footer_width,
                1,
            ),
            buf,
        );
    }
}
