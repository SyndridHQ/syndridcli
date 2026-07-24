use crate::provider_cmd::CodexConnectCommand;
use crate::provider_cmd::OmniRouteConnectCommand;
use crate::provider_cmd::ProviderCommand;
use crate::provider_cmd::ProviderConnectCommand;
use crate::provider_cmd::ProviderConnectProvider;
use crate::provider_cmd::ProviderSubcommand;
use anyhow::Result;
use codex_core::CodexAccountProfileRegistry;
use codex_core::CodexAccountProfileState;
use codex_core::ConnectionValidationStatus;
use codex_core::OmniRouteRegistry;
use codex_core::OpenRouterConnectionMetadata;
use codex_core::RoutingAssignment;
use codex_core::RoutingConnectionDirectory;
use codex_core::RoutingConnectionInfo;
use codex_core::RoutingProfile;
use codex_core::RoutingProfileError;
use codex_core::RoutingProfileId;
use codex_core::RoutingProfileRegistry;
use codex_core::RoutingProfileStore;
use codex_core::RoutingResolutionStatus;
use codex_core::RoutingRole;
use codex_core::config::find_codex_home;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::BufRead;
use std::io::IsTerminal;
use std::io::Write;

const PROFILE_FILE: &str = "syndrid-routing-profiles.json";
const PROVIDER_FILE: &str = "syndrid-provider-connections.json";
const CODEX_FILE: &str = "syndrid-codex-accounts.json";
const OPENROUTER_FILE: &str = "syndrid-openrouter-connections.json";
const MAX_PROMPT_ATTEMPTS: usize = 3;

#[derive(Debug, clap::Args)]
pub struct SetupCommand {
    /// Select or create a routing profile.
    #[arg(long)]
    pub profile: Option<String>,
    /// Require explicit command-line selections instead of prompting.
    #[arg(long)]
    pub non_interactive: bool,
    /// Provider for a new connection, or a provider constraint for selection.
    #[arg(long)]
    pub provider: Option<String>,
    /// Exact named connection to use.
    #[arg(long)]
    pub connection: Option<String>,
    /// Exact model ID to assign to every selected role.
    #[arg(long)]
    pub model: Option<String>,
    /// Activate the resulting profile after confirmation.
    #[arg(long)]
    pub activate: bool,
    /// Confirm persistence and activation in noninteractive mode.
    #[arg(long)]
    pub yes: bool,
    /// Include the optional Repair role.
    #[arg(long)]
    pub repair: bool,
    /// Duplicate an existing profile into --profile.
    #[arg(long, conflicts_with = "validate_only", conflicts_with = "readiness")]
    pub duplicate_from: Option<String>,
    /// Optional display name for a duplicated profile.
    #[arg(long, requires = "duplicate_from")]
    pub display_name: Option<String>,
    /// Validate a named profile without changing persistence.
    #[arg(long, conflicts_with = "duplicate_from")]
    pub validate_only: bool,
    /// Run local readiness, optionally including explicit credential presence checks.
    #[arg(long, conflicts_with = "duplicate_from")]
    pub readiness: bool,
    /// Include credential presence checks in readiness.
    #[arg(long, requires = "readiness")]
    pub check_credentials: bool,
    /// Emit the bounded result as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SetupConnection {
    provider: String,
    id: String,
    label: String,
    enabled: bool,
    validation: ConnectionValidationStatus,
    models: Option<Vec<String>>,
    state: Option<CodexAccountProfileState>,
    credential_reference: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SetupContext {
    connections: Vec<SetupConnection>,
    profiles: RoutingProfileRegistry,
    active_profile: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct OpenRouterConnectionRecord {
    connection_id: String,
    provider_id: String,
    label: String,
    credential_reference: String,
    enabled: bool,
    validation: ConnectionValidationStatus,
    validated_at: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
struct OpenRouterConnectionRegistry {
    connections: BTreeMap<String, OpenRouterConnectionRecord>,
}

#[derive(Clone, Debug, Serialize)]
struct SetupRoleReport {
    role: String,
    provider: Option<String>,
    connection: Option<String>,
    model: Option<String>,
    status: String,
}

#[derive(Clone, Debug, Serialize)]
struct SetupJsonReport {
    schema_version: u32,
    operation: String,
    profile_id: String,
    active: bool,
    status: String,
    roles: Vec<SetupRoleReport>,
    warnings: Vec<String>,
}

const SETUP_SCHEMA_VERSION: u32 = 1;

pub(crate) fn persist_openrouter_connection(metadata: &OpenRouterConnectionMetadata) -> Result<()> {
    let home = find_codex_home()?;
    let path = home.join(OPENROUTER_FILE);
    let mut registry = if path.exists() {
        serde_json::from_slice(&std::fs::read(&path)?)?
    } else {
        OpenRouterConnectionRegistry::default()
    };
    registry.connections.insert(
        metadata.connection_id.clone(),
        OpenRouterConnectionRecord {
            connection_id: metadata.connection_id.clone(),
            provider_id: metadata.provider_id.clone(),
            label: metadata.label.clone(),
            credential_reference: metadata.credential_reference.clone(),
            enabled: metadata.enabled,
            validation: metadata.validation,
            validated_at: metadata.validated_at,
        },
    );
    let bytes = serde_json::to_vec_pretty(&registry)?;
    std::fs::create_dir_all(&home)?;
    let mut temporary = tempfile::NamedTempFile::new_in(&home)?;
    temporary.write_all(&bytes)?;
    temporary
        .persist(&path)
        .map_err(|error| anyhow::anyhow!(error))?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SetupError {
    NonInteractiveInputRequired,
    SetupCancelled,
    EndOfInput,
    InvalidSelection,
    TooManyInvalidAttempts,
    MissingRequiredOption,
    UnknownConnection,
    InvalidModelId,
    InvalidProfileId,
    DuplicateProfile,
    MissingRequiredRole,
    ProfileValidationFailed,
    ProfilePersistenceFailed,
    InvalidFlagCombination,
    ProfileNotFound,
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NonInteractiveInputRequired => {
                "non-interactive setup requires --profile, --connection, and --model"
            }
            Self::SetupCancelled => "setup cancelled",
            Self::EndOfInput => "setup input ended before completion",
            Self::InvalidSelection => "setup selection is invalid",
            Self::TooManyInvalidAttempts => "too many invalid setup inputs",
            Self::MissingRequiredOption => "a required setup option is missing",
            Self::UnknownConnection => "provider connection was not found",
            Self::InvalidModelId => "model ID is invalid",
            Self::InvalidProfileId => "routing profile ID is invalid",
            Self::DuplicateProfile => "routing profile already exists",
            Self::MissingRequiredRole => "a required routing role is missing",
            Self::ProfileValidationFailed => "routing profile validation failed",
            Self::ProfilePersistenceFailed => "routing profile could not be saved",
            Self::InvalidFlagCombination => "setup options cannot be combined",
            Self::ProfileNotFound => "routing profile was not found",
        })
    }
}

impl std::error::Error for SetupError {}

/// A bounded prompt surface used by both the terminal implementation and deterministic tests.
trait SetupPrompt {
    fn output(&mut self, line: &str) -> Result<()>;
    fn select(&mut self, title: &str, options: &[String]) -> Result<usize>;
    fn input(&mut self, title: &str, default: Option<&str>) -> Result<String>;
    fn confirm(&mut self, title: &str, default: bool) -> Result<bool>;
}

struct TerminalPrompt {
    stdin: std::io::Stdin,
    stdout: std::io::Stdout,
}

impl TerminalPrompt {
    fn new() -> Self {
        Self {
            stdin: std::io::stdin(),
            stdout: std::io::stdout(),
        }
    }

