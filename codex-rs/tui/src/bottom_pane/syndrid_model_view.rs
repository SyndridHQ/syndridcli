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
        let width = usize::from(area.width);
        let narrow = width < 60;
        let name_width = self
            .models
            .iter()
            .map(|model| UnicodeWidthStr::width(model.model.to_uppercase().as_str()))
            .max()
            .unwrap_or(1)
            .min(width.saturating_sub(12).max(1));
        let description = |model: &ModelPreset| {
            if model.description.is_empty() {
                "—".to_string()
            } else {
                model.description.clone()
            }
        };
        let fixed_width = 9 + 1 + 1 + 1 + name_width + 3;
        let description_width = self
            .models
            .iter()
            .map(|model| UnicodeWidthStr::width(description(model).as_str()))
            .max()
            .unwrap_or(1)
            .min(width.saturating_sub(fixed_width).max(1));
        let mut rows = Vec::with_capacity(self.models.len());
        for (index, model) in self.models.iter().enumerate() {
            let selected = index == self.selected;
            let current = model.model == self.current_model;
            let marker = if selected { "#" } else { " " };
            let current_marker = if current { "(current)" } else { "" };
            let name = sv::padded(
                &sv::fit_text(&model.model.to_uppercase(), name_width),
                name_width,
            );
            let marker_style = if selected {
                sv::active(marker)
            } else {
                Span::from(marker)
            };
            let name_style = if selected {
                sv::active(name)
            } else {
                Span::from(name)
            };
            let current_style = if current {
                sv::secondary(sv::padded(current_marker, 9))
            } else {
                Span::raw(" ".repeat(9))
            };
            let description = description(model);
            let row = if narrow {
                let mut row = vec![
                    current_style,
                    Span::raw(" "),
                    marker_style,
                    Span::raw(" "),
                    name_style,
                ];
                let description_lines =
                    textwrap::wrap(&description, width.saturating_sub(2).max(1));
                let wrapped = if description_lines.is_empty() {
                    vec!["—".to_string()]
                } else {
                    description_lines
                        .into_iter()
                        .map(std::borrow::Cow::into_owned)
                        .collect()
                };
                let mut lines = vec![Line::from(row.split_off(0))];
                lines.extend(wrapped.into_iter().map(|line| {
                    Line::from(vec![
                        Span::raw("  "),
                        sv::secondary(sv::fit_text(&line, width.saturating_sub(2).max(1))),
                    ])
                }));
                lines
            } else {
                vec![Line::from(vec![
                    current_style,
                    Span::raw(" "),
                    marker_style,
                    Span::raw(" "),
                    name_style,
                    sv::secondary(" │ "),
                    sv::secondary(sv::fit_text(&description, description_width)),
                ])]
            };
            rows.push(row);
        }

        let footer_text = "PRESS ENTER TO CONFIRM # ESC TO GO BACK";
        let footer_width = UnicodeWidthStr::width(footer_text) as u16;
        let footer_y = area.bottom().saturating_sub(2);
        let list_bottom = footer_y.saturating_sub(3);
        let available_height = usize::from(list_bottom.saturating_sub(area.y));
        let total_height = rows.iter().map(Vec::len).sum::<usize>();
        let (first, list_top) = if total_height <= available_height {
            (
                0,
                area.y
                    + u16::try_from(available_height.saturating_sub(total_height) / 2).unwrap_or(0),
            )
        } else {
            let mut first = self.selected.min(rows.len().saturating_sub(1));
            while first > 0
                && rows[first..=self.selected]
                    .iter()
                    .map(Vec::len)
                    .sum::<usize>()
                    <= available_height
            {
                first -= 1;
            }
            (first, area.y)
        };
        let mut lines = Vec::new();
        let mut used_height = 0;
        for row in rows.into_iter().skip(first) {
            if used_height >= available_height {
                break;
            }
            let remaining = available_height - used_height;
            let take = row.len().min(remaining);
            lines.extend(row.into_iter().take(take));
            used_height += take;
        }
        let block_width = if narrow {
            width
        } else {
            fixed_width + description_width
        };
        let left = usize::from(area.width.saturating_sub(block_width as u16)) / 2;
        Paragraph::new(lines).render(
            Rect::new(
                area.x + left as u16,
                list_top,
                block_width as u16,
                available_height as u16,
            ),
            buf,
        );
        Paragraph::new(Line::from(sv::secondary(footer_text))).render(
            Rect::new(
                area.x + area.width.saturating_sub(footer_width) / 2,
                footer_y,
                footer_width.min(area.width),
                1,
            ),
            buf,
        );
    }
}
