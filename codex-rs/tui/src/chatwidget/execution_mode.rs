//! Minimal Syndrid execution-mode selection.

use super::ChatWidget;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionTab;
use crate::bottom_pane::SelectionViewParams;
use crate::legacy_core::ExecutionModeSelection;
use crate::legacy_core::OrchestrationMode;
use crate::legacy_core::OrchestrationStrategyAvailability;
use crate::legacy_core::OrchestrationStrategyUnavailableReason;
use crate::legacy_core::ResolvedOrchestrationPolicy;
use crate::legacy_core::SessionExecutionStateError;
use crate::legacy_core::SessionPolicySource;
use crate::render::renderable::ColumnRenderable;
use ratatui::style::Stylize;
use ratatui::text::Line;

const CUSTOM_UNAVAILABLE_REASON: &str =
    "Custom is unavailable until a valid custom policy is configured.";

const STRATEGY_TAB_ID: &str = "strategy";
const PRESET_TAB_ID: &str = "preset";

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

#[derive(Clone, Copy)]
struct StrategyEntry {
    strategy: OrchestrationMode,
    name: &'static str,
    description: &'static str,
}

fn strategy_entries() -> [StrategyEntry; 5] {
    [
        StrategyEntry {
            strategy: OrchestrationMode::Single,
            name: "Single",
            description: "use the existing Codex-compatible path",
        },
        StrategyEntry {
            strategy: OrchestrationMode::Manual,
            name: "Manual",
            description: "use the exact configured Syndrid role bindings",
        },
        StrategyEntry {
            strategy: OrchestrationMode::Recommended,
            name: "Recommended",
            description: "select a workflow from a trusted recommendation",
        },
        StrategyEntry {
            strategy: OrchestrationMode::Automatic,
            name: "Automatic",
            description: "select a workflow from a trusted automatic selector",
        },
        StrategyEntry {
            strategy: OrchestrationMode::Adaptive,
            name: "Adaptive",
            description: "adapt workflow to trusted usage and quota authorities",
        },
    ]
}

fn strategy_label(strategy: OrchestrationMode) -> &'static str {
    match strategy {
        OrchestrationMode::Single => "Single",
        OrchestrationMode::Manual => "Manual",
        OrchestrationMode::Recommended => "Recommended",
        OrchestrationMode::Automatic => "Automatic",
        OrchestrationMode::Adaptive => "Adaptive",
    }
}