    fn read_line(&mut self, title: &str) -> Result<String> {
        write!(self.stdout, "{title}: ")?;
        self.stdout.flush()?;
        let mut line = String::new();
        let read = self.stdin.lock().read_line(&mut line)?;
        if read == 0 {
            return Err(SetupError::EndOfInput.into());
        }
        Ok(line.trim().to_string())
    }
}

impl SetupPrompt for TerminalPrompt {
    fn output(&mut self, line: &str) -> Result<()> {
        writeln!(self.stdout, "{line}")?;
        Ok(())
    }

    fn select(&mut self, title: &str, options: &[String]) -> Result<usize> {
        for (index, option) in options.iter().enumerate() {
            writeln!(self.stdout, "  {}. {option}", index + 1)?;
        }
        for _ in 0..MAX_PROMPT_ATTEMPTS {
            let value = self.read_line(title)?;
            if value.eq_ignore_ascii_case("cancel") {
                return Err(SetupError::SetupCancelled.into());
            }
            if let Ok(number) = value.parse::<usize>()
                && (1..=options.len()).contains(&number)
            {
                return Ok(number - 1);
            }
        }
        Err(SetupError::TooManyInvalidAttempts.into())
    }

    fn input(&mut self, title: &str, default: Option<&str>) -> Result<String> {
        let value = self.read_line(title)?;
        if value.eq_ignore_ascii_case("cancel") {
            return Err(SetupError::SetupCancelled.into());
        }
        Ok(if value.is_empty() {
            default.unwrap_or_default().to_string()
        } else {
            value
        })
    }

