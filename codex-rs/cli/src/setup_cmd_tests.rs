use super::*;
use clap::Parser;
use codex_core::RoutingProfileRegistry;

#[derive(Debug, Parser)]
struct TestCli {
    #[command(subcommand)]
    command: TestSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum TestSubcommand {
    Setup(SetupCommand),
}

fn context() -> SetupContext {
    SetupContext {
        connections: vec![SetupConnection {
            provider: "omniroute".to_string(),
            id: "local".to_string(),
            label: "Local".to_string(),
            enabled: true,
            validation: ConnectionValidationStatus::Valid,
            models: Some(vec!["provider/model".to_string()]),
            state: None,
            credential_reference: None,
        }],
        profiles: RoutingProfileRegistry::default(),
        active_profile: None,
    }
}

fn context_with_profile() -> (SetupContext, RoutingProfileId) {
    let mut context = context();
    let source_id = RoutingProfileId::new("source").expect("source ID");
    let source = build_profile(
        &context,
        source_id.clone(),
        None,
        &context.connections[0],
        "provider/model",
        false,
    )
    .expect("source profile");
    context.profiles.insert(source).expect("insert source");
    (context, source_id)
}

#[test]
fn setup_command_parses_supported_options() {
    let parsed = TestCli::try_parse_from([
        "syndrid",
        "setup",
        "--profile",
        "default",
        "--non-interactive",
        "--provider",
        "omniroute",
        "--connection",
        "local",
        "--model",
        "provider/model",
        "--yes",
        "--activate",
        "--repair",
    ])
    .expect("setup command");
    let TestSubcommand::Setup(command) = parsed.command;
    assert_eq!(command.profile.as_deref(), Some("default"));
    assert!(command.non_interactive);
    assert!(command.yes);
    assert!(command.activate);
    assert!(command.repair);
}

#[test]
fn non_interactive_setup_requires_explicit_options() {
    let result = run_non_interactive(
        SetupCommand {
            profile: None,
            non_interactive: true,
            provider: None,
            connection: None,
            model: None,
            activate: false,
            yes: false,
            repair: false,
            duplicate_from: None,
            display_name: None,
            validate_only: false,
            readiness: false,
            check_credentials: false,
            json: false,
        },
        context(),
        &mut ScriptedPrompt::new([]),
    );
    assert!(matches!(result, Err(error) if error.to_string().contains("required")));
}

#[test]
fn profile_validation_uses_exact_connection_and_model_without_credentials() {
    let context = context();
    let profile = build_profile(
        &context,
        RoutingProfileId::new("default").expect("profile ID"),
        None,
        &context.connections[0],
        "provider/model",
        false,
    )
    .expect("profile");
    assert_eq!(profile.assignments.len(), 4);
    assert!(!profile.assignments.contains_key(&RoutingRole::Repair));
    validate_profile(&context, &profile).expect("local validation");
}

#[test]
fn scripted_prompt_cancellation_is_terminal() {
    let mut prompt = ScriptedPrompt::new(["cancel"]);
    let result = prompt.input("profile", None);
    assert!(matches!(result, Err(error) if error.to_string() == "setup cancelled"));
}

#[test]
fn duplicate_profile_copies_assignments_without_replacing_source() {
    let (context, source_id) = context_with_profile();
    let destination_id = RoutingProfileId::new("copy").expect("destination ID");
    let duplicate = duplicate_profile(
        &context,
        &source_id,
        destination_id.clone(),
        Some("Copied profile"),
    )
    .expect("duplicate");
    assert_eq!(duplicate.name, "Copied profile");
    assert_eq!(
        duplicate.assignments,
        context.profiles.get(&source_id).unwrap().assignments
    );
    assert!(!duplicate.assignments.contains_key(&RoutingRole::Repair));
    assert!(context.profiles.get(&destination_id).is_none());
}

#[test]
fn duplicate_profile_rejects_collision_and_invalid_source() {
    let (context, source_id) = context_with_profile();
    assert!(matches!(
        duplicate_profile(&context, &source_id, source_id.clone(), None),
        Err(error) if error.to_string().contains("already exists")
    ));
    let missing = RoutingProfileId::new("missing").expect("missing ID");
    let destination = RoutingProfileId::new("copy").expect("destination ID");
    assert!(matches!(
        duplicate_profile(&context, &missing, destination, None),
        Err(error) if error.to_string().contains("not found")
    ));
}

#[test]
fn profile_report_requires_required_roles_and_allows_repair_omission() {
    let context = context();
    let profile = RoutingProfile::new(
        RoutingProfileId::new("partial").expect("profile ID"),
        "partial",
        1,
    )
    .expect("profile");
    let (reports, _, valid) = profile_report(&context, &profile);
    assert!(!valid);
    assert_eq!(reports[0].status, "missing_assignment");
    assert_eq!(reports[4].status, "not_assigned");
}

#[test]
fn model_unverified_is_a_warning_and_json_is_bounded() {
    let mut context = context();
    context.connections[0].models = None;
    let profile = build_profile(
        &context,
        RoutingProfileId::new("default").expect("profile ID"),
        None,
        &context.connections[0],
        "provider/model",
        false,
    )
    .expect("profile");
    let (_, warnings, valid) = profile_report(&context, &profile);
    assert!(valid);
    assert_eq!(warnings.len(), 4);
}

#[test]
fn validation_flags_reject_mutating_options() {
    let command = SetupCommand {
        profile: Some("default".to_string()),
        non_interactive: false,
        provider: None,
        connection: Some("local".to_string()),
        model: None,
        activate: false,
        yes: false,
        repair: false,
        duplicate_from: None,
        display_name: None,
        validate_only: true,
        readiness: false,
        check_credentials: false,
        json: false,
    };
    assert!(
        matches!(validate_flags(&command), Err(error) if error.to_string().contains("cannot be combined"))
    );
}
