//! Minimal Syndrid execution-mode selection.

use super::ChatWidget;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::legacy_core::ExecutionModeSelection;
use crate::legacy_core::SessionExecutionStateError;
use crate::legacy_core::SessionPolicySource;
use crate::render::renderable::ColumnRenderable;
use ratatui::style::Stylize;
use ratatui::text::Line;

const CUSTOM_UNAVAILABLE_REASON: &str =
    "Custom is unavailable until a valid custom policy is configured.";

#[derive(Clone)]
struct ModeEntry {
    selection: Option<ExecutionModeSelection>,
    name: &'static str,
    description: &'static str,
}

fn mode_entries() -> [ModeEntry; 5] {
    [
        ModeEntry {
            selection: Some(ExecutionModeSelection::Fast),
            name: "Fast",
            description: "prioritizes low latency and minimal orchestration",
        },
        ModeEntry {
            selection: Some(ExecutionModeSelection::Balanced),
            name: "Balanced",
            description: "default balance of quality, latency, and usage",
        },
        ModeEntry {
            selection: Some(ExecutionModeSelection::UsageSaver),
            name: "Usage Saver",
            description: "prioritizes lower provider and tool usage",
        },
        ModeEntry {
            selection: Some(ExecutionModeSelection::Deep),
            name: "Deep",
            description: "uses the most thorough bounded orchestration policy",
        },
        ModeEntry {
            selection: None,
            name: "Custom",
            description: "uses the current explicitly configured custom policy",
        },
    ]
}

pub(crate) fn mode_label(selection: &ExecutionModeSelection) -> &'static str {
    match selection {
        ExecutionModeSelection::Fast => "Fast",
        ExecutionModeSelection::Balanced => "Balanced",
        ExecutionModeSelection::UsageSaver => "Usage Saver",
        ExecutionModeSelection::Deep => "Deep",
        ExecutionModeSelection::Custom(_) => "Custom",
    }
}

pub(crate) fn parse_mode_argument(value: &str) -> Option<ExecutionModeSelection> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fast" => Some(ExecutionModeSelection::Fast),
        "balanced" => Some(ExecutionModeSelection::Balanced),
        "usage-saver" | "usage_saver" | "usagesaver" => Some(ExecutionModeSelection::UsageSaver),
        "deep" => Some(ExecutionModeSelection::Deep),
        "custom" => None,
        _ => None,
    }
}

impl ChatWidget {
    pub(crate) fn open_execution_mode_selector(&mut self) {
        let Some(current) = self
            .execution_policy_state
            .as_ref()
            .and_then(|state| state.selected_mode().ok())
        else {
            self.add_error_message("Execution mode is unavailable in this session.".to_string());
            return;
        };
        let items = mode_entries()
            .into_iter()
            .map(|entry| {
                let selection = entry.selection;
                let actions = selection.clone().map_or_else(Vec::new, |selection| {
                    vec![
                        Box::new(move |tx: &crate::app_event_sender::AppEventSender| {
                            tx.send(crate::app_event::AppEvent::UpdateExecutionMode(
                                selection.clone(),
                            ));
                        }) as crate::bottom_pane::SelectionAction,
                    ]
                });
                let is_custom = selection.is_none();
                SelectionItem {
                    name: entry.name.to_string(),
                    description: Some(entry.description.to_string()),
                    is_current: selection.as_ref().is_some_and(|value| current == *value),
                    is_default: selection
                        .as_ref()
                        .is_some_and(|value| matches!(value, ExecutionModeSelection::Balanced)),
                    is_disabled: is_custom,
                    disabled_reason: is_custom.then_some(CUSTOM_UNAVAILABLE_REASON.to_string()),
                    actions,
                    dismiss_on_select: true,
                    ..Default::default()
                }
            })
            .collect();
        let mut header = ColumnRenderable::new();
        header.push(Line::from("Select execution mode".bold()));
        header.push(Line::from(
            "The selection applies to the next eligible Syndrid run.".dim(),
        ));
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Execution mode".to_string()),
            header: Box::new(header),
            footer_hint: Some(self.bottom_pane.standard_popup_hint_line()),
            items,
            ..Default::default()
        });
    }

    pub(crate) fn apply_execution_mode_argument(&mut self, args: &str) {
        let Some(selection) = parse_mode_argument(args) else {
            self.add_error_message(if args.trim().eq_ignore_ascii_case("custom") {
                CUSTOM_UNAVAILABLE_REASON.to_string()
            } else {
                "Usage: /mode [fast|balanced|usage-saver|deep|custom]".to_string()
            });
            return;
        };
        self.apply_execution_mode_selection(selection);
    }

    pub(crate) fn apply_execution_mode_selection(&mut self, selection: ExecutionModeSelection) {
        let Some(state) = self.execution_policy_state.as_ref() else {
            self.add_error_message("Execution mode is unavailable in this session.".to_string());
            return;
        };
        match state.select_mode(
            selection.clone(),
            SessionPolicySource::ExplicitUserSelection,
        ) {
            Ok(()) => self.add_info_message(
                format!("Execution mode set to {}.", mode_label(&selection)),
                None,
            ),
            Err(error) => self.add_error_message(match error {
                SessionExecutionStateError::PolicyMutationWhileActive => {
                    "Execution mode cannot be changed while a run is active.".to_string()
                }
                SessionExecutionStateError::PolicyUnresolved => {
                    "That execution mode is unavailable.".to_string()
                }
                _ => "Execution mode could not be changed.".to_string(),
            }),
        }
    }
}

#[cfg(test)]
#[path = "execution_mode_tests.rs"]
mod tests;