    fn confirm(&mut self, title: &str, default: bool) -> Result<bool> {
        let suffix = if default { "Y/n" } else { "y/N" };
        let value = self.read_line(&format!("{title} [{suffix}]"))?;
        if value.eq_ignore_ascii_case("cancel") {
            return Err(SetupError::SetupCancelled.into());
        }
        Ok(match value.to_ascii_lowercase().as_str() {
            "y" | "yes" => true,
            "n" | "no" => false,
            "" => default,
            _ => return Err(SetupError::InvalidSelection.into()),
        })
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
struct ScriptedPrompt {
    answers: Vec<String>,
    position: usize,
    output: Vec<String>,
}

#[cfg(test)]
impl ScriptedPrompt {
    #[cfg(test)]
    fn new(answers: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            answers: answers.into_iter().map(str::to_string).collect(),
            ..Self::default()
        }
    }

    fn next(&mut self) -> Result<String> {
        let answer = self
            .answers
            .get(self.position)
            .cloned()
            .ok_or(SetupError::EndOfInput)?;
        self.position += 1;
        if answer.eq_ignore_ascii_case("cancel") {
            return Err(SetupError::SetupCancelled.into());
        }
        Ok(answer)
    }
}

#[cfg(test)]
impl SetupPrompt for ScriptedPrompt {
    fn output(&mut self, line: &str) -> Result<()> {
        self.output.push(line.to_string());
        Ok(())
    }

    fn select(&mut self, _title: &str, options: &[String]) -> Result<usize> {
        let value = self.next()?.parse::<usize>().ok();
        value
            .filter(|number| (1..=options.len()).contains(number))
            .map(|number| number - 1)
            .ok_or_else(|| SetupError::InvalidSelection.into())
    }

    fn input(&mut self, _title: &str, default: Option<&str>) -> Result<String> {
        let value = self.next()?;
        Ok(if value.is_empty() {
            default.unwrap_or_default().to_string()
        } else {
            value
        })
    }

