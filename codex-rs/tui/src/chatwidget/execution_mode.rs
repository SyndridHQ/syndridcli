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
use crate::orchestration_profile::OrchestrationProfileSelection;
use crate::orchestration_setup::OrchestrationSetupReadiness;
use crate::render::renderable::ColumnRenderable;
use ratatui::style::Stylize;
use ratatui::text::Line;

const CUSTOM_UNAVAILABLE_REASON: &str =
    "Custom is unavailable until a valid custom policy is configured.";

const STRATEGY_TAB_ID: &str = "strategy";
const PRESET_TAB_ID: &str = "preset";
const PROFILE_TAB_ID: &str = "saved-default";

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
    pub(crate) fn set_orchestration_profile_store(
        &mut self,
        store: std::sync::Arc<crate::orchestration_profile::OrchestrationProfileStore>,
    ) {
        self.orchestration_profile_store = Some(store);
    }

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
        let saved_default = self
            .orchestration_profile_store
            .as_ref()
            .map(|store| store.saved_default_label())
            .unwrap_or_else(|| "Unavailable".to_string());
        let mut profile_header = ColumnRenderable::new();
        profile_header.push(Line::from(
            format!(
                "Current session: {} / {}",
                strategy_label(strategy),
                mode_label(&current)
            )
            .dim(),
        ));
        profile_header.push(Line::from(format!("Saved default: {saved_default}").dim()));
        profile_header.push(Line::from(
            "Changes are session-local until explicitly saved.".dim(),
        ));
        let save_available = self.orchestration_profile_store.is_some();
        let save_items = vec![SelectionItem {
            name: "Save current selection as local default".to_string(),
            description: Some(
                "Applies to future local sessions; the current session and runtime stay unchanged."
                    .to_string(),
            ),
            is_disabled: !save_available,
            disabled_reason: (!save_available).then_some(
                "Local orchestration defaults are unavailable in this session.".to_string(),
            ),
            actions: save_available
                .then(|| {
                    vec![Box::new(|tx: &crate::app_event_sender::AppEventSender| {
                        tx.send(crate::app_event::AppEvent::SaveOrchestrationProfile);
                    })
                        as crate::bottom_pane::SelectionAction]
                })
                .unwrap_or_default(),
            dismiss_on_select: true,
            ..Default::default()
        }];
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
                SelectionTab {
                    id: PROFILE_TAB_ID.to_string(),
                    label: "Saved default".to_string(),
                    header: Box::new(profile_header),
                    items: save_items,
                },
            ],
            initial_tab_id: Some(STRATEGY_TAB_ID.to_string()),
            ..Default::default()
        });
    }

    pub(crate) fn begin_orchestration_setup(&mut self) -> Option<OrchestrationProfileSelection> {
        let state = self.execution_policy_state.as_ref()?;
        let candidate =
            self.orchestration_setup_candidate
                .clone()
                .unwrap_or(OrchestrationProfileSelection {
                    strategy: state.strategy().ok()?,
                    preset: state.selected_mode().ok()?,
                });
        self.orchestration_setup_candidate = Some(candidate.clone());
        Some(candidate)
    }

    pub(crate) fn orchestration_setup_candidate(&self) -> Option<OrchestrationProfileSelection> {
        self.orchestration_setup_candidate.clone()
    }

    pub(crate) fn set_orchestration_setup_candidate(
        &mut self,
        candidate: OrchestrationProfileSelection,
    ) {
        self.orchestration_setup_candidate = Some(candidate);
    }

    pub(crate) fn clear_orchestration_setup_candidate(&mut self) {
        self.orchestration_setup_candidate = None;
    }

    pub(crate) fn open_orchestration_setup(&mut self, readiness: OrchestrationSetupReadiness) {
        let Some(candidate) = self.orchestration_setup_candidate.clone() else {
            return;
        };
        let current = self.execution_policy_state.as_ref().and_then(|state| {
            Some(OrchestrationProfileSelection {
                strategy: state.strategy().ok()?,
                preset: state.selected_mode().ok()?,
            })
        });
        let strategy_items = strategy_entries()
            .into_iter()
            .map(|entry| {
                let selected = entry.strategy;
                let policy =
                    ResolvedOrchestrationPolicy::resolve(selected, candidate.preset.clone());
                let disabled_reason = policy.ok().and_then(|policy| match policy.availability() {
                    OrchestrationStrategyAvailability::Available => None,
                    OrchestrationStrategyAvailability::Unavailable(reason) => {
                        Some(unavailable_reason(reason))
                    }
                });
                let actions = disabled_reason.is_none().then(|| {
                    vec![
                        Box::new(move |tx: &crate::app_event_sender::AppEventSender| {
                            tx.send(
                                crate::app_event::AppEvent::UpdateOrchestrationSetupStrategy(
                                    selected,
                                ),
                            );
                        }) as crate::bottom_pane::SelectionAction,
                    ]
                });
                SelectionItem {
                    name: entry.name.to_string(),
                    description: Some(entry.description.to_string()),
                    is_current: selected == candidate.strategy,
                    is_disabled: disabled_reason.is_some(),
                    disabled_reason,
                    actions: actions.unwrap_or_default(),
                    dismiss_on_select: false,
                    ..Default::default()
                }
            })
            .collect();
        let preset_items = mode_entries()
            .into_iter()
            .map(|entry| {
                let selection = entry.selection.clone().or_else(|| {
                    matches!(candidate.preset, ExecutionModeSelection::Custom(_))
                        .then_some(candidate.preset.clone())
                });
                let is_disabled = selection.is_none();
                let actions = selection.clone().map_or_else(Vec::new, |selection| {
                    vec![
                        Box::new(move |tx: &crate::app_event_sender::AppEventSender| {
                            tx.send(crate::app_event::AppEvent::UpdateOrchestrationSetupPreset(
                                selection.clone(),
                            ));
                        }) as crate::bottom_pane::SelectionAction,
                    ]
                });
                SelectionItem {
                    name: entry.name.to_string(),
                    description: Some(entry.description.to_string()),
                    is_current: selection
                        .as_ref()
                        .is_some_and(|value| *value == candidate.preset),
                    is_default: selection
                        .as_ref()
                        .is_some_and(|value| matches!(value, ExecutionModeSelection::Balanced)),
                    is_disabled,
                    disabled_reason: is_disabled.then_some(CUSTOM_UNAVAILABLE_REASON.to_string()),
                    actions,
                    dismiss_on_select: false,
                    ..Default::default()
                }
            })
            .collect();
        let readiness_items = [
            ("Strategy", &readiness.strategy),
            ("Preset", &readiness.preset),
            ("Routing", &readiness.routing),
            ("Required roles", &readiness.required_roles),
            ("Runtime assembly", &readiness.runtime_assembly),
        ]
        .into_iter()
        .map(|(name, state)| SelectionItem {
            name: format!("{name:<16} {}", state.label()),
            description: state.reason().map(str::to_string),
            is_disabled: true,
            disabled_reason: state.reason().map(str::to_string),
            ..Default::default()
        })
        .collect();
        let actions = vec![
            SelectionItem {
                name: "Apply for this session".to_string(),
                description: Some("Apply without changing the saved local default.".to_string()),
                is_disabled: !readiness.can_apply(),
                disabled_reason: (!readiness.can_apply())
                    .then_some("Resolve the readiness items before applying.".to_string()),
                actions: vec![Box::new(|tx: &crate::app_event_sender::AppEventSender| {
                    tx.send(crate::app_event::AppEvent::ApplyOrchestrationSetup { save: false });
                }) as crate::bottom_pane::SelectionAction],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Apply and save as local default".to_string(),
                description: Some("Apply now and use it for future local sessions.".to_string()),
                is_disabled: !readiness.can_apply(),
                disabled_reason: (!readiness.can_apply())
                    .then_some("Resolve the readiness items before applying.".to_string()),
                actions: vec![Box::new(|tx: &crate::app_event_sender::AppEventSender| {
                    tx.send(crate::app_event::AppEvent::ApplyOrchestrationSetup { save: true });
                }) as crate::bottom_pane::SelectionAction],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Cancel".to_string(),
                description: Some(
                    "Leave the current session and saved default unchanged.".to_string(),
                ),
                actions: vec![Box::new(|tx: &crate::app_event_sender::AppEventSender| {
                    tx.send(crate::app_event::AppEvent::CancelOrchestrationSetup);
                }) as crate::bottom_pane::SelectionAction],
                dismiss_on_select: true,
                ..Default::default()
            },
        ];
        let mut strategy_header = ColumnRenderable::new();
        strategy_header.push(Line::from("Choose the execution strategy.".dim()));
        let mut preset_header = ColumnRenderable::new();
        preset_header.push(Line::from("Choose the execution preset.".dim()));
        let mut readiness_header = ColumnRenderable::new();
        readiness_header.push(Line::from(
            "Readiness is side-effect-free; providers and tools are not invoked.".dim(),
        ));
        let mut action_header = ColumnRenderable::new();
        action_header.push(Line::from(
            "Apply publishes the candidate only after validation.".dim(),
        ));
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Syndrid Setup".to_string()),
            subtitle: Some(format!(
                "Current: {} / {} · Candidate: {} / {}",
                current
                    .as_ref()
                    .map(|selection| strategy_label(selection.strategy))
                    .unwrap_or("Unavailable"),
                current
                    .as_ref()
                    .map(|selection| mode_label(&selection.preset))
                    .unwrap_or("Unavailable"),
                strategy_label(candidate.strategy),
                mode_label(&candidate.preset)
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
                SelectionTab {
                    id: "readiness".to_string(),
                    label: "Readiness".to_string(),
                    header: Box::new(readiness_header),
                    items: readiness_items,
                },
                SelectionTab {
                    id: "actions".to_string(),
                    label: "Actions".to_string(),
                    header: Box::new(action_header),
                    items: actions,
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
