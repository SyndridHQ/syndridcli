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

    fn track_positions(count: usize, track_width: usize) -> Vec<usize> {
        if count == 0 {
            return Vec::new();
        }
        if count == 1 {
            return vec![track_width / 2];
        }
        (0..count)
            .map(|index| (index * track_width.saturating_sub(1) + (count - 1) / 2) / (count - 1))
            .collect()
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
        if width < 48 { 10 } else { 9 }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        ratatui::widgets::Block::default()
            .style(sv::canvas_style())
            .render(area, buf);
        let width = usize::from(area.width);
        let narrow = width < 48;
        let footer = "←/→ TO ADJUST # ENTER TO CONFIRM # ESC TO RETURN";
        let footer_width = UnicodeWidthStr::width(footer);
        let block_width = footer_width.min(width);
        let track_width = if narrow {
            width.saturating_sub(8).max(19)
        } else {
            39
        }
        .min(width);
        let track_left = (block_width.saturating_sub(track_width)) / 2;
        let positions = Self::track_positions(self.efforts.len(), track_width);
        let labels = self
            .efforts
            .iter()
            .map(Self::effort_label)
            .map(|label| label.to_uppercase())
            .collect::<Vec<_>>();

        let mut track = Vec::new();
        for (index, position) in positions.iter().enumerate() {
            if index > 0 {
                let previous = positions[index - 1];
                track.push(Span::raw("─".repeat(position.saturating_sub(previous + 1))));
            }
            track.push(if index == self.selected {
                sv::active("|")
            } else {
                sv::border("|")
            });
        }
        let final_gap = track_width.saturating_sub(positions.last().copied().unwrap_or(0) + 1);
        track.push(Span::raw("─".repeat(final_gap)));

        let mut label_line = Vec::new();
        let mut cursor = 0;
        for (index, label) in labels.iter().enumerate() {
            let label_width = UnicodeWidthStr::width(label.as_str());
            let desired_start = track_left + positions[index].saturating_sub(label_width / 2);
            let start = desired_start.max(cursor);
            label_line.push(Span::raw(" ".repeat(start.saturating_sub(cursor))));
            label_line.push(if index == self.selected {
                sv::active(label)
            } else {
                sv::secondary(label)
            });
            cursor = start + label_width;
        }
        let mut top = vec![Span::raw(" ".repeat(track_left)), sv::secondary("FASTER")];
        let top_gap = track_width.saturating_sub(6 + 7);
        top.push(Span::raw(" ".repeat(top_gap)));
        top.push(sv::secondary("SMARTER"));
        let mut bottom = vec![Span::raw(" ".repeat(track_left)), sv::secondary("LIGHT")];
        let bottom_gap = track_width.saturating_sub(5 + 5);
        bottom.push(Span::raw(" ".repeat(bottom_gap)));
        bottom.push(sv::secondary("HEAVY"));

        let mut lines = vec![
            Line::default(),
            Line::from(top),
            Line::default(),
            Line::from({
                let mut line = vec![Span::raw(" ".repeat(track_left))];
                line.extend(track);
                line
            }),
            Line::from(label_line),
            Line::default(),
            Line::from(bottom),
            Line::default(),
        ];
        if narrow && footer_width > width {
            lines.push(Line::from(sv::secondary(
                "←/→ TO ADJUST # ENTER TO CONFIRM",
            )));
            lines.push(Line::from(sv::secondary("# ESC TO RETURN")));
        } else {
            lines.push(Line::from(sv::secondary(footer)));
        }
        let content_height = lines.len().min(usize::from(area.height));
        let top = area.y + area.height.saturating_sub(content_height as u16) / 2;
        Paragraph::new(lines).render(
            Rect::new(
                area.x + area.width.saturating_sub(block_width as u16) / 2,
                top,
                block_width as u16,
                content_height as u16,
            ),
            buf,
        );
    }
}