    fn confirm(&mut self, _title: &str, default: bool) -> Result<bool> {
        match self.next()?.to_ascii_lowercase().as_str() {
            "y" | "yes" => Ok(true),
            "n" | "no" => Ok(false),
            "" => Ok(default),
            _ => Err(SetupError::InvalidSelection.into()),
        }
    }
}

pub async fn run(command: SetupCommand) -> Result<()> {
    let home = find_codex_home()?;
    let context = load_context(&home)?;
    validate_flags(&command)?;
    if command.validate_only || command.readiness {
        return run_readiness_operation(&command, &context);
    }
    if command.duplicate_from.is_some() {
        return run_duplicate_operation(&command, &context);
    }
    let mut prompt = TerminalPrompt::new();
    if command.non_interactive {
        run_non_interactive(command, context, &mut prompt)
    } else if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        Err(SetupError::NonInteractiveInputRequired.into())
    } else {
        run_interactive(command, context, &mut prompt).await
    }
}

fn validate_flags(command: &SetupCommand) -> Result<()> {
    if command.validate_only || command.readiness {
        if command.profile.is_none()
            || command.connection.is_some()
            || command.model.is_some()
            || command.provider.is_some()
            || command.activate
            || command.repair
            || command.yes
        {
            return Err(SetupError::InvalidFlagCombination.into());
        }
    }
    if command.duplicate_from.is_some()
        && (command.profile.is_none()
            || command.connection.is_some()
            || command.model.is_some()
            || command.provider.is_some()
            || command.activate
            || command.repair)
    {
        return Err(SetupError::InvalidFlagCombination.into());
    }
    if command.display_name.is_some() && command.duplicate_from.is_none() {
        return Err(SetupError::InvalidFlagCombination.into());
    }
    if command.yes && command.validate_only {
        return Err(SetupError::InvalidFlagCombination.into());
    }
    if command.yes
        && !command.non_interactive
        && command.duplicate_from.is_none()
        && !command.validate_only
        && !command.readiness
    {
        return Err(SetupError::InvalidFlagCombination.into());
    }
    if command.non_interactive && command.duplicate_from.is_some() && !command.yes {
        return Err(SetupError::MissingRequiredOption.into());
    }
    if command.json
        && !command.non_interactive
        && command.duplicate_from.is_none()
        && !command.validate_only
        && !command.readiness
    {
        return Err(SetupError::InvalidFlagCombination.into());
    }
    Ok(())
}

fn run_duplicate_operation(command: &SetupCommand, context: &SetupContext) -> Result<()> {
    let source = command
        .duplicate_from
        .as_deref()
        .ok_or(SetupError::MissingRequiredOption)?;
    let destination = command
        .profile
        .as_deref()
        .ok_or(SetupError::MissingRequiredOption)?;
    let source_id = RoutingProfileId::new(source).map_err(|_| SetupError::InvalidProfileId)?;
    let destination_id =
        RoutingProfileId::new(destination).map_err(|_| SetupError::InvalidProfileId)?;
    let duplicate = duplicate_profile(
        context,
        &source_id,
        destination_id,
        command.display_name.as_deref(),
    )?;
    let mut prompt = TerminalPrompt::new();
    if command.json {
        print_json_setup_report(context, &duplicate)?;
    } else {
        print_summary(&mut prompt, &duplicate, context)?;
    }
    if !command.yes && !prompt.confirm("Persist duplicated profile", true)? {
        return Err(SetupError::SetupCancelled.into());
    }
    persist_profile(context, duplicate, false, command.json, &mut prompt, false)
}

fn duplicate_profile(
    context: &SetupContext,
    source_id: &RoutingProfileId,
    destination_id: RoutingProfileId,
    display_name: Option<&str>,
) -> Result<RoutingProfile> {
    if context.profiles.get(&destination_id).is_some() {
        return Err(SetupError::DuplicateProfile.into());
    }
    let source_profile = context
        .profiles
        .get(source_id)
        .ok_or(SetupError::ProfileNotFound)?;
    let mut duplicate = source_profile.clone();
    duplicate.id = destination_id;
    duplicate.name = display_name
        .unwrap_or_else(|| duplicate.id.as_str())
        .to_string();
    duplicate.created_at = now();
    duplicate.updated_at = duplicate.created_at;
    validate_profile(context, &duplicate)?;
    Ok(duplicate)
}

fn run_readiness_operation(command: &SetupCommand, context: &SetupContext) -> Result<()> {
    let profile_id = command
        .profile
        .as_deref()
        .ok_or(SetupError::MissingRequiredOption)?;
    let id = RoutingProfileId::new(profile_id).map_err(|_| SetupError::InvalidProfileId)?;
    let profile = context
        .profiles
        .get(&id)
        .ok_or(SetupError::ProfileNotFound)?;
    let (mut roles, mut warnings, mut valid) = profile_report(context, profile);
    let mut operation = if command.readiness {
        "readiness"
    } else {
        "validate"
    }
    .to_string();
    if command.readiness && command.check_credentials {
        operation = "credential_readiness".to_string();
        let mut seen = BTreeMap::new();
        for report in &mut roles {
            let Some(connection_id) = report.connection.as_deref() else {
                continue;
            };
            let Some(connection) = context
                .connections
                .iter()
                .find(|item| item.id == connection_id)
            else {
                continue;
            };
            let Some(reference) = connection.credential_reference.as_deref() else {
                report.status = "missing_credential".to_string();
                valid = false;
                continue;
            };
            let credential_ready = if let Some(known) = seen.get(reference) {
                *known
            } else {
                let result = if connection.provider == "codex" {
                    codex_core::retrieve_codex_envelope(connection_id).is_ok()
                } else {
                    codex_core::provider_credential_exists(reference).unwrap_or(false)
                };
                seen.insert(reference.to_string(), result);
                result
            };
            if !credential_ready {
                report.status = "missing_credential".to_string();
                valid = false;
            }
        }
        if !valid {
            warnings.push("one or more assigned connections lack credentials".to_string());
        }
    }
    let status = if valid {
        if warnings.is_empty() {
            "ready"
        } else {
            "ready_with_warnings"
        }
    } else {
        "invalid"
    };
    if command.json {
        println!(
            "{}",
            serde_json::to_string(&SetupJsonReport {
                schema_version: SETUP_SCHEMA_VERSION,
                operation,
                profile_id: profile.id.as_str().to_string(),
                active: context.active_profile.as_deref() == Some(profile.id.as_str()),
                status: status.to_string(),
                roles,
                warnings,
            })?
        );
    } else {
        println!("Profile: {}", profile.id);
        println!("Status: {}", status.replace('_', " "));
        for report in roles {
            println!("{}: {}", report.role, report.status.replace('_', " "));
        }
        for warning in warnings {
            println!("Warning: {warning}");
        }
        println!("No changes were made.");
    }
    if valid {
        Ok(())
    } else {
        Err(SetupError::ProfileValidationFailed.into())
    }
}

fn profile_report(
    context: &SetupContext,
    profile: &RoutingProfile,
) -> (Vec<SetupRoleReport>, Vec<String>, bool) {
    let mut warnings = Vec::new();
    let mut valid = true;
    let mut reports = Vec::new();
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
        RoutingRole::Repair,
    ] {
        let Some(assignment) = profile.assignments.get(&role) else {
            let required = role != RoutingRole::Repair;
            if required {
                valid = false;
            }
            reports.push(SetupRoleReport {
                role: role.to_string(),
                provider: None,
                connection: None,
                model: None,
                status: if required {
                    "missing_assignment"
                } else {
                    "not_assigned"
                }
                .to_string(),
            });
            continue;
        };
        let status = match context
            .connections
            .iter()
            .find(|item| item.id == assignment.connection_id)
        {
            None => "unknown_connection".to_string(),
            Some(connection) if !connection.enabled => "disabled".to_string(),
            Some(connection)
                if connection.state == Some(CodexAccountProfileState::Unconfigured) =>
            {
                "unconfigured_account".to_string()
            }
            Some(connection)
                if connection.state == Some(CodexAccountProfileState::ReauthenticationRequired) =>
            {
                "reauthentication_required".to_string()
            }
            Some(_connection) => match context.directory().validate_assignment(assignment) {
                Ok(RoutingResolutionStatus::LocallyValid) => "valid".to_string(),
                Ok(RoutingResolutionStatus::ModelUnverified) => {
                    warnings.push(format!("{} model is unverified", role));
                    "model_unverified".to_string()
                }
                Err(RoutingProfileError::ModelNotFound) => "invalid_model".to_string(),
                Err(RoutingProfileError::UnsupportedAuthenticationMethod) => {
                    "unsupported_provider".to_string()
                }
                Err(RoutingProfileError::DisabledConnection) => "disabled".to_string(),
                Err(_) => "invalid".to_string(),
            },
        };
        if !matches!(
            status.as_str(),
            "valid" | "model_unverified" | "not_assigned"
        ) {
            valid = false;
        }
        reports.push(SetupRoleReport {
            role: role.to_string(),
            provider: Some(assignment.provider_id.clone()),
            connection: Some(assignment.connection_id.clone()),
            model: Some(assignment.model_id.clone()),
            status,
        });
    }
    (reports, warnings, valid)
}

