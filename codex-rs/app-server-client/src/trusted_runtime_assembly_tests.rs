use super::*;
use crate::TrustedApprovedToolSnapshot;
use crate::TrustedCompositionSnapshotError;
use crate::TrustedProductionProviderAuthority;
use crate::TrustedRoutingSnapshot;
use codex_app_server::ObjectiveOnlyProductionTurnContext;
use codex_core::CodexAccountConnectionMetadata;
use codex_core::CodexAccountProfileId;
use codex_core::CodexAccountProfileRegistry;
use codex_core::CodexAccountProfileState;
use codex_core::ConnectionValidationStatus;
use codex_core::ExecutionModeSelection;
use codex_core::ProviderSelection;
use codex_core::RoleCapabilityConfiguration;
use codex_core::RoleCapabilityDeclaration;
use codex_core::RoleCapabilityValidationContext;
use codex_core::RoutingAssignment;
use codex_core::RoutingConnectionDirectory;
use codex_core::RoutingConnectionInfo;
use codex_core::RoutingProfile;
use codex_core::RoutingProfileId;
use codex_core::RoutingRole;
use codex_core::SessionExecutionPolicyState;
use codex_core::SubagentToolKind;
use codex_core::native_codex_binding;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

struct NoopProviderAuthority;

impl TrustedProductionProviderAuthority for NoopProviderAuthority {
    fn validate_routes(
        &self,
        _routing: &TrustedRoutingSnapshot,
    ) -> Result<(), TrustedCompositionSnapshotError> {
        Ok(())
    }
}

fn account(connection_id: &str) -> CodexAccountConnectionMetadata {
    CodexAccountConnectionMetadata {
        connection_id: connection_id.to_string(),
        profile_id: CodexAccountProfileId::new(connection_id).expect("profile ID"),
        provider_id: codex_core::CODEX_PROVIDER_ID.to_string(),
        label: "test account".to_string(),
        state: CodexAccountProfileState::Connected,
        account_email: None,
        account_id: Some("opaque-account".to_string()),
        plan_label: None,
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        last_authenticated_at: None,
        last_validated_at: None,
        credential_reference: CodexAccountProfileRegistry::credential_reference_for(connection_id)
            .expect("credential reference"),
        schema_version: 1,
    }
}

fn snapshot(root: PathBuf) -> AuthoritativeSyndridCompositionSnapshot {
    let profile_id = RoutingProfileId::new("assembly-test").expect("profile ID");
    let mut profile = RoutingProfile::new(profile_id.clone(), "Assembly test", 1).expect("profile");
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ] {
        profile
            .assign(
                role,
                RoutingAssignment {
                    connection_id: "codex-test".to_string(),
                    provider_id: codex_core::CODEX_PROVIDER_ID.to_string(),
                    model_id: "assembly-model".to_string(),
                    enabled: true,
                    label: None,
                },
            )
            .expect("assignment");
    }
    let mut connections = RoutingConnectionDirectory::default();
    connections.insert(RoutingConnectionInfo {
        connection_id: "codex-test".to_string(),
        provider_id: codex_core::CODEX_PROVIDER_ID.to_string(),
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        authentication_supported: true,
        models: Some(vec!["assembly-model".to_string()]),
    });
    let policy = ExecutionModeSelection::Balanced.resolve().expect("policy");
    let mut accounts = CodexAccountProfileRegistry::default();
    accounts.insert(account("codex-test")).expect("account");
    let mut bindings = BTreeMap::new();
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ] {
        let route = codex_core::ProductionProviderRoute::new(
            ProviderSelection::new("codex-test", "codex", "assembly-model").expect("route"),
            policy.role(role).effort.clone(),
        );
        bindings.insert(
            role,
            native_codex_binding(route, accounts.clone()).expect("binding"),
        );
    }
    let configuration = RoleCapabilityConfiguration::new(
        [
            RoutingRole::Planner,
            RoutingRole::Executor,
            RoutingRole::Verifier,
            RoutingRole::Repair,
        ]
        .into_iter()
        .map(RoleCapabilityDeclaration::no_tools)
        .collect(),
    );
    let capabilities = codex_core::validate_role_capabilities(
        &configuration,
        &policy,
        &RoleCapabilityValidationContext::new(
            root.clone(),
            [SubagentToolKind::ReadFile].into_iter().collect(),
            false,
            false,
        ),
    )
    .expect("capabilities");
    let (event_sender, _event_receiver) = mpsc::channel(1);
    AuthoritativeSyndridCompositionSnapshot {
        session_id: "assembly-session".to_string(),
        policy,
        routing: TrustedRoutingSnapshot {
            profile_id,
            profile,
            connections,
        },
        provider_authority: Arc::new(NoopProviderAuthority),
        provider_construction: codex_core::ProductionProviderConstructionSnapshot::new(bindings),
        approved_tools: TrustedApprovedToolSnapshot::from_validated(capabilities),
        context_provider: Arc::new(ObjectiveOnlyProductionTurnContext),
        workspace_root: root,
        event_sender,
    }
}

#[test]
fn complete_snapshot_assembles_an_inert_session_runtime() {
    let root = tempfile::tempdir().expect("workspace");
    let snapshot = snapshot(root.path().to_path_buf());
    let policy_state = SessionExecutionPolicyState::new().expect("policy state");
    let runtime =
        assemble_trusted_production_runtime(&snapshot, policy_state).expect("runtime assembly");
    assert!(!format!("{runtime:?}").contains("assembly-session"));
}
