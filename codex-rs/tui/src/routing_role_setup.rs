//! Candidate-only routing role identity-source and pool selection helpers.

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::SelectionItem;
use crate::legacy_core::AccountPoolProviderFamily;
use crate::legacy_core::PoolId;
use crate::legacy_core::PoolMemberReadiness;
use crate::legacy_core::PoolReadiness;
use crate::legacy_core::RoutingProfile;
use crate::legacy_core::RoutingRole;
use crate::pool_setup::PoolSetupSnapshot;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IdentitySourceChoice {
    Direct,
    NamedPool,
}

impl IdentitySourceChoice {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Direct => "Direct",
            Self::NamedPool => "Named pool",
        }
    }
}

pub(crate) fn identity_source_items(
    profile: Option<&RoutingProfile>,
    role: RoutingRole,
    selected_source: Option<IdentitySourceChoice>,
) -> Vec<SelectionItem> {
    let current = selected_source.or_else(|| {
        profile
            .and_then(|profile| profile.assignments.get(&role))
            .map(|assignment| {
                if assignment.pool_id.is_some() {
                    IdentitySourceChoice::NamedPool
                } else {
                    IdentitySourceChoice::Direct
                }
            })
    });
    [
        IdentitySourceChoice::Direct,
        IdentitySourceChoice::NamedPool,
    ]
    .into_iter()
    .map(|source| SelectionItem {
        name: source.label().to_string(),
        description: Some(match source {
            IdentitySourceChoice::Direct => {
                "Choose an exact account or connection below.".to_string()
            }
            IdentitySourceChoice::NamedPool => {
                "Use the pool's explicitly selected member; no rotation or fallback.".to_string()
            }
        }),
        is_current: current == Some(source),
        actions: vec![Box::new(move |tx: &AppEventSender| {
            tx.send(AppEvent::UpdateOrchestrationSetupIdentitySource { role, source });
        }) as crate::bottom_pane::SelectionAction],
        dismiss_on_select: false,
        ..Default::default()
    })
    .collect()
}

