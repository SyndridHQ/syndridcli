//! Redacted provider, account, connection, and role readiness for `/setup`.
//!
//! This module is a presentation boundary over the canonical Syndrid registries. It deliberately
//! carries display metadata only; it never owns credentials, provider clients, or runtime state.

use crate::cooldown_status::TuiProviderCooldownSnapshot;
use crate::cooldown_status::cooldown_label;
use crate::legacy_core::AccountPoolTarget;
use crate::legacy_core::CodexAccountProfileRegistry;
use crate::legacy_core::CodexAccountProfileState;
use crate::legacy_core::ConnectionValidationStatus;
use crate::legacy_core::RoutingConnectionDirectory;
use crate::legacy_core::RoutingRole;
use crate::orchestration_setup::SetupReadinessState;
use crate::syndrid_composition::TuiCanonicalAuthorities;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProviderSetupItem {
    pub(crate) name: String,
    pub(crate) detail: String,
    pub(crate) id: Option<String>,
    pub(crate) provider_id: Option<String>,
    pub(crate) target: Option<AccountPoolTarget>,
    pub(crate) models: Vec<String>,
    pub(crate) readiness: SetupReadinessState,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub(crate) struct ProviderSetupSnapshot {
    pub(crate) providers: Vec<ProviderSetupItem>,
    pub(crate) accounts: Vec<ProviderSetupItem>,
    pub(crate) connections: Vec<ProviderSetupItem>,
    pub(crate) roles: Vec<ProviderSetupItem>,
    pub(crate) saved_profile_id: Option<String>,
}

impl ProviderSetupSnapshot {
    pub(crate) fn with_cooldowns(&self, cooldowns: &TuiProviderCooldownSnapshot) -> Self {
        let mut snapshot = self.clone();
        for item in snapshot
            .accounts
            .iter_mut()
            .chain(snapshot.connections.iter_mut())
        {
            let Some(target) = item.target.as_ref() else {
                continue;
            };
            item.detail.push_str(" · ");
            item.detail
                .push_str(&cooldown_label(&cooldowns.status_for_target(&target)));
        }
        snapshot
    }

    pub(crate) fn unavailable() -> Self {
        Self {
            providers: vec![
                ProviderSetupItem {
                    name: "Native Codex".to_string(),
                    detail: "authentication metadata is unavailable".to_string(),
                    id: None,
                    provider_id: Some("codex".to_string()),
                    target: None,
                    models: Vec::new(),
                    readiness: SetupReadinessState::MissingAuthority(
                        "Codex account authority is unavailable".to_string(),
                    ),
                },
                ProviderSetupItem {
                    name: "OmniRoute".to_string(),
                    detail: "connection metadata is unavailable".to_string(),
                    id: None,
                    provider_id: Some("omniroute".to_string()),
                    target: None,
                    models: Vec::new(),
                    readiness: SetupReadinessState::MissingAuthority(
                        "OmniRoute connection authority is unavailable".to_string(),
                    ),
                },
                ProviderSetupItem {
                    name: "OpenRouter".to_string(),
                    detail: "production provider integration is not implemented".to_string(),
                    id: None,
                    provider_id: Some("openrouter".to_string()),
                    target: None,
                    models: Vec::new(),
                    readiness: SetupReadinessState::Unavailable(
                        "OpenRouter production provider integration is not implemented yet"
                            .to_string(),
                    ),
                },
            ],
            ..Default::default()
        }
    }

    pub(crate) fn from_authorities(authorities: &TuiCanonicalAuthorities) -> Self {
        let accounts = authorities.provider.accounts.as_deref();
        let omni_route = authorities.provider.omni_route.as_deref();
        let connections = authorities.routing.connections.as_deref();
        let profiles = authorities.routing.profiles.as_deref();

        let account_items: Vec<ProviderSetupItem> = accounts
            .map(|registry| {
                registry
                    .profiles()
                    .map(|profile| {
                        let ready = profile.enabled
                            && profile.state == CodexAccountProfileState::Connected
                            && profile.validation == ConnectionValidationStatus::Valid
                            && profile.account_id.is_some();
                        ProviderSetupItem {
                            name: profile.label.clone(),
                            detail: "Native Codex account".to_string(),
                            id: Some(profile.connection_id.clone()),
                            provider_id: Some(profile.provider_id.clone()),
                            target: Some(AccountPoolTarget::native_codex(
                                profile.profile_id.clone(),
                            )),
                            models: Vec::new(),
                            readiness: if ready {
                                SetupReadinessState::Ready
                            } else {
                                SetupReadinessState::Invalid(
                                    "Codex authentication is not usable".to_string(),
                                )
                            },
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let connection_items: Vec<ProviderSetupItem> = connections
            .map(|directory| {
                let mut items = Vec::new();
                if let Some(registry) = omni_route {
                    items.extend(registry.connections().map(|connection| {
                        let ready = connection.enabled
                            && connection.validation.status == ConnectionValidationStatus::Valid
                            && !connection.models.is_empty();
                        ProviderSetupItem {
                            name: connection.label.clone(),
                            detail: format!(
                                "OmniRoute · {} configured models",
                                connection.models.len()
                            ),
                            id: Some(connection.connection_id.clone()),
                            provider_id: Some(connection.provider_id.clone()),
                            target: AccountPoolTarget::omniroute(&connection.connection_id).ok(),
                            models: connection.models.clone(),
                            readiness: if ready {
                                SetupReadinessState::Ready
                            } else {
                                SetupReadinessState::Invalid(
                                    "OmniRoute endpoint, authentication, or model configuration needs attention".to_string(),
                                )
                            },
                        }
                    }));
                }
                for connection in directory_names(directory, accounts) {
                    items.push(ProviderSetupItem {
                        name: connection.0,
                        detail: "Native Codex connection".to_string(),
                        id: Some(connection.2.clone()),
                        provider_id: Some("codex".to_string()),
                        target: crate::legacy_core::CodexAccountProfileId::new(&connection.2)
                            .ok()
                            .map(AccountPoolTarget::native_codex),
                        models: Vec::new(),
                        readiness: connection.1,
                    });
                }
                items
            })
            .unwrap_or_default();

        let codex_ready = account_items
            .iter()
            .any(|item| item.readiness == SetupReadinessState::Ready);
        let omniroute_ready = connection_items.iter().any(|item| {
            item.detail.starts_with("OmniRoute") && item.readiness == SetupReadinessState::Ready
        });
        let providers = vec![
            ProviderSetupItem {
                name: "Native Codex".to_string(),
                detail: if codex_ready {
                    "authenticated account available".to_string()
                } else {
                    "no usable authenticated account".to_string()
                },
                readiness: if codex_ready {
                    SetupReadinessState::Ready
                } else {
                    SetupReadinessState::MissingAuthority(
                        "No usable Codex authentication".to_string(),
                    )
                },
                id: None,
                provider_id: Some("codex".to_string()),
                target: None,
                models: Vec::new(),
            },
            ProviderSetupItem {
                name: "OmniRoute".to_string(),
                detail: if omniroute_ready {
                    "configured connection available".to_string()
                } else {
                    "no usable OmniRoute connection".to_string()
                },
                readiness: if omniroute_ready {
                    SetupReadinessState::Ready
                } else {
                    SetupReadinessState::MissingAuthority(
                        "No usable OmniRoute connection".to_string(),
                    )
                },
                id: None,
                provider_id: Some("omniroute".to_string()),
                target: None,
                models: Vec::new(),
            },
            ProviderSetupItem {
                name: "OpenRouter".to_string(),
                detail: "production provider integration is not implemented".to_string(),
                readiness: SetupReadinessState::Unavailable(
                    "OpenRouter production provider integration is not implemented yet".to_string(),
                ),
                id: None,
                provider_id: Some("openrouter".to_string()),
                target: None,
                models: Vec::new(),
            },
        ];

        let roles = profiles
            .and_then(|registry| registry.read().ok())
            .and_then(|registry| registry.active().ok().cloned())
            .map(|profile| {
                [
                    RoutingRole::Main,
                    RoutingRole::Planner,
                    RoutingRole::Executor,
                    RoutingRole::Verifier,
                    RoutingRole::Repair,
                ]
                .into_iter()
                .map(|role| {
                    let Some(assignment) = profile.assignments.get(&role) else {
                        return ProviderSetupItem {
                            name: role.to_string(),
                            detail: "no configured binding".to_string(),
                            id: None,
                            provider_id: None,
                            target: None,
                            models: Vec::new(),
                            readiness: SetupReadinessState::MissingAuthority(
                                "required role binding is missing".to_string(),
                            ),
                        };
                    };
                    let readiness = if connections
                        .and_then(|directory| directory.validate_assignment(assignment).ok())
                        .is_none()
                    {
                        SetupReadinessState::Invalid(
                            "provider, connection, or model configuration needs attention"
                                .to_string(),
                        )
                    } else {
                        match assignment.provider_id.as_str() {
                            "codex" => accounts
                                .and_then(|registry| {
                                    registry.get_connection(&assignment.connection_id)
                                })
                                .filter(|account| {
                                    account.enabled
                                        && account.state == CodexAccountProfileState::Connected
                                        && account.validation == ConnectionValidationStatus::Valid
                                        && account.account_id.is_some()
                                })
                                .map(|_| SetupReadinessState::Ready)
                                .unwrap_or_else(|| {
                                    SetupReadinessState::Invalid(
                                        "Codex account authentication needs attention".to_string(),
                                    )
                                }),
                            "omniroute" => omni_route
                                .and_then(|registry| registry.get(&assignment.connection_id))
                                .filter(|connection| {
                                    connection.enabled
                                        && connection.validation.status
                                            == ConnectionValidationStatus::Valid
                                        && connection
                                            .models
                                            .iter()
                                            .any(|model| model == &assignment.model_id)
                                })
                                .map(|_| SetupReadinessState::Ready)
                                .unwrap_or_else(|| {
                                    SetupReadinessState::Invalid(
                                        "OmniRoute connection or model needs attention".to_string(),
                                    )
                                }),
                            _ => SetupReadinessState::Unavailable(
                                "provider construction is not implemented for this provider"
                                    .to_string(),
                            ),
                        }
                    };
                    ProviderSetupItem {
                        name: role.to_string(),
                        detail: format!(
                            "{} / {} / {}",
                            assignment.provider_id, assignment.connection_id, assignment.model_id
                        ),
                        id: Some(assignment.connection_id.clone()),
                        provider_id: Some(assignment.provider_id.clone()),
                        target: None,
                        models: Vec::new(),
                        readiness,
                    }
                })
                .collect()
            })
            .unwrap_or_else(|| {
                vec![ProviderSetupItem {
                    name: "Roles".to_string(),
                    detail: "no active routing profile".to_string(),
                    id: None,
                    provider_id: None,
                    target: None,
                    models: Vec::new(),
                    readiness: SetupReadinessState::MissingAuthority(
                        "active routing profile is unavailable".to_string(),
                    ),
                }]
            });

        Self {
            providers,
            accounts: account_items,
            connections: connection_items,
            roles,
            saved_profile_id: profiles
                .and_then(|registry| registry.read().ok())
                .and_then(|registry| registry.active().ok().map(|profile| profile.id.to_string())),
        }
    }
}

fn directory_names(
    directory: &RoutingConnectionDirectory,
    accounts: Option<&CodexAccountProfileRegistry>,
) -> Vec<(String, SetupReadinessState, String)> {
    accounts
        .map(|registry| {
            registry
                .profiles()
                .filter_map(|profile| {
                    (directory.provider_id_for(&profile.connection_id) == Some("codex")).then(
                        || {
                            let ready = profile.enabled
                                && profile.state == CodexAccountProfileState::Connected
                                && profile.validation == ConnectionValidationStatus::Valid
                                && profile.account_id.is_some();
                            (
                                profile.label.clone(),
                                if ready {
                                    SetupReadinessState::Ready
                                } else {
                                    SetupReadinessState::Invalid(
                                        "Codex authentication is not usable".to_string(),
                                    )
                                },
                                profile.connection_id.clone(),
                            )
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "provider_setup_tests.rs"]
mod tests;