fn run_non_interactive(
    command: SetupCommand,
    context: SetupContext,
    prompt: &mut impl SetupPrompt,
) -> Result<()> {
    if command.duplicate_from.is_some() || command.validate_only || command.readiness {
        return Err(SetupError::InvalidFlagCombination.into());
    }
    if !command.yes {
        return Err(SetupError::MissingRequiredOption.into());
    }
    let profile_id = command.profile.ok_or(SetupError::MissingRequiredOption)?;
    let connection_id = command
        .connection
        .ok_or(SetupError::MissingRequiredOption)?;
    let model = command.model.ok_or(SetupError::MissingRequiredOption)?;
    let id = RoutingProfileId::new(profile_id).map_err(|_| SetupError::InvalidProfileId)?;
    let connection = find_connection(&context, &connection_id, command.provider.as_deref())?;
    let profile = build_profile(&context, id, None, &connection, &model, command.repair)?;
    finish_profile(
        command.activate,
        false,
        command.json,
        context,
        profile,
        prompt,
    )
}

async fn run_interactive(
    command: SetupCommand,
    context: SetupContext,
    prompt: &mut impl SetupPrompt,
) -> Result<()> {
    prompt.output("Syndrid setup")?;
    prompt.output(&format!(
        "Connections: {}; profiles: {}; active: {}",
        context.connections.len(),
        context.profiles.profiles().count(),
        context.active_profile.as_deref().unwrap_or("none")
    ))?;
    if context.connections.is_empty() {
        connect_provider_interactively(prompt).await?;
        return Box::pin(run_interactive(
            command,
            load_context(&find_codex_home()?)?,
            prompt,
        ))
        .await;
    }

    if command.profile.is_none()
        && command.duplicate_from.is_none()
        && context.profiles.profiles().next().is_some()
    {
        let actions = vec![
            "Create or edit a routing profile".to_string(),
            "Duplicate an existing profile".to_string(),
            "Cancel".to_string(),
        ];
        match prompt.select("Profile action", &actions)? {
            1 => {
                let source = prompt.input("Source profile ID", None)?;
                let source_id =
                    RoutingProfileId::new(source).map_err(|_| SetupError::InvalidProfileId)?;
                let destination = prompt.input("New profile ID", None)?;
                let destination_id =
                    RoutingProfileId::new(destination).map_err(|_| SetupError::InvalidProfileId)?;
                let display_name = prompt.input("Display name", None)?;
                let duplicate = duplicate_profile(
                    &context,
                    &source_id,
                    destination_id,
                    (!display_name.is_empty()).then_some(display_name.as_str()),
                )?;
                if command.json {
                    print_json_setup_report(&context, &duplicate)?;
                } else {
                    print_summary(prompt, &duplicate, &context)?;
                }
                if !prompt.confirm("Persist duplicated profile", true)? {
                    return Err(SetupError::SetupCancelled.into());
                }
                return persist_profile(&context, duplicate, false, command.json, prompt, false);
            }
            2 => return Err(SetupError::SetupCancelled.into()),
            _ => {}
        }
    }

    let profile_id = match command.profile.clone() {
        Some(value) => RoutingProfileId::new(value).map_err(|_| SetupError::InvalidProfileId)?,
        None => {
            let value = prompt.input("Profile ID", Some("default"))?;
            RoutingProfileId::new(value).map_err(|_| SetupError::InvalidProfileId)?
        }
    };
    let existing = context.profiles.get(&profile_id).cloned();
    if existing.is_some() {
        prompt.output("An existing profile will be edited in memory until confirmation.")?;
    }

    let connection_options = context
        .connections
        .iter()
        .map(|connection| {
            format!(
                "{} / {} / {} ({:?})",
                connection.provider, connection.id, connection.label, connection.validation
            )
        })
        .collect::<Vec<_>>();
    let mut connection_options = connection_options;
    connection_options.push("Create a new provider connection".to_string());
    let index = prompt.select("Connection number", &connection_options)?;
    if index == context.connections.len() {
        connect_provider_interactively(prompt).await?;
        return Box::pin(run_interactive(
            command,
            load_context(&find_codex_home()?)?,
            prompt,
        ))
        .await;
    }
    let connection = &context.connections[index];
    let model = match (&connection.models, command.model) {
        (Some(models), Some(model)) if models.iter().any(|item| item == &model) => model,
        (Some(models), None) => {
            let mut options = models.clone();
            options.push("Enter an explicit model ID".to_string());
            let selected = prompt.select("Model number", &options)?;
            if selected < models.len() {
                models[selected].clone()
            } else {
                prompt.input("Model ID", None)?
            }
        }
        (_, Some(model)) => model,
        (None, None) => prompt.input("Exact model ID", None)?,
    };
    let profile = if prompt.confirm(
        "Reuse this connection and model for all required roles",
        true,
    )? {
        build_profile(
            &context,
            profile_id,
            existing.as_ref(),
            connection,
            &model,
            command.repair,
        )?
    } else {
        let mut profile = existing.clone().unwrap_or(RoutingProfile::new(
            profile_id.clone(),
            profile_id.as_str(),
            now(),
        )?);
        for role in [
            RoutingRole::Main,
            RoutingRole::Planner,
            RoutingRole::Executor,
            RoutingRole::Verifier,
        ] {
            profile.replace_assignment(role, prompt_assignment(prompt, &context, role)?)?;
        }
        if command.repair {
            profile.replace_assignment(
                RoutingRole::Repair,
                prompt_assignment(prompt, &context, RoutingRole::Repair)?,
            )?;
        } else {
            profile.assignments.remove(&RoutingRole::Repair);
        }
        validate_profile(&context, &profile)?;
        profile
    };
    finish_profile(
        command.activate,
        true,
        command.json,
        context,
        profile,
        prompt,
    )
}

