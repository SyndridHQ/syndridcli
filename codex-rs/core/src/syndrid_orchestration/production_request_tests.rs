use super::SubagentProvider;
use super::codex_accounts::CodexAccountProfileRegistry;
use super::codex_invocation::CodexInvocationAdapter;
use super::codex_invocation::UnavailableCodexInvocationClient;
use super::execution_modes::ExecutionModeSelection;
use super::invocation::ProviderInvocationRequest;
use super::live_coordinator_types::PlanningContract;
use super::live_coordinator_types::VerificationContract;
use super::omniroute::ProviderSelection;
use super::production_request::ProductionOrchestrationInput;
use super::production_request::ProductionOrchestrationRequestBuilder;
use super::production_request::ProductionProviderAdapter;
use super::production_request::ProductionProviderRoute;
use super::production_request::ProductionRequestError;
use super::provider_connection::ConnectionValidationStatus;
use super::routing_profiles::RoutingAssignment;
use super::routing_profiles::RoutingConnectionDirectory;
use super::routing_profiles::RoutingConnectionInfo;
use super::routing_profiles::RoutingProfile;
use super::routing_profiles::RoutingProfileId;
use super::routing_profiles::RoutingProfileRegistry;
use super::routing_profiles::RoutingRole;
use super::subagent_batch::SubagentFailurePolicy;
use super::subagent_tools::ProductionApprovedToolAdapter;
use super::subagent_tools::SubagentToolKind;
use super::subagent_tools::SubagentToolPolicy;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;

fn profile_and_connections() -> (
    RoutingProfileRegistry,
    RoutingConnectionDirectory,
    RoutingProfileId,
) {
    let profile_id = RoutingProfileId::new("production-test").expect("profile ID");
    let mut profile =
        RoutingProfile::new(profile_id.clone(), "Production test", 1).expect("profile");
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
                    connection_id: "connection-1".to_string(),
                    provider_id: "codex".to_string(),
                    model_id: "gpt-test".to_string(),
                    enabled: true,
                    label: None,
                },
            )
            .expect("assignment");
    }
    let mut profiles = RoutingProfileRegistry::default();
    profiles.insert(profile).expect("profile insert");
    let mut connections = RoutingConnectionDirectory::default();
    connections.insert(RoutingConnectionInfo {
        connection_id: "connection-1".to_string(),
        provider_id: "codex".to_string(),
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        authentication_supported: true,
        models: Some(vec!["gpt-test".to_string()]),
    });
    (profiles, connections, profile_id)
}

fn input(root: &Path, policy: SubagentToolPolicy) -> ProductionOrchestrationInput {
    ProductionOrchestrationInput {
        run_id: "run-1".to_string(),
        instruction: "inspect the workspace".to_string(),
        context: Some("bounded context".to_string()),
        workspace_root: root.to_path_buf(),
        tasks: Vec::new(),
        planning: PlanningContract::NotRequested,
        verification: VerificationContract::NotRequested,
        failure_policy: SubagentFailurePolicy::ContinueIndependent,
        repair_instruction: String::new(),
        approved_tool_policy: policy,
        cancellation: CancellationToken::new(),
        overall_timeout: None,
    }
}

#[test]
fn valid_resolved_state_builds_a_request_without_mutable_state() {
    let root = tempdir().expect("tempdir");
    let policy = SubagentToolPolicy::for_workspace(root.path(), Default::default()).expect("tools");
    let (profiles, connections, profile_id) = profile_and_connections();
    let builder = ProductionOrchestrationRequestBuilder::new(
        ExecutionModeSelection::Balanced,
        profile_id.clone(),
        profiles,
        connections,
    )
    .expect("builder");
    let request = builder.build(input(root.path(), policy)).expect("request");
    assert_eq!(request.run_id, "run-1");
    assert_eq!(request.routing_profile_id, Some(profile_id));
    assert_eq!(request.instruction, "inspect the workspace");
    assert!(request.policy.is_some());
    assert_eq!(
        builder
            .provider_selection(RoutingRole::Main)
            .expect("main route"),
        ProviderSelection::new("connection-1", "codex", "gpt-test").expect("route")
    );
    let route = builder
        .provider_route(RoutingRole::Main)
        .expect("main provider route");
    assert_eq!(
        route.selection(),
        &ProviderSelection::new("connection-1", "codex", "gpt-test").expect("route")
    );
    assert_eq!(
        route.effort(),
        codex_protocol::openai_models::ReasoningEffort::Medium
    );
}

