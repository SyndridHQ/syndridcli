//! Candidate-only `/setup` management for named account pools.

use crate::app_event::AppEvent;
use crate::bottom_pane::ColumnWidthMode;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::bottom_pane::custom_prompt_view::CustomPromptView;
use crate::chatwidget::ChatWidget;
use crate::cooldown_status::TuiProviderCooldownSnapshot;
use crate::cooldown_status::cooldown_label;
use crate::cooldown_status::format_cooldown_duration;
use crate::legacy_core::AccountPoolMember;
use crate::legacy_core::AccountPoolProviderFamily;
use crate::legacy_core::AccountPoolSelectionPolicy;
use crate::legacy_core::AccountPoolTarget;
use crate::legacy_core::NamedAccountPool;
use crate::legacy_core::NamedAccountPoolRegistry;
use crate::legacy_core::PoolId;
use crate::legacy_core::PoolMemberId;
use crate::legacy_core::PoolMemberReadiness;
use crate::legacy_core::PoolReadiness;
use crate::legacy_core::RoutingProfile;
use crate::legacy_core::RoutingRole;
use crate::pool_authority::TuiPoolAuthority;
use crate::render::renderable::ColumnRenderable;
use crate::render::renderable::Renderable;
use ratatui::style::Stylize;
use ratatui::text::Line;
use std::collections::BTreeMap;