fn unavailable_reason(reason: OrchestrationStrategyUnavailableReason) -> String {
    let authority = match reason {
        OrchestrationStrategyUnavailableReason::AutomaticSelectorUnavailable => {
            "automatic workflow selection is not implemented yet"
        }
        OrchestrationStrategyUnavailableReason::RecommendationAuthorityUnavailable => {
            "recommendation authority is not implemented yet"
        }
        OrchestrationStrategyUnavailableReason::AdaptiveUsageAuthorityUnavailable => {
            "account, quota, and usage authorities are not implemented yet"
        }
    };
    let strategy = match reason {
        OrchestrationStrategyUnavailableReason::AutomaticSelectorUnavailable => {
            OrchestrationMode::Automatic
        }
        OrchestrationStrategyUnavailableReason::RecommendationAuthorityUnavailable => {
            OrchestrationMode::Recommended
        }
        OrchestrationStrategyUnavailableReason::AdaptiveUsageAuthorityUnavailable => {
            OrchestrationMode::Adaptive
        }
    };
    format!(
        "{} is unavailable because {authority}.",
        strategy_label(strategy)
    )
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
        let preset_items = mode_entries()
            .into_iter()
            .map(|entry| {
                let selection = entry.selection.clone().or_else(|| {
                    matches!(current, ExecutionModeSelection::Custom(_)).then_some(current.clone())
                });
                let actions = selection.clone().map_or_else(Vec::new, |selection| {
                    vec![
                        Box::new(move |tx: &crate::app_event_sender::AppEventSender| {
                            tx.send(crate::app_event::AppEvent::UpdateExecutionMode(
                                selection.clone(),
                            ));
                        }) as crate::bottom_pane::SelectionAction,
                    ]
                });
                let is_custom = entry.selection.is_none();
                SelectionItem {
                    name: entry.name.to_string(),
                    description: Some(entry.description.to_string()),
                    is_current: selection.as_ref().is_some_and(|value| current == *value),
                    is_default: selection
                        .as_ref()
                        .is_some_and(|value| matches!(value, ExecutionModeSelection::Balanced)),
                    is_disabled: is_custom && !matches!(current, ExecutionModeSelection::Custom(_)),
                    disabled_reason: (is_custom
                        && !matches!(current, ExecutionModeSelection::Custom(_)))
                    .then_some(CUSTOM_UNAVAILABLE_REASON.to_string()),
                    actions,
                    dismiss_on_select: true,
                    ..Default::default()
                }
            })
            .collect();
        let strategy = self
            .execution_policy_state
            .as_ref()
            .and_then(|state| state.strategy().ok())
            .unwrap_or(OrchestrationMode::Single);
        let preset = current.clone();
        let strategy_items = strategy_entries()
            .into_iter()
            .map(|entry| {
                let availability = ResolvedOrchestrationPolicy::resolve(
                    entry.strategy,
                    preset.clone(),
                )
                .map(|policy| policy.availability())
                .unwrap_or(OrchestrationStrategyAvailability::Unavailable(
                    OrchestrationStrategyUnavailableReason::RecommendationAuthorityUnavailable,
                ));
                let unavailable = match availability {
                    OrchestrationStrategyAvailability::Available => None,
                    OrchestrationStrategyAvailability::Unavailable(reason) => {
                        Some(unavailable_reason(reason))
                    }
                };
                let selected = entry.strategy;
                let actions = unavailable
                    .is_none()
                    .then(|| {
                        vec![
                            Box::new(move |tx: &crate::app_event_sender::AppEventSender| {
                                tx.send(crate::app_event::AppEvent::UpdateOrchestrationStrategy(
                                    selected,
                                ));
                            }) as crate::bottom_pane::SelectionAction,
                        ]
                    })
                    .unwrap_or_default();
                SelectionItem {
                    name: entry.name.to_string(),
                    description: Some(entry.description.to_string()),
                    is_current: selected == strategy,
                    is_disabled: unavailable.is_some(),
                    disabled_reason: unavailable,
                    actions,
                    dismiss_on_select: true,
                    ..Default::default()
                }
            })
            .collect();
        let mut strategy_header = ColumnRenderable::new();
        strategy_header.push(Line::from(
            "Choose how the next turn is orchestrated.".dim(),
        ));
        let mut preset_header = ColumnRenderable::new();
        preset_header.push(Line::from("Preset applies to orchestrated turns.".dim()));
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Orchestration".to_string()),
            subtitle: Some(format!(
                "Strategy: {} · Preset: {}",
                strategy_label(strategy),
                mode_label(&current)
            )),
            footer_hint: Some(self.bottom_pane.standard_popup_hint_line()),
            tabs: vec![
                SelectionTab {
                    id: STRATEGY_TAB_ID.to_string(),
                    label: "Strategy".to_string(),
                    header: Box::new(strategy_header),
                    items: strategy_items,
                },
                SelectionTab {
                    id: PRESET_TAB_ID.to_string(),
                    label: "Preset".to_string(),
                    header: Box::new(preset_header),
                    items: preset_items,
                },
            ],
            initial_tab_id: Some(STRATEGY_TAB_ID.to_string()),
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

    pub(crate) fn apply_execution_mode_selection(
        &mut self,
        selection: ExecutionModeSelection,
    ) -> bool {
        let Some(state) = self.execution_policy_state.as_ref() else {
            self.add_error_message("Execution mode is unavailable in this session.".to_string());
            return false;
        };
        match state.select_mode(
            selection.clone(),
            SessionPolicySource::ExplicitUserSelection,
        ) {
            Ok(()) => true,
            Err(error) => {
                self.add_error_message(match error {
                    SessionExecutionStateError::PolicyMutationWhileActive => {
                        "Execution mode cannot be changed while a run is active.".to_string()
                    }
                    SessionExecutionStateError::PolicyUnresolved => {
                        "That execution mode is unavailable.".to_string()
                    }
                    _ => "Execution mode could not be changed.".to_string(),
                });
                false
            }
        }
    }

    pub(crate) fn apply_orchestration_strategy_selection(
        &mut self,
        strategy: OrchestrationMode,
    ) -> bool {
        let Some(state) = self.execution_policy_state.as_ref() else {
            self.add_error_message(
                "Orchestration strategy is unavailable in this session.".to_string(),
            );
            return false;
        };
        let Ok(preset) = state.selected_mode() else {
            self.add_error_message(
                "Orchestration strategy is unavailable in this session.".to_string(),
            );
            return false;
        };
        let availability = match ResolvedOrchestrationPolicy::resolve(strategy, preset) {
            Ok(policy) => policy.availability(),
            Err(_) => {
                self.add_error_message("Orchestration strategy could not be resolved.".to_string());
                return false;
            }
        };
        if let OrchestrationStrategyAvailability::Unavailable(reason) = availability {
            self.add_error_message(unavailable_reason(reason));
            return false;
        }
        match state.select_strategy(strategy) {
            Ok(()) => true,
            Err(SessionExecutionStateError::PolicyMutationWhileActive) => {
                self.add_error_message(
                    "Orchestration strategy cannot be changed while a run is active.".to_string(),
                );
                false
            }
            Err(_) => {
                self.add_error_message("Orchestration strategy could not be changed.".to_string());
                false
            }
        }
    }
}

#[cfg(test)]
#[path = "execution_mode_tests.rs"]
mod tests;