fn prompt_assignment(
    prompt: &mut impl SetupPrompt,
    context: &SetupContext,
    role: RoutingRole,
) -> Result<RoutingAssignment> {
    let options = context
        .connections
        .iter()
        .map(|connection| {
            format!(
                "{} / {} / {}",
                connection.provider, connection.id, connection.label
            )
        })
        .collect::<Vec<_>>();
    let index = prompt.select(&format!("{role} connection"), &options)?;
    let connection = &context.connections[index];
    let model = match &connection.models {
        Some(models) => models[prompt.select(&format!("{role} model"), models)?].clone(),
        None => prompt.input(&format!("{role} exact model ID"), None)?,
    };
    Ok(RoutingAssignment {
        connection_id: connection.id.clone(),
        provider_id: connection.provider.clone(),
        model_id: model,
        enabled: true,
        label: Some(connection.label.clone()),
    })
}

async fn connect_provider_interactively(prompt: &mut impl SetupPrompt) -> Result<()> {
    let providers = [
        "Codex".to_string(),
        "OpenRouter".to_string(),
        "OmniRoute".to_string(),
        "Cancel".to_string(),
    ];
    match prompt.select("Provider to connect", &providers)? {
        0 => {
            let name = prompt.input("Codex connection ID", None)?;
            let label = prompt.input("Codex connection label", Some("Codex account"))?;
            crate::provider_cmd::run(ProviderCommand {
                subcommand: ProviderSubcommand::Connect(ProviderConnectCommand {
                    provider: ProviderConnectProvider::Codex(CodexConnectCommand { name, label }),
                }),
            })
            .await
        }
        1 => {
            crate::provider_cmd::run(ProviderCommand {
                subcommand: ProviderSubcommand::Connect(ProviderConnectCommand {
                    provider: ProviderConnectProvider::Openrouter,
                }),
            })
            .await
        }
        2 => {
            let name = prompt.input("OmniRoute connection ID", Some("omniroute-local"))?;
            let label = prompt.input("OmniRoute connection label", Some("Local OmniRoute"))?;
            prompt.output(
                "OmniRoute reads its API key from stdin through the existing provider flow.",
            )?;
            crate::provider_cmd::run(ProviderCommand {
                subcommand: ProviderSubcommand::Connect(ProviderConnectCommand {
                    provider: ProviderConnectProvider::Omniroute(OmniRouteConnectCommand {
                        name,
                        label,
                        base_url: codex_core::OMNIROUTE_DEFAULT_BASE_URL.to_string(),
                        allow_remote_https: false,
                    }),
                }),
            })
            .await
        }
        _ => Err(SetupError::SetupCancelled.into()),
    }
}