pub(crate) const POOLS_TAB_ID: &str = "pools";
pub(crate) const POOL_MANAGEMENT_VIEW_ID: &str = "syndrid-account-pools";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstalledPoolRoleSnapshot {
    pub(crate) pool_id: PoolId,
    pub(crate) fingerprint: crate::legacy_core::PoolRotationFingerprint,
    pub(crate) policy: AccountPoolSelectionPolicy,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct InstalledPoolRoutingSnapshot {
    pub(crate) generation: u64,
    pub(crate) roles: BTreeMap<RoutingRole, InstalledPoolRoleSnapshot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstalledPoolStatus {
    Current,
    ReapplyRouting,
    NotCurrentlyRouted,
    InstalledSnapshotMissingFromRegistry,
}

impl InstalledPoolRoutingSnapshot {
    pub(crate) fn from_captured_routing(
        profile: &RoutingProfile,
        pools: &NamedAccountPoolRegistry,
        generation: u64,
    ) -> Self {
        let roles = profile
            .assignments
            .iter()
            .filter_map(|(role, assignment)| {
                let pool_id = assignment.pool_id.as_ref()?;
                let pool = pools.get(pool_id)?;
                Some((
                    *role,
                    InstalledPoolRoleSnapshot {
                        pool_id: pool_id.clone(),
                        fingerprint: pool.rotation_fingerprint(),
                        policy: pool.selection_policy.clone(),
                    },
                ))
            })
            .collect();
        Self { generation, roles }
    }

    pub(crate) fn status_for_pool(
        &self,
        pool_id: &PoolId,
        saved_registry: &NamedAccountPoolRegistry,
    ) -> InstalledPoolStatus {
        let installed = self
            .roles
            .values()
            .filter(|role| role.pool_id == *pool_id)
            .collect::<Vec<_>>();
        if installed.is_empty() {
            return InstalledPoolStatus::NotCurrentlyRouted;
        }
        let Some(saved_pool) = saved_registry.get(pool_id) else {
            return InstalledPoolStatus::InstalledSnapshotMissingFromRegistry;
        };
        let saved_fingerprint = saved_pool.rotation_fingerprint();
        if installed
            .iter()
            .all(|role| role.fingerprint == saved_fingerprint)
        {
            InstalledPoolStatus::Current
        } else {
            InstalledPoolStatus::ReapplyRouting
        }
    }
}

pub(crate) fn installed_pool_status_label(status: InstalledPoolStatus) -> &'static str {
    match status {
        InstalledPoolStatus::Current => "Current",
        InstalledPoolStatus::ReapplyRouting => "Reapply routing",
        InstalledPoolStatus::NotCurrentlyRouted => "Not currently routed",
        InstalledPoolStatus::InstalledSnapshotMissingFromRegistry => {
            "Installed snapshot uses missing pool"
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PoolSetupSnapshot {
    pub(crate) summaries: Vec<PoolSummary>,
    pub(crate) member_statuses: BTreeMap<(PoolId, PoolMemberId), PoolMemberReadiness>,
    pub(crate) member_labels: BTreeMap<(PoolId, PoolMemberId), String>,
    pub(crate) runtime_statuses: BTreeMap<PoolId, InstalledPoolStatus>,
    pub(crate) cooldowns: TuiProviderCooldownSnapshot,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PoolSummary {
    pub(crate) id: PoolId,
    pub(crate) display_name: String,
    pub(crate) provider: AccountPoolProviderFamily,
    pub(crate) member_count: usize,
    pub(crate) selected: String,
    pub(crate) is_round_robin: bool,
    pub(crate) readiness: PoolReadiness,
    pub(crate) available_target_count: usize,
    pub(crate) cooling_target_count: usize,
    pub(crate) earliest_recovery: Option<std::time::Duration>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PoolMemberChoice {
    pub(crate) target: AccountPoolTarget,
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) readiness: PoolMemberReadiness,
}

impl PoolSetupSnapshot {
    pub(crate) fn from_registry_with_cooldowns(
        registry: &NamedAccountPoolRegistry,
        accounts: Option<&crate::legacy_core::CodexAccountProfileRegistry>,
        connections: Option<&crate::legacy_core::OmniRouteRegistry>,
        cooldowns: &TuiProviderCooldownSnapshot,
    ) -> Self {
        let empty_accounts = crate::legacy_core::CodexAccountProfileRegistry::default();
        let empty_connections = crate::legacy_core::OmniRouteRegistry::default();
        let accounts = accounts.unwrap_or(&empty_accounts);
        let connections = connections.unwrap_or(&empty_connections);
        let mut readiness = registry.readiness(accounts, connections);
        let mut member_statuses = BTreeMap::new();
        let mut member_labels = BTreeMap::new();
        for pool in registry.pools() {
            if let Ok(statuses) = registry.member_readiness(&pool.id, accounts, connections) {
                for (member_id, status) in statuses {
                    member_statuses.insert((pool.id.clone(), member_id), status);
                }
            }
            for member in &pool.members {
                let label = match &member.target {
                    AccountPoolTarget::NativeCodexAccount(account_id) => accounts
                        .get(account_id)
                        .map(|profile| profile.label.clone())
                        .unwrap_or_else(|| format!("Missing account {account_id}")),
                    AccountPoolTarget::OmniRouteConnection(connection_id) => connections
                        .get(connection_id)
                        .map(|connection| connection.label.clone())
                        .unwrap_or_else(|| format!("Missing connection {connection_id}")),
                };
                member_labels.insert((pool.id.clone(), member.id.clone()), label);
            }
        }
        for pool in registry.pools() {
            if matches!(
                pool.selection_policy,
                AccountPoolSelectionPolicy::RoundRobin
            ) && pool.validate_structure().is_ok()
            {
                let statuses = pool
                    .members
                    .iter()
                    .filter_map(|member| {
                        member_statuses
                            .get(&(pool.id.clone(), member.id.clone()))
                            .copied()
                    })
                    .collect::<Vec<_>>();
                if statuses
                    .iter()
                    .all(|status| *status == PoolMemberReadiness::Ready)
                {
                    readiness.insert(pool.id.clone(), PoolReadiness::Ready);
                } else if let Some(status) = statuses
                    .into_iter()
                    .find(|status| *status != PoolMemberReadiness::Ready)
                {
                    let readiness_value = match status {
                        PoolMemberReadiness::MissingAccountReference => {
                            PoolReadiness::MissingAccountReference
                        }
                        PoolMemberReadiness::MissingConnectionReference => {
                            PoolReadiness::MissingConnectionReference
                        }
                        PoolMemberReadiness::UnavailableAccountReference => {
                            PoolReadiness::UnavailableAccountReference
                        }
                        PoolMemberReadiness::UnavailableConnectionReference => {
                            PoolReadiness::UnavailableConnectionReference
                        }
                        PoolMemberReadiness::Ready => PoolReadiness::Ready,
                    };
                    readiness.insert(pool.id.clone(), readiness_value);
                }
            }
        }
        Self {
            summaries: registry
                .pools()
                .map(|pool| {
                    let selected = match &pool.selection_policy {
                        AccountPoolSelectionPolicy::ExplicitMember(member_id) => {
                            member_id.to_string()
                        }
                        AccountPoolSelectionPolicy::RoundRobin => "Round robin".to_string(),
                    };
                    PoolSummary {
                        id: pool.id.clone(),
                        display_name: pool.display_name.clone(),
                        provider: pool.provider_family,
                        member_count: pool.members.len(),
                        selected,
                        is_round_robin: matches!(
                            pool.selection_policy,
                            AccountPoolSelectionPolicy::RoundRobin
                        ),
                        readiness: readiness
                            .get(&pool.id)
                            .copied()
                            .unwrap_or(PoolReadiness::InvalidStructure),
                        available_target_count: 0,
                        cooling_target_count: 0,
                        earliest_recovery: None,
                    }
                })
                .collect(),
            member_statuses,
            member_labels,
            runtime_statuses: BTreeMap::new(),
            cooldowns: cooldowns.clone(),
            error: None,
        }
        .with_cooldown_summary(registry)
    }

    fn with_cooldown_summary(mut self, registry: &NamedAccountPoolRegistry) -> Self {
        for summary in &mut self.summaries {
            let Some(pool) = registry.get(&summary.id) else {
                continue;
            };
            let targets = pool.members.iter().map(|member| &member.target);
            summary.available_target_count = self.cooldowns.available_target_count(targets);
            let targets = pool.members.iter().map(|member| &member.target);
            summary.cooling_target_count = self.cooldowns.cooling_target_count(targets);
            let targets = pool.members.iter().map(|member| &member.target);
            summary.earliest_recovery = self.cooldowns.earliest_recovery_for_targets(targets);
        }
        self
    }

    pub(crate) fn member_choices(
        &self,
        authority: &TuiPoolAuthority,
        provider: AccountPoolProviderFamily,
    ) -> Vec<PoolMemberChoice> {
        let mut choices = Vec::new();
        if provider == AccountPoolProviderFamily::NativeCodex {
            if let Some(accounts) = authority.accounts.as_deref() {
                choices.extend(accounts.profiles().map(|profile| PoolMemberChoice {
                    target: AccountPoolTarget::native_codex(profile.profile_id.clone()),
                    label: profile.label.clone(),
                    detail: format!("Native Codex account · {}", profile.profile_id),
                    readiness: if profile.enabled
                        && profile.state == crate::legacy_core::CodexAccountProfileState::Connected
                        && profile.validation
                            == crate::legacy_core::ConnectionValidationStatus::Valid
                    {
                        PoolMemberReadiness::Ready
                    } else {
                        PoolMemberReadiness::UnavailableAccountReference
                    },
                }));
            }
        } else if let Some(connections) = authority.omni_route.as_deref() {
            choices.extend(connections.connections().filter_map(|connection| {
                AccountPoolTarget::omniroute(connection.connection_id.clone())
                    .ok()
                    .map(|target| PoolMemberChoice {
                        target,
                        label: connection.label.clone(),
                        detail: format!(
                            "OmniRoute connection · {}",
                            safe_endpoint(&connection.base_url)
                        ),
                        readiness: if connection.enabled
                            && connection.validation.status
                                == crate::legacy_core::ConnectionValidationStatus::Valid
                        {
                            PoolMemberReadiness::Ready
                        } else {
                            PoolMemberReadiness::UnavailableConnectionReference
                        },
                    })
            }));
        }
        choices
    }
}

fn safe_endpoint(base_url: &str) -> String {
    base_url
        .split('?')
        .next()
        .unwrap_or(base_url)
        .chars()
        .take(96)
        .collect()
}

fn provider_label(provider: AccountPoolProviderFamily) -> &'static str {
    match provider {
        AccountPoolProviderFamily::NativeCodex => "Native Codex",
        AccountPoolProviderFamily::OmniRoute => "OmniRoute",
    }
}

fn readiness_label(readiness: PoolReadiness) -> &'static str {
    match readiness {
        PoolReadiness::Ready => "Ready",
        PoolReadiness::InvalidStructure => "Invalid candidate",
        PoolReadiness::MissingAccountReference
        | PoolReadiness::MissingConnectionReference
        | PoolReadiness::UnavailableAccountReference
        | PoolReadiness::UnavailableConnectionReference => "Needs attention",
        PoolReadiness::RotationRequiresRuntimeSelection => "Ready at role admission",
    }
}

fn member_readiness_label(readiness: PoolMemberReadiness) -> &'static str {
    match readiness {
        PoolMemberReadiness::Ready => "Ready",
        PoolMemberReadiness::MissingAccountReference
        | PoolMemberReadiness::MissingConnectionReference => "Missing",
        PoolMemberReadiness::UnavailableAccountReference
        | PoolMemberReadiness::UnavailableConnectionReference => "Unavailable",
    }
}

fn member_id_for(target: &AccountPoolTarget, registry: &NamedAccountPoolRegistry) -> PoolMemberId {
    let base = match target {
        AccountPoolTarget::NativeCodexAccount(id) => format!("account-{}", id.as_str()),
        AccountPoolTarget::OmniRouteConnection(id) => format!("connection-{id}"),
    };
    let mut candidate = base.clone();
    let mut suffix = 2;
    while registry
        .pools()
        .flat_map(|pool| pool.members.iter())
        .any(|member| member.id.as_str() == candidate)
    {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    PoolMemberId::new(candidate).unwrap_or_else(|_| PoolMemberId::new("member").unwrap())
}

pub(crate) fn pool_tab(snapshot: &PoolSetupSnapshot) -> (Box<dyn Renderable>, Vec<SelectionItem>) {
    let mut header = ColumnRenderable::new();
    header.push(Line::from(
        "Named pools use explicit selection or deterministic round robin. No fallback.".dim(),
    ));
    if let Some(error) = &snapshot.error {
        header.push(Line::from(error.clone().red()));
    }
    let mut items = snapshot
        .summaries
        .iter()
        .map(|summary| {
            let id = summary.id.clone();
            let cooldown_suffix = if summary.cooling_target_count == 0 {
                String::new()
            } else if summary.available_target_count == 0 {
                format!(
                    " · All targets cooling · Earliest recovery {}",
                    summary
                        .earliest_recovery
                        .map(format_cooldown_duration)
                        .unwrap_or_else(|| "unknown".to_string())
                )
            } else {
                format!(
                    " · {} available · {} cooling",
                    summary.available_target_count, summary.cooling_target_count
                )
            };
            SelectionItem {
                name: format!(
                    "{} · {}",
                    summary.display_name,
                    readiness_label(summary.readiness)
                ),
                description: Some(
                    format!(
                        "{} · {} members · {} · ID {}",
                        provider_label(summary.provider),
                        summary.member_count,
                        if summary.is_round_robin {
                            "Round robin · selected at role admission"
                        } else {
                            "Explicit member"
                        },
                        summary.id
                    ) + &format!(
                        " · Runtime {}",
                        installed_pool_status_label(
                            snapshot
                                .runtime_statuses
                                .get(&summary.id)
                                .copied()
                                .unwrap_or(InstalledPoolStatus::NotCurrentlyRouted)
                        )
                    ) + &cooldown_suffix,
                ),
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::OpenPoolEditor {
                        pool_id: id.clone(),
                    })
                })],
                dismiss_on_select: true,
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();
    items.push(SelectionItem {
        name: "Create pool".to_string(),
        description: Some(
            "Choose a stable pool ID, provider, members, and explicit selection.".to_string(),
        ),
        actions: vec![Box::new(|tx| tx.send(AppEvent::BeginPoolCreation))],
        dismiss_on_select: true,
        ..Default::default()
    });
    if snapshot.error.is_some() {
        items.push(SelectionItem {
            name: "Replace invalid registry with an empty registry".to_string(),
            description: Some(
                "The preserved pool file will be replaced only after confirmation.".to_string(),
            ),
            actions: vec![Box::new(|tx| tx.send(AppEvent::ConfirmPoolRegistryRepair))],
            dismiss_on_select: true,
            ..Default::default()
        });
    }
    items.push(SelectionItem {
        name: "Cancel".to_string(),
        description: Some("Discard all pool edits.".to_string()),
        actions: vec![Box::new(|tx| tx.send(AppEvent::CancelPoolManagement))],
        dismiss_on_select: true,
        ..Default::default()
    });
    (Box::new(header), items)
}

impl ChatWidget {
    pub(crate) fn set_pool_setup_candidate(&mut self, candidate: NamedAccountPoolRegistry) {
        self.pool_setup_candidate = Some(candidate);
        self.pool_policy_needs_member = None;
    }

    pub(crate) fn clear_pool_setup_candidate(&mut self) {
        self.pool_setup_candidate = None;
        self.pool_policy_needs_member = None;
        self.clear_pool_creation();
    }

    pub(crate) fn pool_setup_candidate(&self) -> Option<NamedAccountPoolRegistry> {
        self.pool_setup_candidate.clone()
    }

    pub(crate) fn set_pool_creation_id(&mut self, pool_id: PoolId) {
        self.pool_creation_id = Some(pool_id);
    }

    pub(crate) fn set_pool_creation_name(&mut self, name: String) {
        self.pool_creation_name = Some(name);
    }

    pub(crate) fn set_pool_creation_provider(&mut self, provider: AccountPoolProviderFamily) {
        self.pool_creation_provider = Some(provider);
    }

    pub(crate) fn pool_creation(&self) -> Option<(PoolId, String, AccountPoolProviderFamily)> {
        Some((
            self.pool_creation_id.clone()?,
            self.pool_creation_name.clone()?,
            self.pool_creation_provider?,
        ))
    }

    pub(crate) fn clear_pool_creation(&mut self) {
        self.pool_creation_id = None;
        self.pool_creation_name = None;
        self.pool_creation_provider = None;
    }

    pub(crate) fn pool_by_id(&self, pool_id: &PoolId) -> Option<NamedAccountPool> {
        self.pool_setup_candidate.as_ref()?.get(pool_id).cloned()
    }

    pub(crate) fn rename_pool_candidate(
        &mut self,
        pool_id: &PoolId,
        display_name: String,
    ) -> Result<(), String> {
        let Some(registry) = self.pool_setup_candidate.as_mut() else {
            return Err("Pool candidate is unavailable.".to_string());
        };
        let Some(mut pool) = registry.remove(pool_id) else {
            return Err("Pool was not found in the candidate.".to_string());
        };
        pool.display_name = display_name;
        if let Err(error) = registry.insert(pool.clone()) {
            let _ = registry.insert(pool);
            return Err(error.to_string());
        }
        Ok(())
    }

    pub(crate) fn select_pool_member_candidate(
        &mut self,
        pool_id: &PoolId,
        member_id: &PoolMemberId,
    ) -> Result<(), String> {
        let Some(registry) = self.pool_setup_candidate.as_mut() else {
            return Err("Pool candidate is unavailable.".to_string());
        };
        let Some(mut pool) = registry.remove(pool_id) else {
            return Err("Pool was not found in the candidate.".to_string());
        };
        if !pool.members.iter().any(|member| member.id == *member_id) {
            registry.insert(pool).map_err(|error| error.to_string())?;
            return Err("Selected member is not in the pool.".to_string());
        }
        pool.selection_policy = AccountPoolSelectionPolicy::ExplicitMember(member_id.clone());
        if self.pool_policy_needs_member.as_ref() == Some(pool_id) {
            self.pool_policy_needs_member = None;
        }
        if let Err(error) = registry.insert(pool.clone()) {
            let _ = registry.insert(pool);
            return Err(error.to_string());
        }
        Ok(())
    }

    pub(crate) fn remove_pool_member_candidate(
        &mut self,
        pool_id: &PoolId,
        member_id: &PoolMemberId,
    ) -> Result<(), String> {
        let Some(registry) = self.pool_setup_candidate.as_mut() else {
            return Err("Pool candidate is unavailable.".to_string());
        };
        let Some(mut pool) = registry.remove(pool_id) else {
            return Err("Pool was not found in the candidate.".to_string());
        };
        if pool.members.len() <= 1 {
            registry.insert(pool).map_err(|error| error.to_string())?;
            return Err("A pool must retain at least one member.".to_string());
        }
        let selected = match &pool.selection_policy {
            AccountPoolSelectionPolicy::ExplicitMember(selected) => *selected == *member_id,
            AccountPoolSelectionPolicy::RoundRobin => false,
        };
        if selected {
            registry.insert(pool).map_err(|error| error.to_string())?;
            return Err("Select another member before removing the explicit member.".to_string());
        }
        pool.members.retain(|member| member.id != *member_id);
        if let Err(error) = registry.insert(pool.clone()) {
            let _ = registry.insert(pool);
            return Err(error.to_string());
        }
        Ok(())
    }

    pub(crate) fn pool_policy_needs_member(&self, pool_id: &PoolId) -> bool {
        self.pool_policy_needs_member.as_ref() == Some(pool_id)
    }

    pub(crate) fn select_pool_policy_candidate(
        &mut self,
        pool_id: &PoolId,
        policy: AccountPoolSelectionPolicy,
    ) -> Result<(), String> {
        let Some(registry) = self.pool_setup_candidate.as_mut() else {
            return Err("Pool candidate is unavailable.".to_string());
        };
        let Some(mut pool) = registry.remove(pool_id) else {
            return Err("Pool was not found in the candidate.".to_string());
        };
        match policy {
            AccountPoolSelectionPolicy::RoundRobin => {
                pool.selection_policy = AccountPoolSelectionPolicy::RoundRobin;
                self.pool_policy_needs_member = None;
            }
            AccountPoolSelectionPolicy::ExplicitMember(_) => {
                if matches!(
                    pool.selection_policy,
                    AccountPoolSelectionPolicy::RoundRobin
                ) {
                    self.pool_policy_needs_member = Some(pool_id.clone());
                }
            }
        }
        registry.insert(pool).map_err(|error| error.to_string())
    }

    pub(crate) fn add_pool_member_candidate(
        &mut self,
        pool_id: &PoolId,
        target: AccountPoolTarget,
    ) -> Result<PoolId, String> {
        let creation = self.pool_creation();
        let Some(registry) = self.pool_setup_candidate.as_mut() else {
            return Err("Pool candidate is unavailable.".to_string());
        };
        let member_id = member_id_for(&target, registry);
        if let Some(mut pool) = registry.remove(pool_id) {
            if pool.provider_family != target.provider_family() {
                registry.insert(pool).map_err(|error| error.to_string())?;
                return Err("Pool members must use one provider family.".to_string());
            }
            pool.members.push(AccountPoolMember {
                id: member_id,
                target,
            });
            if let Err(error) = registry.insert(pool.clone()) {
                let _ = registry.insert(pool);
                return Err(error.to_string());
            }
            return Ok(pool_id.clone());
        }
        let Some((creation_id, name, provider)) = creation else {
            return Err("Pool creation is unavailable.".to_string());
        };
        if provider != target.provider_family() {
            return Err("The selected member does not match the pool provider.".to_string());
        }
        let selected_member_id = member_id.clone();
        let pool = NamedAccountPool {
            id: creation_id.clone(),
            display_name: name,
            provider_family: provider,
            members: vec![AccountPoolMember {
                id: member_id,
                target,
            }],
            selection_policy: AccountPoolSelectionPolicy::ExplicitMember(selected_member_id),
        };
        registry.insert(pool).map_err(|error| error.to_string())?;
        self.clear_pool_creation();
        Ok(creation_id)
    }

    pub(crate) fn delete_pool_candidate(&mut self, pool_id: &PoolId) -> Result<(), String> {
        let Some(registry) = self.pool_setup_candidate.as_mut() else {
            return Err("Pool candidate is unavailable.".to_string());
        };
        let result = registry
            .remove(pool_id)
            .map(|_| ())
            .ok_or_else(|| "Pool was not found in the candidate.".to_string());
        if result.is_ok() && self.pool_policy_needs_member.as_ref() == Some(pool_id) {
            self.pool_policy_needs_member = None;
        }
        result
    }

    pub(crate) fn open_pool_provider_picker(&mut self) {
        let items = [
            AccountPoolProviderFamily::NativeCodex,
            AccountPoolProviderFamily::OmniRoute,
        ]
        .into_iter()
        .map(|provider| SelectionItem {
            name: provider_label(provider).to_string(),
            description: Some("Pool provider is immutable once members exist.".to_string()),
            actions: vec![Box::new(move |tx| {
                tx.send(AppEvent::ChoosePoolProvider(provider))
            })],
            dismiss_on_select: true,
            ..Default::default()
        })
        .chain([SelectionItem {
            name: "Cancel".to_string(),
            description: Some("Return without creating a pool.".to_string()),
            actions: vec![Box::new(|tx| tx.send(AppEvent::CancelPoolManagement))],
            dismiss_on_select: true,
            ..Default::default()
        }])
        .collect();
        self.bottom_pane.show_selection_view(SelectionViewParams {
            view_id: Some(POOL_MANAGEMENT_VIEW_ID),
            title: Some("Create pool · provider".to_string()),
            subtitle: Some("Choose a homogeneous provider family".to_string()),
            items,
            on_cancel: Some(Box::new(|tx| tx.send(AppEvent::CancelPoolManagement))),
            ..Default::default()
        });
    }

    pub(crate) fn open_pool_registry_repair_confirmation(&mut self) {
        let items = vec![
            SelectionItem {
                name: "Replace preserved invalid registry".to_string(),
                description: Some("Write a new empty canonical registry.".to_string()),
                actions: vec![Box::new(|tx| tx.send(AppEvent::ReplacePoolRegistry))],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Cancel".to_string(),
                description: Some("Keep the invalid file untouched.".to_string()),
                actions: vec![Box::new(|tx| tx.send(AppEvent::CancelPoolManagement))],
                dismiss_on_select: true,
                ..Default::default()
            },
        ];
        self.bottom_pane.show_selection_view(SelectionViewParams {
            view_id: Some(POOL_MANAGEMENT_VIEW_ID),
            title: Some("Repair pool registry?".to_string()),
            subtitle: Some("This explicit action replaces the preserved invalid file.".to_string()),
            items,
            on_cancel: Some(Box::new(|tx| tx.send(AppEvent::CancelPoolManagement))),
            ..Default::default()
        });
    }

    pub(crate) fn open_pool_delete_confirmation(&mut self, pool_id: PoolId) {
        let confirm_id = pool_id.clone();
        let return_pool_id = pool_id.clone();
        let action_pool_id = return_pool_id.clone();
        let items = vec![
            SelectionItem {
                name: format!("Delete pool {pool_id}"),
                description: Some("This only changes the named pool registry.".to_string()),
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::DeletePool {
                        pool_id: confirm_id.clone(),
                    })
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Cancel".to_string(),
                description: Some("Keep the pool and all candidate edits.".to_string()),
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::OpenPoolEditor {
                        pool_id: action_pool_id.clone(),
                    })
                })],
                dismiss_on_select: true,
                ..Default::default()
            },
        ];
        self.bottom_pane.show_selection_view(SelectionViewParams {
            view_id: Some(POOL_MANAGEMENT_VIEW_ID),
            title: Some("Confirm pool deletion".to_string()),
            subtitle: Some("Destructive action".to_string()),
            items,
            on_cancel: Some(Box::new(move |tx| {
                tx.send(AppEvent::OpenPoolEditor {
                    pool_id: return_pool_id.clone(),
                })
            })),
            ..Default::default()
        });
    }
    pub(crate) fn open_pool_editor(&mut self, pool: NamedAccountPool, snapshot: PoolSetupSnapshot) {
        let pool_id = pool.id.clone();
        let policy_needs_member = self.pool_policy_needs_member(&pool.id);
        let is_round_robin = !policy_needs_member
            && matches!(
                &pool.selection_policy,
                AccountPoolSelectionPolicy::RoundRobin
            );
        let mut header = ColumnRenderable::new();
        header.push(Line::from(format!(
            "Provider: {}",
            provider_label(pool.provider_family)
        )));
        header.push(Line::from(match &pool.selection_policy {
            AccountPoolSelectionPolicy::ExplicitMember(_) => {
                "Only the explicitly selected member is used; no automatic member substitution."
                    .dim()
            }
            AccountPoolSelectionPolicy::RoundRobin => {
                "First use of each role per turn selects deterministically; active cooldowns are skipped before provider invocation; same-turn use reuses it."
                    .dim()
            }
        }));
        let mut items = vec![SelectionItem {
            name: format!("Name · {}", pool.display_name),
            description: Some("Rename this pool without changing its stable ID.".to_string()),
            actions: vec![Box::new(move |tx| {
                tx.send(AppEvent::EditPoolName {
                    pool_id: pool_id.clone(),
                })
            })],
            dismiss_on_select: true,
            ..Default::default()
        }];
        let selected_id = match &pool.selection_policy {
            AccountPoolSelectionPolicy::ExplicitMember(id) => Some(id.clone()),
            AccountPoolSelectionPolicy::RoundRobin => {
                policy_needs_member.then_some(PoolMemberId::new("pending").unwrap())
            }
        };
        let readiness = snapshot
            .summaries
            .iter()
            .find(|summary| summary.id == pool.id)
            .map(|summary| summary.readiness)
            .unwrap_or(PoolReadiness::InvalidStructure);
        let installed_status = snapshot
            .runtime_statuses
            .get(&pool.id)
            .copied()
            .unwrap_or(InstalledPoolStatus::NotCurrentlyRouted);
        if let Some(summary) = snapshot
            .summaries
            .iter()
            .find(|summary| summary.id == pool.id)
            && summary.cooling_target_count > 0
        {
            let description = if summary.available_target_count == 0 {
                format!(
                    "All configured targets are cooling down. Earliest recovery: {}.",
                    summary
                        .earliest_recovery
                        .map(format_cooldown_duration)
                        .unwrap_or_else(|| "unknown".to_string())
                )
            } else {
                format!(
                    "{} targets currently available; {} cooling.",
                    summary.available_target_count, summary.cooling_target_count
                )
            };
            items.push(SelectionItem {
                name: "Runtime eligibility".to_string(),
                description: Some(description),
                is_disabled: true,
                ..Default::default()
            });
        }
        items.push(SelectionItem {
            name: format!("Status · {}", readiness_label(readiness)),
            description: Some(if policy_needs_member {
                "Choose an explicit member before saving this policy.".to_string()
            } else if is_round_robin {
                "Round robin is active at role admission; active cooldowns are skipped before provider invocation. Ordinary unavailable or invalid members still fail normally.".to_string()
            } else {
                "Readiness is derived from canonical account and connection metadata.".to_string()
            }),
            is_disabled: true,
            ..Default::default()
        });
        let installed_status_description = match installed_status {
            InstalledPoolStatus::Current => {
                "Saved pool configuration matches the installed routing snapshot.".to_string()
            }
            InstalledPoolStatus::ReapplyRouting => {
                "Saved pool configuration changed. Use Setup → Routing → Apply to activate it; the installed runtime and active turn are unchanged.".to_string()
            }
            InstalledPoolStatus::NotCurrentlyRouted => {
                "This pool is not referenced by the installed routing runtime.".to_string()
            }
            InstalledPoolStatus::InstalledSnapshotMissingFromRegistry => {
                "The installed runtime uses a prior snapshot of this missing pool. Repair the pool, then reapply routing.".to_string()
            }
        };
        items.push(SelectionItem {
            name: format!(
                "Installed routing · {}",
                installed_pool_status_label(installed_status)
            ),
            description: Some(installed_status_description),
            is_disabled: true,
            ..Default::default()
        });
        for (policy, label, description) in [
            (
                AccountPoolSelectionPolicy::ExplicitMember(PoolMemberId::new("pending").unwrap()),
                "Explicit member",
                "Use exactly one explicitly selected member.",
            ),
            (
                AccountPoolSelectionPolicy::RoundRobin,
                "Round robin",
                "Select once per role per turn; reuse within the turn. No skip or fallback.",
            ),
        ] {
            let policy_pool_id = pool.id.clone();
            let is_current = if matches!(policy, AccountPoolSelectionPolicy::RoundRobin) {
                is_round_robin
            } else {
                !is_round_robin
            };
            items.push(SelectionItem {
                name: format!("Policy · {label}"),
                description: Some(description.to_string()),
                is_current,
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::SelectPoolPolicy {
                        pool_id: policy_pool_id.clone(),
                        policy: policy.clone(),
                    })
                })],
                dismiss_on_select: false,
                ..Default::default()
            });
        }
        for member in &pool.members {
            let member_id = member.id.clone();
            let select_pool_id = pool.id.clone();
            let label = member.id.to_string();
            let cooldown_detail =
                cooldown_label(&snapshot.cooldowns.status_for_target(&member.target));
            items.push(SelectionItem {
                name: format!(
                    "{} {} · {} · {}",
                    if selected_id.as_ref() == Some(&member.id) {
                        "●"
                    } else {
                        "○"
                    },
                    label,
                    member_readiness_label(
                        snapshot
                            .member_statuses
                            .get(&(pool.id.clone(), member.id.clone()))
                            .copied()
                            .unwrap_or(PoolMemberReadiness::UnavailableAccountReference),
                    ),
                    cooldown_detail
                ),
                description: Some(format!(
                    "Select as explicit member: {} · {}",
                    snapshot
                        .member_labels
                        .get(&(pool.id.clone(), member.id.clone()))
                        .cloned()
                        .unwrap_or_else(|| target_label(&member.target)),
                    cooldown_detail
                )),
                is_current: selected_id.as_ref() == Some(&member.id),
                is_disabled: is_round_robin,
                disabled_reason: is_round_robin
                    .then_some("Round robin does not choose a member in setup.".to_string()),
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::SelectPoolMember {
                        pool_id: select_pool_id.clone(),
                        member_id: member_id.clone(),
                    })
                })],
                dismiss_on_select: false,
                ..Default::default()
            });
            let remove_id = member.id.clone();
            let remove_pool_id = pool.id.clone();
            items.push(SelectionItem {
                name: format!("Remove {}", member.id),
                description: Some(if selected_id.as_ref() == Some(&member.id) {
                    "Select another member before removing the explicit member.".to_string()
                } else {
                    "Remove this member explicitly from the candidate.".to_string()
                }),
                is_disabled: selected_id.as_ref() == Some(&member.id),
                disabled_reason: (selected_id.as_ref() == Some(&member.id))
                    .then_some("The explicitly selected member cannot be removed yet.".to_string()),
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::RemovePoolMember {
                        pool_id: remove_pool_id.clone(),
                        member_id: remove_id.clone(),
                    })
                })],
                dismiss_on_select: false,
                ..Default::default()
            });
        }
        let add_id = pool.id.clone();
        items.push(SelectionItem {
            name: "Add member".to_string(),
            description: Some("Choose an existing canonical account or connection.".to_string()),
            actions: vec![Box::new(move |tx| {
                tx.send(AppEvent::OpenPoolMemberPicker {
                    pool_id: add_id.clone(),
                })
            })],
            dismiss_on_select: true,
            ..Default::default()
        });
        let save_pool_id = pool.id.clone();
        items.push(SelectionItem {
            name: "Save pool".to_string(),
            description: Some(
                "Validate and atomically save the complete pool registry.".to_string(),
            ),
            is_disabled: policy_needs_member,
            disabled_reason: policy_needs_member
                .then_some("Choose an explicit member before saving.".to_string()),
            actions: vec![Box::new(move |tx| {
                tx.send(AppEvent::SavePoolRegistry {
                    pool_id: save_pool_id.clone(),
                })
            })],
            dismiss_on_select: true,
            ..Default::default()
        });
        let delete_id = pool.id.clone();
        items.push(SelectionItem {
            name: "Delete pool".to_string(),
            description: Some("Destructive: remove this pool after confirmation.".to_string()),
            actions: vec![Box::new(move |tx| {
                tx.send(AppEvent::ConfirmPoolDeletion {
                    pool_id: delete_id.clone(),
                })
            })],
            dismiss_on_select: true,
            ..Default::default()
        });
        items.push(SelectionItem {
            name: "Cancel".to_string(),
            description: Some("Discard candidate edits to this pool.".to_string()),
            actions: vec![Box::new(|tx| tx.send(AppEvent::CancelPoolManagement))],
            dismiss_on_select: true,
            ..Default::default()
        });
        self.bottom_pane.show_selection_view(SelectionViewParams {
            view_id: Some(POOL_MANAGEMENT_VIEW_ID),
            title: Some(format!("Pool: {}", pool.id)),
            subtitle: Some(pool.display_name),
            header: Box::new(header),
            items,
            on_cancel: Some(Box::new(|tx| tx.send(AppEvent::CancelPoolManagement))),
            col_width_mode: ColumnWidthMode::AutoAllRows,
            ..Default::default()
        });
    }

    pub(crate) fn open_pool_member_picker(
        &mut self,
        pool: NamedAccountPool,
        snapshot: PoolSetupSnapshot,
        authority: &TuiPoolAuthority,
    ) {
        let return_pool_id = pool.id.clone();
        let action_pool_id = return_pool_id.clone();
        let choices = snapshot.member_choices(authority, pool.provider_family);
        let existing = pool
            .members
            .iter()
            .map(|member| &member.target)
            .collect::<Vec<_>>();
        let items = choices
            .into_iter()
            .map(|choice| {
                let already_present = existing.iter().any(|target| **target == choice.target);
                let member_id = self.pool_setup_candidate.as_ref().map_or_else(
                    || PoolMemberId::new("member").unwrap(),
                    |registry| member_id_for(&choice.target, registry),
                );
                let pool_id = pool.id.clone();
                let target = choice.target.clone();
                SelectionItem {
                    name: format!(
                        "{} · {}",
                        choice.label,
                        member_readiness_label(choice.readiness)
                    ),
                    description: Some(if already_present {
                        "Already present in this pool.".to_string()
                    } else {
                        format!("Add as member ID {} · {}", member_id, choice.detail)
                    }),
                    is_disabled: already_present || choice.readiness != PoolMemberReadiness::Ready,
                    disabled_reason: (already_present
                        || choice.readiness != PoolMemberReadiness::Ready)
                        .then_some(
                            "Only available, not-yet-added identities can be selected.".to_string(),
                        ),
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::AddPoolMember {
                            pool_id: pool_id.clone(),
                            target: target.clone(),
                        })
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                }
            })
            .chain([SelectionItem {
                name: "Cancel".to_string(),
                description: Some("Return without changing the candidate.".to_string()),
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::OpenPoolEditor {
                        pool_id: action_pool_id.clone(),
                    })
                })],
                dismiss_on_select: true,
                ..Default::default()
            }])
            .collect();
        let mut header = ColumnRenderable::new();
        header.push(Line::from("Existing canonical identities only.".dim()));
        header.push(Line::from(
            "Credentials and secret connection data are never displayed.".dim(),
        ));
        self.bottom_pane.show_selection_view(SelectionViewParams {
            view_id: Some(POOL_MANAGEMENT_VIEW_ID),
            title: Some(format!("Add member · {}", pool.id)),
            subtitle: Some(provider_label(pool.provider_family).to_string()),
            header: Box::new(header),
            items,
            on_cancel: Some(Box::new(move |tx| {
                tx.send(AppEvent::OpenPoolEditor {
                    pool_id: return_pool_id.clone(),
                })
            })),
            is_searchable: true,
            search_placeholder: Some("Search existing identities".to_string()),
            col_width_mode: ColumnWidthMode::AutoAllRows,
            ..Default::default()
        });
    }

    pub(crate) fn open_pool_id_prompt(&mut self) {
        let tx = self.app_event_tx.clone();
        self.bottom_pane.show_view(Box::new(CustomPromptView::new(
            "Create pool · stable ID".to_string(),
            "Enter a bounded ID (letters, numbers, - or _)".to_string(),
            String::new(),
            Some(
                "The ID is independent from the display name and cannot be silently changed."
                    .to_string(),
            ),
            Box::new(move |value| tx.send(AppEvent::PoolCreationIdEntered { value })),
        )));
    }

    pub(crate) fn open_pool_name_prompt(&mut self) {
        let tx = self.app_event_tx.clone();
        self.bottom_pane.show_view(Box::new(CustomPromptView::new(
            "Create pool · display name".to_string(),
            "Enter a bounded display name".to_string(),
            String::new(),
            Some("The display name is not used as identity.".to_string()),
            Box::new(move |value| tx.send(AppEvent::PoolCreationNameEntered { value })),
        )));
    }

    pub(crate) fn open_pool_rename_prompt(&mut self, pool_id: PoolId, current: String) {
        let tx = self.app_event_tx.clone();
        self.bottom_pane.show_view(Box::new(CustomPromptView::new(
            "Rename pool".to_string(),
            "Enter a bounded display name".to_string(),
            current,
            None,
            Box::new(move |value| {
                tx.send(AppEvent::PoolRenamed {
                    pool_id: pool_id.clone(),
                    value,
                })
            }),
        )));
    }
}

fn target_label(target: &AccountPoolTarget) -> String {
    match target {
        AccountPoolTarget::NativeCodexAccount(id) => format!("Native Codex account {}", id),
        AccountPoolTarget::OmniRouteConnection(id) => format!("OmniRoute connection {id}"),
    }
}

#[cfg(test)]
#[path = "pool_setup_tests.rs"]
mod tests;
