//! Redacted provider, account, connection, and role readiness for `/setup`.
//!
//! This module is a presentation boundary over the canonical Syndrid registries. It deliberately
//! carries display metadata only; it never owns credentials, provider clients, or runtime state.

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
    pub(crate) readiness: SetupReadinessState,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub(crate) struct ProviderSetupSnapshot {
    pub(crate) providers: Vec<ProviderSetupItem>,
    pub(crate) accounts: Vec<ProviderSetupItem>,
    pub(crate) connections: Vec<ProviderSetupItem>,
    pub(crate) roles: Vec<ProviderSetupItem>,
}

impl ProviderSetupSnapshot {
    pub(crate) fn unavailable() -> Self {
        Self {
            providers: vec![
                ProviderSetupItem {
                    name: "Native Codex".to_string(),
                    detail: "authentication metadata is unavailable".to_string(),
                    readiness: SetupReadinessState::MissingAuthority(
                        "Codex account authority is unavailable".to_string(),
                    ),
                },
                ProviderSetupItem {
                    name: "OmniRoute".to_string(),
                    detail: "connection metadata is unavailable".to_string(),
                    readiness: SetupReadinessState::MissingAuthority(
                        "OmniRoute connection authority is unavailable".to_string(),
                    ),
                },
                ProviderSetupItem {
                    name: "OpenRouter".to_string(),
                    detail: "production provider integration is not implemented".to_string(),
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
            },
            ProviderSetupItem {
                name: "OpenRouter".to_string(),
                detail: "production provider integration is not implemented".to_string(),
                readiness: SetupReadinessState::Unavailable(
                    "OpenRouter production provider integration is not implemented yet".to_string(),
                ),
            },
        ];

        let roles = profiles
            .and_then(|registry| registry.active().ok())
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
                        readiness,
                    }
                })
                .collect()
            })
            .unwrap_or_else(|| {
                vec![ProviderSetupItem {
                    name: "Roles".to_string(),
                    detail: "no active routing profile".to_string(),
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
        }
    }
}

fn directory_names(
    directory: &RoutingConnectionDirectory,
    accounts: Option<&CodexAccountProfileRegistry>,
) -> Vec<(String, SetupReadinessState)> {
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