fn build_profile(
    context: &SetupContext,
    id: RoutingProfileId,
    existing: Option<&RoutingProfile>,
    connection: &SetupConnection,
    model: &str,
    include_repair: bool,
) -> Result<RoutingProfile> {
    if model.trim().is_empty() || model.len() > 256 {
        return Err(SetupError::InvalidModelId.into());
    }
    let mut profile =
        existing
            .cloned()
            .unwrap_or(RoutingProfile::new(id.clone(), id.as_str(), now())?);
    let assignment = RoutingAssignment {
        connection_id: connection.id.clone(),
        provider_id: connection.provider.clone(),
        model_id: model.to_string(),
        enabled: true,
        label: Some(connection.label.clone()),
    };
    for role in [
        RoutingRole::Main,
        RoutingRole::Planner,
        RoutingRole::Executor,
        RoutingRole::Verifier,
    ] {
        profile.replace_assignment(role, assignment.clone())?;
    }
    if include_repair {
        profile.replace_assignment(RoutingRole::Repair, assignment)?;
    } else {
        profile.assignments.remove(&RoutingRole::Repair);
    }
    validate_profile(context, &profile)?;
    Ok(profile)
}

fn finish_profile(
    activate: bool,
    prompt_for_confirmation: bool,
    json: bool,
    context: SetupContext,
    profile: RoutingProfile,
    prompt: &mut impl SetupPrompt,
) -> Result<()> {
    if json {
        print_json_setup_report(&context, &profile)?;
    } else {
        print_summary(prompt, &profile, &context)?;
    }
    if prompt_for_confirmation && !prompt.confirm("Persist this profile", true)? {
        return Err(SetupError::SetupCancelled.into());
    }
    persist_profile(
        &context,
        profile,
        activate,
        json,
        prompt,
        prompt_for_confirmation,
    )
}

fn print_json_setup_report(context: &SetupContext, profile: &RoutingProfile) -> Result<()> {
    let (roles, warnings, valid) = profile_report(context, profile);
    println!(
        "{}",
        serde_json::to_string(&SetupJsonReport {
            schema_version: SETUP_SCHEMA_VERSION,
            operation: "setup".to_string(),
            profile_id: profile.id.as_str().to_string(),
            active: context.active_profile.as_deref() == Some(profile.id.as_str()),
            status: if valid {
                if warnings.is_empty() {
                    "ready".to_string()
                } else {
                    "ready_with_warnings".to_string()
                }
            } else {
                "invalid".to_string()
            },
            roles,
            warnings,
        })?
    );
    Ok(())
}

fn persist_profile(
    context: &SetupContext,
    profile: RoutingProfile,
    activate: bool,
    json: bool,
    prompt: &mut impl SetupPrompt,
    prompt_for_confirmation: bool,
) -> Result<()> {
    let home = find_codex_home()?;
    let store = RoutingProfileStore::new(home.join(PROFILE_FILE));
    let mut registry = context.profiles.clone();
    let id = profile.id.clone();
    if registry.get(&id).is_some() {
        registry.profiles.insert(id.clone(), profile);
    } else {
        registry.insert(profile)?;
    }
    if let Err(error) = store.save(&registry) {
        return Err(map_profile_error(error));
    }
    if activate {
        if prompt_for_confirmation && !prompt.confirm("Activate this profile", true)? {
            if !json {
                prompt.output("Profile saved inactive.")?;
            }
            return Ok(());
        }
        let mut registry = store.load().map_err(map_profile_error)?;
        registry.activate(&id).map_err(map_profile_error)?;
        store.save(&registry).map_err(map_profile_error)?;
        if !json {
            prompt.output(&format!("Active profile: {id}"))?;
        }
    }
    if !json {
        prompt.output(&format!("Setup complete: profile {id}"))?;
    }
    Ok(())
}

fn print_summary(
    prompt: &mut impl SetupPrompt,
    profile: &RoutingProfile,
    context: &SetupContext,
) -> Result<()> {
    prompt.output(&format!("Profile: {} ({})", profile.id, profile.name))?;
    for (role, assignment) in &profile.assignments {
        let status = context
            .connections
            .iter()
            .find(|connection| connection.id == assignment.connection_id)
            .map(|connection| format_status(connection, assignment))
            .unwrap_or_else(|| "unknown connection".to_string());
        prompt.output(&format!(
            "  {role}: {} / {} / {status}",
            assignment.provider_id, assignment.model_id
        ))?;
    }
    if !profile.assignments.contains_key(&RoutingRole::Repair) {
        prompt.output("  repair: not assigned")?;
    }
    Ok(())
}

