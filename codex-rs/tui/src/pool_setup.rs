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
    #[cfg(test)]
    pub(crate) fn from_registry(
        registry: &NamedAccountPoolRegistry,
        accounts: Option<&crate::legacy_core::CodexAccountProfileRegistry>,
        connections: Option<&crate::legacy_core::OmniRouteRegistry>,
    ) -> Self {
        Self::from_registry_with_cooldowns(
            registry,
            accounts,
            connections,
            &TuiProviderCooldownSnapshot::default(),
        )
    }

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

// Remaining file content intentionally preserved from the current branch.
