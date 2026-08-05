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
use crate::legacy_core::RoutingProfile;
use crate::legacy_core::RoutingRole;
use crate::legacy_core::SessionExecutionStateError;
use crate::legacy_core::SessionPolicySource;
use crate::orchestration_profile::OrchestrationProfileSelection;
use crate::orchestration_setup::OrchestrationSetupReadiness;
use crate::pool_setup::PoolSetupSnapshot;
use crate::provider_setup::ProviderSetupItem;
use crate::provider_setup::ProviderSetupSnapshot;
use crate::render::renderable::ColumnRenderable;
use crate::routing_role_setup::IdentitySourceChoice;
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
            on_cancel: Some(Box::new(|tx| {
                tx.send(crate::app_event::AppEvent::CancelOrchestrationSetup)
            })),
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
        self.orchestration_setup_routing_candidate = None;
        self.orchestration_setup_identity_source = None;
        self.orchestration_setup_role = RoutingRole::Planner;
    }

    pub(crate) fn set_orchestration_setup_routing_candidate(&mut self, profile: RoutingProfile) {
        self.orchestration_setup_routing_candidate = Some(profile);
    }

    pub(crate) fn orchestration_setup_routing_candidate(&self) -> Option<RoutingProfile> {
        self.orchestration_setup_routing_candidate.clone()
    }

    pub(crate) fn set_orchestration_setup_role(&mut self, role: RoutingRole) {
        self.orchestration_setup_role = role;
    }

    pub(crate) fn update_orchestration_setup_connection(
        &mut self,
        role: RoutingRole,
        connection_id: String,
        provider_id: String,
    ) {
        let Some(profile) = self.orchestration_setup_routing_candidate.as_mut() else {
            return;
        };
        let assignment = profile.assignments.get(&role).cloned().unwrap_or(
            crate::legacy_core::RoutingAssignment {
                connection_id: String::new(),
                provider_id: String::new(),
                model_id: String::new(),
                enabled: true,
                label: None,
                pool_id: None,
            },
        );
        let model_id = (assignment.provider_id == provider_id
            && assignment.connection_id == connection_id)
            .then_some(assignment.model_id)
            .unwrap_or_default();
        let replacement = crate::legacy_core::RoutingAssignment {
            connection_id,
            provider_id,
            model_id,
            enabled: true,
            label: None,
            pool_id: None,
        };
        self.orchestration_setup_identity_source = Some((role, IdentitySourceChoice::Direct));
        if profile.assignments.contains_key(&role) {
            let _ = profile.replace_assignment(role, replacement);
        } else {
            profile.assignments.insert(role, replacement);
        }
    }

    pub(crate) fn update_orchestration_setup_identity_source(
        &mut self,
        role: RoutingRole,
        source: IdentitySourceChoice,
    ) {
        let Some(profile) = self.orchestration_setup_routing_candidate.as_mut() else {
            return;
        };
        let result = crate::routing_role_setup::set_identity_source(profile, role, source);
        if let Err(error) = result {
            self.add_error_message(error);
        } else {
            self.orchestration_setup_identity_source = Some((role, source));
        }
    }

    pub(crate) fn update_orchestration_setup_pool(
        &mut self,
        role: RoutingRole,
        pool_id: crate::legacy_core::PoolId,
    ) {
        let Some(profile) = self.orchestration_setup_routing_candidate.as_mut() else {
            return;
        };
        let result = crate::routing_role_setup::set_pool_selection(profile, role, pool_id);
        if let Err(error) = result {
            self.add_error_message(error);
        } else {
            self.orchestration_setup_identity_source =
                Some((role, IdentitySourceChoice::NamedPool));
        }
    }

    pub(crate) fn update_orchestration_setup_model(&mut self, role: RoutingRole, model_id: String) {
        let Some(profile) = self.orchestration_setup_routing_candidate.as_mut() else {
            return;
        };
        let Some(assignment) = profile.assignments.get_mut(&role) else {
            return;
        };
        assignment.model_id = model_id;
    }

    pub(crate) fn open_orchestration_setup(
        &mut self,
        readiness: OrchestrationSetupReadiness,
        provider_setup: ProviderSetupSnapshot,
        pool_snapshot: crate::pool_setup::PoolSetupSnapshot,
    ) {
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
        let mut actions = vec![
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
        ];
        if self.orchestration_setup_routing_candidate.is_some() {
            actions.push(SelectionItem {
                name: "Use saved routing for this session".to_string(),
                description: Some(
                    "Clear the session override without changing the saved routing profile."
                        .to_string(),
                ),
                actions: vec![Box::new(|tx: &crate::app_event_sender::AppEventSender| {
                    tx.send(crate::app_event::AppEvent::ClearSessionRoutingOverride);
                }) as crate::bottom_pane::SelectionAction],
                dismiss_on_select: true,
                ..Default::default()
            });
        }
        actions.push(SelectionItem {
            name: "Cancel".to_string(),
            description: Some("Leave the current session and saved routing unchanged.".to_string()),
            actions: vec![Box::new(|tx: &crate::app_event_sender::AppEventSender| {
                tx.send(crate::app_event::AppEvent::CancelOrchestrationSetup);
            }) as crate::bottom_pane::SelectionAction],
            dismiss_on_select: true,
            ..Default::default()
        });
        let selected_role = self.orchestration_setup_role;
        let provider_items = provider_setup_items(&provider_setup.providers);
        let account_items =
            selectable_provider_setup_items(&provider_setup.accounts, selected_role, "codex");
        let connection_items = selectable_provider_setup_items(
            &provider_setup.connections,
            selected_role,
            "connection",
        );
        let role_items = routing_role_items(
            self.orchestration_setup_routing_candidate.as_ref(),
            selected_role,
            &pool_snapshot,
        );
        let identity_source_items = crate::routing_role_setup::identity_source_items(
            self.orchestration_setup_routing_candidate.as_ref(),
            selected_role,
            self.orchestration_setup_identity_source
                .filter(|(role, _)| *role == selected_role)
                .map(|(_, source)| source),
        );
        let role_pool_items = crate::routing_role_setup::pool_selection_items(
            self.orchestration_setup_routing_candidate.as_ref(),
            selected_role,
            &pool_snapshot,
        );
        let model_items = routing_model_items(
            self.orchestration_setup_routing_candidate.as_ref(),
            selected_role,
            &provider_setup,
        );
        let mut providers_header = ColumnRenderable::new();
        providers_header.push(Line::from("Configured production providers.".dim()));
        providers_header.push(Line::from(
            "Read-only inspection; no provider changes in this version.".dim(),
        ));
        let mut accounts_header = ColumnRenderable::new();
        accounts_header.push(Line::from(
            "Existing authenticated account metadata only.".dim(),
        ));
        accounts_header.push(Line::from(
            "Credentials and tokens are never displayed.".dim(),
        ));
        let mut connections_header = ColumnRenderable::new();
        connections_header.push(Line::from("Existing provider connections.".dim()));
        connections_header.push(Line::from(
            "Read-only inspection; connection changes are deferred.".dim(),
        ));
        let mut roles_header = ColumnRenderable::new();
        roles_header.push(Line::from(
            "Choose the role edited by Accounts, Connections, and Models.".dim(),
        ));
        roles_header.push(Line::from(
            "Bindings are published only by an explicit Apply action.".dim(),
        ));
        let mut identity_source_header = ColumnRenderable::new();
        identity_source_header.push(Line::from(
            "Choose one identity source for this role.".dim(),
        ));
        identity_source_header.push(Line::from(
            "Pool members and explicit selection are managed in Setup → Pools.".dim(),
        ));
        let mut role_pools_header = ColumnRenderable::new();
        role_pools_header.push(Line::from(
            "Only pools compatible with the role provider are shown.".dim(),
        ));
        role_pools_header.push(Line::from(
            "Pool policy controls selection timing; no retry or fallback.".dim(),
        ));
        let mut models_header = ColumnRenderable::new();
        models_header.push(Line::from(
            "Choose only models already present in canonical metadata.".dim(),
        ));
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
        let (pool_header, managed_pool_items) = crate::pool_setup::pool_tab(&pool_snapshot);
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Syndrid Setup".to_string()),
            subtitle: Some(format!(
                "Current: {} / {} · Saved routing: {} · Candidate: {} / {}",
                current
                    .as_ref()
                    .map(|selection| strategy_label(selection.strategy))
                    .unwrap_or("Unavailable"),
                current
                    .as_ref()
                    .map(|selection| mode_label(&selection.preset))
                    .unwrap_or("Unavailable"),
                provider_setup
                    .saved_profile_id
                    .as_deref()
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
                    id: crate::pool_setup::POOLS_TAB_ID.to_string(),
                    label: "Pools".to_string(),
                    header: pool_header,
                    items: managed_pool_items,
                },
                SelectionTab {
                    id: "providers".to_string(),
                    label: "Providers".to_string(),
                    header: Box::new(providers_header),
                    items: provider_items,
                },
                SelectionTab {
                    id: "accounts".to_string(),
                    label: "Accounts".to_string(),
                    header: Box::new(accounts_header),
                    items: account_items,
                },
                SelectionTab {
                    id: "connections".to_string(),
                    label: "Connections".to_string(),
                    header: Box::new(connections_header),
                    items: connection_items,
                },
                SelectionTab {
                    id: "roles".to_string(),
                    label: "Roles".to_string(),
                    header: Box::new(roles_header),
                    items: role_items,
                },
                SelectionTab {
                    id: "identity-source".to_string(),
                    label: "Identity source".to_string(),
                    header: Box::new(identity_source_header),
                    items: identity_source_items,
                },
                SelectionTab {
                    id: "role-pools".to_string(),
                    label: "Role pool".to_string(),
                    header: Box::new(role_pools_header),
                    items: role_pool_items,
                },
                SelectionTab {
                    id: "models".to_string(),
                    label: "Models".to_string(),
                    header: Box::new(models_header),
                    items: model_items,
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

fn provider_setup_items(items: &[ProviderSetupItem]) -> Vec<SelectionItem> {
    items
        .iter()
        .map(|item| SelectionItem {
            name: format!("{} — {}", item.name, item.readiness.label()),
            description: Some(item.detail.clone()),
            is_disabled: true,
            disabled_reason: item.readiness.reason().map(str::to_string),
            ..Default::default()
        })
        .collect()
}

fn selectable_provider_setup_items(
    items: &[ProviderSetupItem],
    role: RoutingRole,
    kind: &str,
) -> Vec<SelectionItem> {
    items
        .iter()
        .map(|item| {
            let selectable = item.readiness
                == crate::orchestration_setup::SetupReadinessState::Ready
                && item.id.is_some()
                && ((kind == "codex" && item.provider_id.as_deref() == Some("codex"))
                    || (kind == "connection" && item.provider_id.as_deref() == Some("omniroute")));
            let actions = if selectable {
                match (item.id.clone(), item.provider_id.clone()) {
                    (Some(connection_id), Some(provider_id)) => {
                        vec![
                            Box::new(move |tx: &crate::app_event_sender::AppEventSender| {
                                tx.send(
                                crate::app_event::AppEvent::UpdateOrchestrationSetupConnection {
                                    role,
                                    connection_id: connection_id.clone(),
                                    provider_id: provider_id.clone(),
                                },
                            );
                            }) as crate::bottom_pane::SelectionAction,
                        ]
                    }
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };
            SelectionItem {
                name: format!("{} — {}", item.name, item.readiness.label()),
                description: Some(if selectable {
                    format!("{} · edits {role} candidate", item.detail)
                } else {
                    item.readiness.reason().unwrap_or(&item.detail).to_string()
                }),
                is_disabled: !selectable,
                disabled_reason: (!selectable).then(|| {
                    item.readiness
                        .reason()
                        .unwrap_or("This configuration cannot be selected.")
                        .to_string()
                }),
                actions,
                dismiss_on_select: false,
                ..Default::default()
            }
        })
        .collect()
}

fn routing_role_items(
    profile: Option<&RoutingProfile>,
    selected_role: RoutingRole,
    pool_snapshot: &PoolSetupSnapshot,
) -> Vec<SelectionItem> {
    [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ]
    .into_iter()
    .map(|role| {
        let detail = profile
            .and_then(|profile| profile.assignments.get(&role))
            .map(|assignment| {
                let identity = assignment
                    .pool_id
                    .as_ref()
                    .map(|pool_id| {
                        let policy = pool_snapshot
                            .summaries
                            .iter()
                            .find(|summary| summary.id == *pool_id)
                            .map(|summary| {
                                if summary.is_round_robin {
                                    "Round robin".to_string()
                                } else {
                                    format!("Explicit member {}", summary.selected)
                                }
                            })
                            .unwrap_or_else(|| "Needs attention".to_string());
                        let status = pool_snapshot
                            .runtime_statuses
                            .get(pool_id)
                            .copied()
                            .map(crate::pool_setup::installed_pool_status_label)
                            .unwrap_or("Not currently routed");
                        format!("Pool · {pool_id} · {policy} · Runtime {status}")
                    })
                    .unwrap_or_else(|| format!("Direct · {}", assignment.connection_id));
                format!(
                    "{} / {} / {}",
                    assignment.provider_id, identity, assignment.model_id
                )
            })
            .unwrap_or_else(|| "missing binding".to_string());
        SelectionItem {
            name: format!(
                "{} {}",
                role,
                (role == selected_role).then_some("· editing").unwrap_or("")
            ),
            description: Some(detail),
            is_current: role == selected_role,
            actions: vec![
                Box::new(move |tx: &crate::app_event_sender::AppEventSender| {
                    tx.send(crate::app_event::AppEvent::UpdateOrchestrationSetupRole(
                        role,
                    ));
                }) as crate::bottom_pane::SelectionAction,
            ],
            dismiss_on_select: false,
            ..Default::default()
        }
    })
    .collect()
}

fn routing_model_items(
    profile: Option<&RoutingProfile>,
    role: RoutingRole,
    provider_setup: &ProviderSetupSnapshot,
) -> Vec<SelectionItem> {
    let Some(assignment) = profile.and_then(|profile| profile.assignments.get(&role)) else {
        return vec![SelectionItem {
            name: "No role binding".to_string(),
            description: Some("Select a role binding before choosing a model.".to_string()),
            is_disabled: true,
            ..Default::default()
        }];
    };
    let models = provider_setup
        .connections
        .iter()
        .find(|item| item.id.as_deref() == Some(assignment.connection_id.as_str()))
        .map(|item| item.models.clone())
        .unwrap_or_default();
    if models.is_empty() {
        return vec![SelectionItem {
            name: assignment.model_id.clone(),
            description: Some(
                "Model metadata is not separately selectable for this binding.".to_string(),
            ),
            is_current: true,
            is_disabled: true,
            ..Default::default()
        }];
    }
    models
        .into_iter()
        .map(|model_id| {
            let selected_model = model_id.clone();
            SelectionItem {
                name: model_id,
                is_current: selected_model == assignment.model_id,
                actions: vec![
                    Box::new(move |tx: &crate::app_event_sender::AppEventSender| {
                        tx.send(crate::app_event::AppEvent::UpdateOrchestrationSetupModel {
                            role,
                            model_id: selected_model.clone(),
                        });
                    }) as crate::bottom_pane::SelectionAction,
                ],
                dismiss_on_select: false,
                ..Default::default()
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "execution_mode_tests.rs"]
mod tests;
