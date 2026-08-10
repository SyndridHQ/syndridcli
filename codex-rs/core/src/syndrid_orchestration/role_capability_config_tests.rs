use super::*;
use crate::ExecutionModeSelection;
use crate::SubagentToolKind;
use pretty_assertions::assert_eq;
use std::collections::BTreeSet;
use std::fs;
use tempfile::TempDir;

fn policy() -> ResolvedExecutionPolicy {
    ExecutionModeSelection::Balanced.resolve().expect("policy")
}

fn context() -> RoleCapabilityValidationContext {
    RoleCapabilityValidationContext::new(
        std::env::current_dir().expect("workspace"),
        SubagentToolKind::all().into_iter().collect::<BTreeSet<_>>(),
        false,
        false,
    )
}

fn write(home: &TempDir, contents: &str) {
    fs::write(home.path().join(ROLE_CAPABILITY_FILE), contents).expect("configuration");
}

fn no_tools_configuration() -> String {
    r#"{
      "schema_version": 1,
      "planner": {"mode": "no_tools"},
      "executor": {"mode": "no_tools"},
      "verifier": {"mode": "no_tools"},
      "repair": {"mode": "no_tools"}
    }"#
    .to_string()
}

#[test]
fn loads_canonical_no_tools_configuration() {
    let home = tempfile::tempdir().expect("home");
    write(&home, &no_tools_configuration());
    let loaded = load_role_capabilities(home.path(), &policy(), &context()).expect("capabilities");
    assert_eq!(loaded.roles().count(), 4);
    assert_eq!(
        loaded.get(RoutingRole::Planner).unwrap().max_tool_calls(),
        0
    );
}

#[test]
fn missing_and_invalid_storage_are_typed() {
    let home = tempfile::tempdir().expect("home");
    assert_eq!(
        load_role_capabilities(home.path(), &policy(), &context()),
        Err(RoleCapabilityConfigError::Unavailable)
    );

    write(
        &home,
        &no_tools_configuration().replace("\"schema_version\": 1", "\"schema_version\": 2"),
    );
    assert_eq!(
        load_role_capabilities(home.path(), &policy(), &context()),
        Err(RoleCapabilityConfigError::UnsupportedVersion)
    );

    write(&home, "not-json");
    assert_eq!(
        load_role_capabilities(home.path(), &policy(), &context()),
        Err(RoleCapabilityConfigError::Malformed)
    );
}

#[test]
fn oversized_storage_is_rejected_before_parsing() {
    let home = tempfile::tempdir().expect("home");
    let contents = vec![b' '; MAX_ROLE_CAPABILITY_FILE_BYTES as usize + 1];
    fs::write(home.path().join(ROLE_CAPABILITY_FILE), contents).expect("configuration");
    assert_eq!(
        load_role_capabilities(home.path(), &policy(), &context()),
        Err(RoleCapabilityConfigError::Oversized)
    );
}

#[test]
fn explicit_declaration_uses_session_scope_and_core_validation() {
    let home = tempfile::tempdir().expect("home");
    write(
        &home,
        r#"{
          "schema_version": 1,
          "planner": {
            "mode": "explicit",
            "tool_names": ["read_file"],
            "workspace": "session",
            "shell": "prohibited",
            "network": "prohibited",
            "max_output_bytes": 1024,
            "max_tool_calls": 1,
            "approval": "pre_authorized"
          },
          "executor": {"mode": "no_tools"},
          "verifier": {"mode": "no_tools"},
          "repair": {"mode": "no_tools"}
        }"#,
    );
    let loaded = load_role_capabilities(home.path(), &policy(), &context()).expect("capabilities");
    assert_eq!(
        loaded.get(RoutingRole::Planner).unwrap().max_tool_calls(),
        1
    );
    assert_eq!(
        loaded
            .get(RoutingRole::Planner)
            .unwrap()
            .tool_policy()
            .workspace_root(),
        Some(context().workspace_root())
    );
}
