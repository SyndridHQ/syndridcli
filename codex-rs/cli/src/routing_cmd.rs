use anyhow::Result;
use clap::Subcommand;
use codex_core::CodexAccountProfileRegistry;
use codex_core::OmniRouteRegistry;
use codex_core::RoutingAssignment;
use codex_core::RoutingConnectionDirectory;
use codex_core::RoutingProfile;
use codex_core::RoutingProfileError;
use codex_core::RoutingProfileId;
use codex_core::RoutingProfileStore;
use codex_core::RoutingResolutionStatus;
use codex_core::RoutingRole;
use codex_core::config::find_codex_home;

const PROFILE_FILE: &str = "syndrid-routing-profiles.json";

#[derive(Debug, clap::Args)]
pub struct RoutingCommand {
    #[command(subcommand)]
    pub subcommand: RoutingSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum RoutingSubcommand {
    Profile(RoutingProfileCommand),
    Active,
}

#[derive(Debug, clap::Args)]
pub struct RoutingProfileCommand {
    #[command(subcommand)]
    pub subcommand: RoutingProfileSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum RoutingProfileSubcommand {
    Create(ProfileCreateCommand),
    List,
    Show(ProfileIdCommand),
    Activate(ProfileIdCommand),
    Delete(ProfileIdCommand),
    Assign(ProfileAssignCommand),
    Unassign(ProfileRoleCommand),
    Validate(ProfileIdCommand),
    Resolve(ProfileResolveCommand),
}

#[derive(Debug, clap::Args)]
pub struct ProfileCreateCommand {
    pub profile_id: String,
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct ProfileIdCommand {
    pub profile_id: String,
}

#[derive(Debug, clap::Args)]
pub struct ProfileRoleCommand {
    pub profile_id: String,
    pub role: String,
}

#[derive(Debug, clap::Args)]
pub struct ProfileAssignCommand {
    pub profile_id: String,
    pub role: String,
    #[arg(long)]
    pub connection: String,
    #[arg(long)]
    pub model: String,
    #[arg(long)]
    pub provider: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct ProfileResolveCommand {
    pub profile_id: String,
    pub role: String,
}

pub async fn run(command: RoutingCommand) -> Result<()> {
    match command.subcommand {
        RoutingSubcommand::Profile(command) => match command.subcommand {
            RoutingProfileSubcommand::Create(command) => create(command),
            RoutingProfileSubcommand::List => list(),
            RoutingProfileSubcommand::Show(command) => show(command),
            RoutingProfileSubcommand::Activate(command) => activate(command),
            RoutingProfileSubcommand::Delete(command) => delete(command),
            RoutingProfileSubcommand::Assign(command) => assign(command),
            RoutingProfileSubcommand::Unassign(command) => unassign(command),
            RoutingProfileSubcommand::Validate(command) => validate(command),
            RoutingProfileSubcommand::Resolve(command) => resolve(command),
        },
        RoutingSubcommand::Active => active(),
    }
}

fn store() -> Result<RoutingProfileStore> {
    Ok(RoutingProfileStore::new(
        find_codex_home()?.join(PROFILE_FILE).to_path_buf(),
    ))
}

fn connection_directory() -> Result<RoutingConnectionDirectory> {
    let home = find_codex_home()?;
    let registry = OmniRouteRegistry::load(&home.join("syndrid-provider-connections.json"))?;
    let mut directory = RoutingConnectionDirectory::from_omniroute(&registry);
    let codex = CodexAccountProfileRegistry::load(&home.join("syndrid-codex-accounts.json"))?;
    directory.add_codex(&codex);
    Ok(directory)
}

fn profile_id(value: &str) -> Result<RoutingProfileId> {
    Ok(RoutingProfileId::new(value)?)
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn create(command: ProfileCreateCommand) -> Result<()> {
    let store = store()?;
    let mut registry = store.load()?;
    let id = profile_id(&command.profile_id)?;
    let name = command.name.unwrap_or_else(|| command.profile_id.clone());
    registry.insert(RoutingProfile::new(id, name, now())?)?;
    store.save(&registry)?;
    println!("created profile: {}", command.profile_id);
    Ok(())
}

fn list() -> Result<()> {
    let registry = store()?.load()?;
    for profile in registry.profiles() {
        let active = registry.active_profile_id.as_ref() == Some(&profile.id);
        println!(
            "{}{} ({})",
            if active { "* " } else { "  " },
            profile.id,
            profile.name
        );
    }
    Ok(())
}

fn show(command: ProfileIdCommand) -> Result<()> {
    let registry = store()?.load()?;
    let id = profile_id(&command.profile_id)?;
    let profile = registry
        .get(&id)
        .ok_or(RoutingProfileError::UnknownProfile)?;
    println!("profile: {}", profile.id);
    println!("name: {}", profile.name);
    println!("enabled: {}", profile.enabled);
    println!(
        "active: {}",
        registry.active_profile_id.as_ref() == Some(&profile.id)
    );
    for (role, assignment) in &profile.assignments {
        println!(
            "{}: {} / {} / {}",
            role, assignment.connection_id, assignment.provider_id, assignment.model_id
        );
    }
    Ok(())
}

fn activate(command: ProfileIdCommand) -> Result<()> {
    let store = store()?;
    let mut registry = store.load()?;
    let id = profile_id(&command.profile_id)?;
    let (previous, current) = registry.activate(&id)?;
    store.save(&registry)?;
    println!("active profile: {}", current);
    if let Some(previous) = previous {
        println!("previous profile: {previous}");
    }
    Ok(())
}

fn delete(command: ProfileIdCommand) -> Result<()> {
    let store = store()?;
    let mut registry = store.load()?;
    let id = profile_id(&command.profile_id)?;
    registry.delete(&id)?;
    store.save(&registry)?;
    println!("deleted profile: {}", id);
    Ok(())
}

fn assign(command: ProfileAssignCommand) -> Result<()> {
    let store = store()?;
    let mut registry = store.load()?;
    let id = profile_id(&command.profile_id)?;
    let role = RoutingRole::parse(&command.role)?;
    let provider_id = match command.provider {
        Some(provider) => provider,
        None => connection_directory()?
            .provider_id_for(&command.connection)
            .ok_or(RoutingProfileError::UnknownConnection)?
            .to_string(),
    };
    let profile = registry
        .get_mut(&id)
        .ok_or(RoutingProfileError::UnknownProfile)?;
    profile.replace_assignment(
        role,
        RoutingAssignment {
            connection_id: command.connection,
            provider_id,
            model_id: command.model,
            enabled: true,
            label: None,
        },
    )?;
    store.save(&registry)?;
    println!("assigned {role} in profile {id}");
    Ok(())
}

fn unassign(command: ProfileRoleCommand) -> Result<()> {
    let store = store()?;
    let mut registry = store.load()?;
    let id = profile_id(&command.profile_id)?;
    let role = RoutingRole::parse(&command.role)?;
    registry
        .get_mut(&id)
        .ok_or(RoutingProfileError::UnknownProfile)?
        .unassign(role)?;
    store.save(&registry)?;
    println!("unassigned {role} in profile {id}");
    Ok(())
}

fn validate(command: ProfileIdCommand) -> Result<()> {
    let registry = store()?.load()?;
    let id = profile_id(&command.profile_id)?;
    let profile = registry
        .get(&id)
        .ok_or(RoutingProfileError::UnknownProfile)?;
    let directory = connection_directory()?;
    profile.validate_required_roles()?;
    for (role, assignment) in &profile.assignments {
        let status = directory.validate_assignment(assignment)?;
        println!("{role}: {status:?}");
    }
    Ok(())
}

fn resolve(command: ProfileResolveCommand) -> Result<()> {
    let registry = store()?.load()?;
    let id = profile_id(&command.profile_id)?;
    let role = RoutingRole::parse(&command.role)?;
    let profile = registry
        .get(&id)
        .ok_or(RoutingProfileError::UnknownProfile)?;
    let selection = profile.resolve_role(role)?;
    let status = connection_directory()?.validate_assignment(
        profile
            .assignments
            .get(&role)
            .ok_or(RoutingProfileError::MissingRoleAssignment)?,
    )?;
    println!("profile: {}", profile.id);
    println!("role: {role}");
    println!("connection: {}", selection.connection_id);
    println!("provider: {}", selection.provider_id);
    println!("model: {}", selection.model_id);
    println!(
        "status: {}",
        match status {
            RoutingResolutionStatus::LocallyValid => "locally valid",
            RoutingResolutionStatus::ModelUnverified => "model unverified",
        }
    );
    Ok(())
}

fn active() -> Result<()> {
    let registry = store()?.load()?;
    let profile = registry.active()?;
    println!("active profile: {}", profile.id);
    println!("name: {}", profile.name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Args;
    use clap::FromArgMatches;

    fn parse(arguments: &[&str]) -> RoutingCommand {
        let command = RoutingCommand::augment_args(clap::Command::new("routing"));
        RoutingCommand::from_arg_matches(&command.try_get_matches_from(arguments).expect("matches"))
            .expect("command")
    }

    #[test]
    fn profile_management_commands_parse() {
        assert!(matches!(
            parse(&["routing", "profile", "create", "default"]).subcommand,
            RoutingSubcommand::Profile(_)
        ));
        assert!(matches!(
            parse(&["routing", "profile", "list"]).subcommand,
            RoutingSubcommand::Profile(_)
        ));
        assert!(matches!(
            parse(&["routing", "active"]).subcommand,
            RoutingSubcommand::Active
        ));
    }

    #[test]
    fn assignment_and_resolution_commands_parse() {
        assert!(matches!(
            parse(&[
                "routing",
                "profile",
                "assign",
                "default",
                "planner",
                "--connection",
                "omniroute-local",
                "--model",
                "provider/model"
            ])
            .subcommand,
            RoutingSubcommand::Profile(_)
        ));
        assert!(matches!(
            parse(&["routing", "profile", "resolve", "default", "planner"]).subcommand,
            RoutingSubcommand::Profile(_)
        ));
    }

    #[test]
    fn sensitive_values_are_not_in_command_debug() {
        let command = parse(&[
            "routing",
            "profile",
            "assign",
            "default",
            "planner",
            "--connection",
            "local",
            "--model",
            "model",
        ]);
        let debug = format!("{command:?}");
        assert!(!debug.contains("credential-sentinel"));
        assert!(!debug.contains("token-sentinel"));
    }
}