#[test]
fn missing_profile_and_invalid_custom_policy_are_rejected() {
    let root = tempdir().expect("tempdir");
    let missing = ProductionOrchestrationRequestBuilder::new(
        ExecutionModeSelection::Balanced,
        RoutingProfileId::new("missing").expect("profile ID"),
        RoutingProfileRegistry::default(),
        RoutingConnectionDirectory::default(),
    );
    assert!(matches!(
        missing,
        Err(ProductionRequestError::InvalidRoutingProfile(_))
    ));

    let invalid = ExecutionModeSelection::custom(super::execution_modes::ExecutionPolicy {
        roles: BTreeMap::new(),
        max_subagents: 0,
        max_concurrency: 0,
        max_provider_invocations: 0,
        max_tool_calls: 0,
        max_tool_output_bytes: 0,
        max_repair_attempts: 0,
        task_timeout: std::time::Duration::ZERO,
        batch_timeout: std::time::Duration::ZERO,
        repair_timeout: std::time::Duration::ZERO,
        context_budget_bytes: 0,
        output_budget_tokens: 0,
        max_final_response_tokens: 0,
        optional_roles_may_skip: false,
        shape: super::execution_modes::ExecutionShape::SinglePass,
    });
    let (profiles, connections, profile_id) = profile_and_connections();
    let result =
        ProductionOrchestrationRequestBuilder::new(invalid, profile_id, profiles, connections);
    assert!(matches!(
        result,
        Err(ProductionRequestError::InvalidExecutionPolicy(_))
    ));
    let _ = root;
}

#[tokio::test]
async fn provider_adapter_rejects_route_mismatch_without_fallback() {
    let selection = ProviderSelection::new("connection-1", "codex", "gpt-test").expect("route");
    let provider = CodexInvocationAdapter::new(
        selection.clone(),
        CodexAccountProfileRegistry::default(),
        UnavailableCodexInvocationClient,
    )
    .expect("provider");
    let route = ProductionProviderRoute::new(
        selection,
        codex_protocol::openai_models::ReasoningEffort::Medium,
    );
    let adapter = ProductionProviderAdapter::new(route, provider).expect("adapter");
    let request = ProviderInvocationRequest {
        provider: "openrouter".to_string(),
        model: "other-model".to_string(),
        system: None,
        user: "bounded".to_string(),
        max_output_tokens: 16,
        tools: Vec::new(),
        tool_results: Vec::new(),
    };
    assert_eq!(
        <ProductionProviderAdapter<_> as SubagentProvider>::invoke(
            &adapter,
            request,
            CancellationToken::new(),
        )
        .await,
        Err(super::ProviderInvocationError::InvalidRequest)
    );
}

#[tokio::test]
async fn approved_tool_adapter_preserves_o6b_workspace_and_allowlist() {
    let root = tempdir().expect("tempdir");
    fs::write(root.path().join("note.txt"), "safe").expect("file");
    let policy = SubagentToolPolicy::for_workspace(root.path(), Default::default())
        .expect("policy")
        .approve(SubagentToolKind::ReadFile);
    let adapter = ProductionApprovedToolAdapter::new(policy);
    let result = adapter
        .execute(
            SubagentToolKind::ReadFile,
            "call-1",
            r#"{"path":"note.txt"}"#,
            &CancellationToken::new(),
        )
        .await
        .expect("tool result");
    assert_eq!(result.content, "1: safe");
    assert!(matches!(
        adapter
            .execute(
                SubagentToolKind::GitStatus,
                "call-2",
                "{}",
                &CancellationToken::new(),
            )
            .await,
        Err(super::subagent_tools::SubagentToolError::ToolNotApproved)
    ));
}