fn format_status(connection: &SetupConnection, assignment: &RoutingAssignment) -> String {
    if !connection.enabled {
        return "disabled".to_string();
    }
    if connection.validation != ConnectionValidationStatus::Valid {
        return format!("{:?}", connection.validation);
    }
    match &connection.models {
        Some(models) if models.iter().any(|model| model == &assignment.model_id) => {
            "valid".to_string()
        }
        Some(_) => "model unverified".to_string(),
        None => "model unverified".to_string(),
    }
}

fn validate_profile(context: &SetupContext, profile: &RoutingProfile) -> Result<()> {
    profile
        .validate_required_roles()
        .map_err(map_profile_error)?;
    for assignment in profile.assignments.values() {
        context
            .directory()
            .validate_assignment(assignment)
            .map_err(map_profile_error)?;
    }
    Ok(())
}

fn find_connection<'a>(
    context: &'a SetupContext,
    id: &str,
    provider: Option<&str>,
) -> Result<&'a SetupConnection> {
    let connection = context
        .connections
        .iter()
        .find(|connection| connection.id == id)
        .ok_or(SetupError::UnknownConnection)?;
    if provider.is_some_and(|value| value != connection.provider) {
        return Err(SetupError::UnknownConnection.into());
    }
    Ok(connection)
}

impl SetupContext {
    fn directory(&self) -> RoutingConnectionDirectory {
        let mut directory = RoutingConnectionDirectory::default();
        for connection in &self.connections {
            directory.insert(RoutingConnectionInfo {
                connection_id: connection.id.clone(),
                provider_id: connection.provider.clone(),
                enabled: connection.enabled,
                validation: connection.validation,
                authentication_supported: true,
                models: connection.models.clone(),
            });
        }
        directory
    }
}

fn load_context(home: &std::path::Path) -> Result<SetupContext> {
    let providers = OmniRouteRegistry::load(&home.join(PROVIDER_FILE))?;
    let codex = CodexAccountProfileRegistry::load(&home.join(CODEX_FILE))?;
    let profiles = RoutingProfileRegistry::load(&home.join(PROFILE_FILE))?;
    let openrouter = load_openrouter_registry(home)?;
    let mut connections = providers
        .connections()
        .map(|connection| SetupConnection {
            provider: connection.provider_id.clone(),
            id: connection.connection_id.clone(),
            label: connection.label.clone(),
            enabled: connection.enabled,
            validation: connection.validation.status,
            models: Some(connection.models.clone()),
            state: None,
            credential_reference: Some(connection.credential_reference.clone()),
        })
        .collect::<Vec<_>>();
    connections.extend(codex.profiles().map(|account| SetupConnection {
        provider: account.provider_id.clone(),
        id: account.connection_id.clone(),
        label: account.label.clone(),
        enabled: account.enabled,
        validation: if account.state == CodexAccountProfileState::Connected {
            account.validation
        } else {
            ConnectionValidationStatus::Invalid
        },
        models: None,
        state: Some(account.state),
        credential_reference: Some(account.credential_reference.clone()),
    }));
    connections.extend(
        openrouter
            .connections
            .values()
            .map(|connection| SetupConnection {
                provider: connection.provider_id.clone(),
                id: connection.connection_id.clone(),
                label: connection.label.clone(),
                enabled: connection.enabled,
                validation: connection.validation,
                models: None,
                state: None,
                credential_reference: Some(connection.credential_reference.clone()),
            }),
    );
    connections.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(SetupContext {
        active_profile: profiles.active_profile_id.as_ref().map(ToString::to_string),
        connections,
        profiles,
    })
}

fn load_openrouter_registry(home: &std::path::Path) -> Result<OpenRouterConnectionRegistry> {
    let path = home.join(OPENROUTER_FILE);
    if !path.exists() {
        return Ok(OpenRouterConnectionRegistry::default());
    }
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn map_profile_error(error: RoutingProfileError) -> anyhow::Error {
    match error {
        RoutingProfileError::MissingRoleAssignment => SetupError::MissingRequiredRole.into(),
        RoutingProfileError::AtomicWriteFailed => SetupError::ProfilePersistenceFailed.into(),
        RoutingProfileError::UnknownConnection
        | RoutingProfileError::DisabledConnection
        | RoutingProfileError::UnvalidatedConnection
        | RoutingProfileError::ProviderMismatch
        | RoutingProfileError::ModelNotFound
        | RoutingProfileError::InvalidModelId => SetupError::ProfileValidationFailed.into(),
        RoutingProfileError::DuplicateProfileId => SetupError::DuplicateProfile.into(),
        _ => SetupError::ProfileValidationFailed.into(),
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
#[path = "setup_cmd_tests.rs"]
mod tests;