pub(crate) fn pool_selection_items(
    profile: Option<&RoutingProfile>,
    role: RoutingRole,
    snapshot: &PoolSetupSnapshot,
) -> Vec<SelectionItem> {
    let Some(assignment) = profile.and_then(|profile| profile.assignments.get(&role)) else {
        return vec![SelectionItem {
            name: "No role binding".to_string(),
            description: Some("Select a role binding before choosing a pool.".to_string()),
            is_disabled: true,
            ..Default::default()
        }];
    };
    let expected_provider = match assignment.provider_id.as_str() {
        "codex" => AccountPoolProviderFamily::NativeCodex,
        "omniroute" => AccountPoolProviderFamily::OmniRoute,
        _ => {
            return vec![unavailable_pool_item(
                "The role provider has no compatible pools.",
            )];
        }
    };
    let mut items: Vec<SelectionItem> = snapshot
        .summaries
        .iter()
        .filter(|summary| summary.provider == expected_provider)
        .map(|summary| {
            let pool_id = summary.id.clone();
            let round_robin = summary.is_round_robin;
            let selected_key = (
                summary.id.clone(),
                crate::legacy_core::PoolMemberId::new(summary.selected.clone()).ok(),
            );
            let selected_label = selected_key
                .1
                .and_then(|member_id| {
                    snapshot
                        .member_labels
                        .get(&(selected_key.0.clone(), member_id))
                })
                .cloned()
                .unwrap_or_else(|| {
                    if round_robin {
                        "selected at first role use per turn".to_string()
                    } else {
                        "selected member unavailable".to_string()
                    }
                });
            let selectable = summary.readiness == PoolReadiness::Ready;
            let degraded_count = snapshot
                .member_statuses
                .iter()
                .filter(|((pool_id, member_id), status)| {
                    pool_id == &summary.id
                        && member_id.as_str() != summary.selected
                        && **status != PoolMemberReadiness::Ready
                })
                .count();
            let is_current = assignment.pool_id.as_ref() == Some(&summary.id);
            let description = format!(
                "{} · {} members · {} · {}",
                summary.display_name,
                summary.member_count,
                if round_robin {
                    "round robin · active at role admission".to_string()
                } else {
                    format!("explicit member {} ({selected_label})", summary.selected)
                },
                if selectable && degraded_count == 0 {
                    "Ready".to_string()
                } else if selectable {
                    format!("Ready · degraded: {degraded_count} nonselected member(s)")
                } else {
                    "Needs attention".to_string()
                },
            );
            let actions = selectable
                .then(|| {
                    vec![Box::new(move |tx: &AppEventSender| {
                        tx.send(AppEvent::UpdateOrchestrationSetupPool {
                            role,
                            pool_id: pool_id.clone(),
                        });
                    })
                        as crate::bottom_pane::SelectionAction]
                })
                .unwrap_or_default();
            SelectionItem {
                name: format!(
                    "{} · {}",
                    summary.id,
                    if selectable {
                        "Ready"
                    } else {
                        "Needs attention"
                    }
                ),
                description: Some(description),
                is_current,
                is_disabled: !selectable,
                disabled_reason: (!selectable).then_some(
                    "The pool is structurally invalid or its configured target is unavailable."
                        .to_string(),
                ),
                actions,
                dismiss_on_select: false,
                ..Default::default()
            }
        })
        .collect();
    if let Some(pool_id) = assignment.pool_id.as_ref()
        && !snapshot
            .summaries
            .iter()
            .any(|summary| summary.id == *pool_id)
    {
        items.push(SelectionItem {
            name: format!("{} · Needs attention", pool_id),
            description: Some(format!(
                "Pool {pool_id} is missing from the current registry."
            )),
            is_current: true,
            is_disabled: true,
            disabled_reason: Some("The referenced pool is unavailable.".to_string()),
            ..Default::default()
        });
    } else if let Some(pool_id) = assignment.pool_id.as_ref()
        && snapshot
            .summaries
            .iter()
            .find(|summary| summary.id == *pool_id)
            .is_some_and(|summary| summary.provider != expected_provider)
    {
        items.push(SelectionItem {
            name: format!("{} · Needs attention", pool_id),
            description: Some("The referenced pool has an incompatible provider.".to_string()),
            is_current: true,
            is_disabled: true,
            disabled_reason: Some("Choose a pool matching the role provider.".to_string()),
            ..Default::default()
        });
    }
    if items.is_empty() {
        items.push(unavailable_pool_item(
            "No provider-compatible pools are configured.",
        ));
    }
    items
}

fn unavailable_pool_item(reason: &str) -> SelectionItem {
    SelectionItem {
        name: "No selectable pool".to_string(),
        description: Some(reason.to_string()),
        is_disabled: true,
        disabled_reason: Some(reason.to_string()),
        ..Default::default()
    }
}

pub(crate) fn set_identity_source(
    profile: &mut RoutingProfile,
    role: RoutingRole,
    _source: IdentitySourceChoice,
) -> Result<(), String> {
    let Some(assignment) = profile.assignments.get_mut(&role) else {
        return Err("The selected role has no editable binding.".to_string());
    };
    assignment.connection_id.clear();
    assignment.pool_id = None;
    Ok(())
}

pub(crate) fn set_pool_selection(
    profile: &mut RoutingProfile,
    role: RoutingRole,
    pool_id: PoolId,
) -> Result<(), String> {
    let Some(mut assignment) = profile.assignments.get(&role).cloned() else {
        return Err("The selected role has no editable binding.".to_string());
    };
    assignment.connection_id.clear();
    assignment.pool_id = Some(pool_id);
    profile
        .replace_assignment(role, assignment)
        .map_err(|error| error.to_string())
}
