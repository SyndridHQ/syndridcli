use anyhow::Result;
use clap::Subcommand;
use codex_cli::read_api_key_from_stdin;
use codex_core::BrowserLaunchStatus;
use codex_core::CodexAccountConnectionMetadata;
use codex_core::CodexAccountProfileId;
use codex_core::CodexAccountProfileRegistry;
use codex_core::CodexAccountProfileState;
use codex_core::CodexAccountStore;
use codex_core::ConnectionValidationStatus;
use codex_core::OMNIROUTE_DEFAULT_BASE_URL;
use codex_core::OMNIROUTE_PROVIDER_ID;
use codex_core::OmniRouteConnectionSetupRequest;
use codex_core::OmniRouteRegistry;
use codex_core::OpenRouterSetupCancellation as CancellationToken;
use codex_core::OpenRouterSetupRequest;
use codex_core::ProviderInvocationRequest;
use codex_core::ProviderSelection;
use codex_core::config::find_codex_home;
use codex_core::delete_codex_auth;
use codex_core::delete_omniroute_credential;
use codex_core::invoke_codex;
use codex_core::invoke_omniroute;
use codex_core::list_omniroute_models;
use codex_core::setup_omniroute;
use codex_core::setup_openrouter;
use codex_core::store_codex_auth;
use codex_login::AuthCredentialsStoreMode;
use codex_login::AuthDotJson;
use codex_login::AuthKeyringBackendKind;
use codex_login::CLIENT_ID;
use codex_login::ServerOptions;
use codex_login::load_auth_dot_json;
use codex_login::run_login_server;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, clap::Args)]
pub struct ProviderCommand {
    #[command(subcommand)]
    pub subcommand: ProviderSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ProviderSubcommand {
    /// Connect an OpenRouter account through the browser or a local OmniRoute instance.
    Connect(ProviderConnectCommand),
    /// List models from a named connection.
    Models(ProviderModelsCommand),
    /// Show safe metadata for provider connections.
    Status(ProviderStatusCommand),
    /// List all named provider connections.
    List,
    /// Validate one named provider connection.
    Validate(ProviderConnectionCommand),
    /// Log out one named provider connection.
    Logout(ProviderConnectionCommand),
    /// Invoke one explicitly selected provider model.
    Invoke(ProviderInvokeCommand),
}

#[derive(Debug, clap::Args)]
pub struct ProviderConnectCommand {
    #[command(subcommand)]
    pub provider: ProviderConnectProvider,
}

#[derive(Debug, Subcommand)]
pub enum ProviderConnectProvider {
    /// Authenticate OpenRouter using OAuth PKCE.
    Openrouter,
    /// Connect OmniRoute; read its API key from stdin.
    Omniroute(OmniRouteConnectCommand),
    /// Authenticate a named Codex account using the repository OAuth flow.
    Codex(CodexConnectCommand),
}

#[derive(Debug, clap::Args)]
pub struct CodexConnectCommand {
    #[arg(long)]
    pub name: String,
    #[arg(long, default_value = "Codex account")]
    pub label: String,
}

#[derive(Debug, clap::Args)]
pub struct ProviderConnectionCommand {
    pub connection_id: String,
}

#[derive(Debug, clap::Args)]
pub struct OmniRouteConnectCommand {
    #[arg(long, default_value = "omniroute-local")]
    pub name: String,
    #[arg(long, default_value = "Local OmniRoute")]
    pub label: String,
    #[arg(long, default_value = OMNIROUTE_DEFAULT_BASE_URL)]
    pub base_url: String,
    #[arg(long)]
    pub allow_remote_https: bool,
}

#[derive(Debug, clap::Args)]
pub struct ProviderModelsCommand {
    pub connection_id: String,
    #[arg(long, default_value_t = 128)]
    pub limit: usize,
}

#[derive(Debug, clap::Args)]
pub struct ProviderStatusCommand {
    pub connection_id: Option<String>,
}

#[derive(Clone, Eq, PartialEq, clap::Args)]
pub struct ProviderInvokeCommand {
    #[arg(long)]
    pub connection: String,
    #[arg(long)]
    pub model: String,
    #[arg(long)]
    pub prompt: String,
    #[arg(long = "max-output-tokens", default_value_t = 1024)]
    pub max_output_tokens: u32,
}

impl fmt::Debug for ProviderInvokeCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderInvokeCommand")
            .field("connection", &self.connection)
            .field("model", &self.model)
            .field("prompt_bytes", &self.prompt.len())
            .field("max_output_tokens", &self.max_output_tokens)
            .finish()
    }
}

const REGISTRY_FILE: &str = "syndrid-provider-connections.json";
const CODEX_REGISTRY_FILE: &str = "syndrid-codex-accounts.json";
const MAX_PROMPT_BYTES: usize = 16 * 1024;
const MAX_DISPLAY_MODELS: usize = 512;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

pub async fn run(command: ProviderCommand) -> Result<()> {
    match command.subcommand {
        ProviderSubcommand::Connect(ProviderConnectCommand { provider }) => match provider {
            ProviderConnectProvider::Openrouter => run_openrouter().await,
            ProviderConnectProvider::Omniroute(command) => run_omniroute_connect(command).await,
            ProviderConnectProvider::Codex(command) => run_codex_connect(command).await,
        },
        ProviderSubcommand::Models(command) => run_omniroute_models(command).await,
        ProviderSubcommand::Status(command) => run_provider_status(command).await,
        ProviderSubcommand::List => run_provider_list().await,
        ProviderSubcommand::Validate(command) => run_provider_validate(command).await,
        ProviderSubcommand::Logout(command) => run_provider_logout(command).await,
        ProviderSubcommand::Invoke(command) => run_omniroute_invoke(command).await,
    }
}

fn registry_path() -> Result<PathBuf> {
    Ok(find_codex_home()?.join(REGISTRY_FILE).to_path_buf())
}

fn cancellation_with_ctrl_c() -> (CancellationToken, tokio::task::JoinHandle<()>) {
    let cancellation = CancellationToken::new();
    let signal_cancellation = cancellation.clone();
    let signal_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal_cancellation.cancel();
        }
    });
    (cancellation, signal_task)
}

async fn run_omniroute_connect(command: OmniRouteConnectCommand) -> Result<()> {
    let path = registry_path()?;
    let registry = OmniRouteRegistry::load(&path)?;
    if registry.get(&command.name).is_some() {
        anyhow::bail!("provider connection ID already exists");
    }
    let api_key = read_api_key_from_stdin();
    let (cancellation, signal_task) = cancellation_with_ctrl_c();
    let result = setup_omniroute(
        OmniRouteConnectionSetupRequest {
            connection_id: command.name.clone(),
            label: command.label,
            base_url: command.base_url,
            credential_reference: format!("omniroute-{}", command.name),
            api_key,
            allow_remote_https: command.allow_remote_https,
        },
        cancellation,
    )
    .await;
    signal_task.abort();
    let metadata = result?;
    let mut registry = registry;
    if let Err(error) = registry
        .insert(metadata.clone())
        .and_then(|_| registry.save(&path))
    {
        let _ = delete_omniroute_credential(&metadata);
        return Err(error.into());
    }
    println!(
        "Connected {} ({}) with {} available model(s).",
        metadata.label,
        metadata.connection_id,
        metadata.models.len()
    );
    Ok(())
}

fn codex_store() -> Result<CodexAccountStore> {
    Ok(CodexAccountStore::new(
        find_codex_home()?.join(CODEX_REGISTRY_FILE),
    ))
}

fn codex_metadata_from_auth(
    command: &CodexConnectCommand,
    auth: &AuthDotJson,
    credential_reference: String,
) -> Result<CodexAccountConnectionMetadata> {
    let tokens = auth
        .tokens
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Codex login did not return supported account tokens"))?;
    let account_id = tokens
        .account_id
        .clone()
        .or_else(|| tokens.id_token.chatgpt_account_id.clone());
    Ok(CodexAccountConnectionMetadata {
        connection_id: command.name.clone(),
        profile_id: CodexAccountProfileId::new(command.name.clone())?,
        provider_id: "codex".to_string(),
        label: command.label.clone(),
        state: CodexAccountProfileState::Connected,
        account_email: tokens.id_token.email.clone(),
        account_id,
        plan_label: tokens.id_token.get_chatgpt_plan_type(),
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        last_authenticated_at: Some(now()),
        last_validated_at: Some(now()),
        credential_reference,
        schema_version: 1,
    })
}

async fn run_codex_connect(command: CodexConnectCommand) -> Result<()> {
    let store = codex_store()?;
    let registry = store.load()?;
    let profile_id = CodexAccountProfileId::new(command.name.clone())?;
    if registry.get(&profile_id).is_some() || registry.get_connection(&command.name).is_some() {
        anyhow::bail!("provider connection ID already exists");
    }

    let staging = tempfile::tempdir()?;
    let options = ServerOptions::new(
        staging.path().to_path_buf(),
        CLIENT_ID.to_string(),
        None,
        AuthCredentialsStoreMode::Ephemeral,
        AuthKeyringBackendKind::default(),
        None,
    );
    let server = run_login_server(options)?;
    eprintln!(
        "If the browser did not open, copy this URL into your browser:\n\n{}",
        server.auth_url
    );
    let shutdown = server.cancel_handle();
    let login_result = tokio::select! {
        result = tokio::time::timeout(
            std::time::Duration::from_secs(15 * 60),
            server.block_until_done(),
        ) => result.map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "login timed out"))?,
        result = tokio::signal::ctrl_c() => {
            shutdown.shutdown();
            result.map_err(std::io::Error::other).and(Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "login cancelled")))
        }
    };
    login_result?;
    let auth = load_auth_dot_json(
        staging.path(),
        AuthCredentialsStoreMode::Ephemeral,
        AuthKeyringBackendKind::default(),
    )?
    .ok_or_else(|| anyhow::anyhow!("Codex login completed without credentials"))?;
    let credential_reference = store_codex_auth(&command.name, &auth)?;
    let metadata = match codex_metadata_from_auth(&command, &auth, credential_reference.clone()) {
        Ok(metadata) => metadata,
        Err(error) => {
            return rollback_codex_setup(&command.name, error);
        }
    };
    let mut registry = registry;
    if let Err(error) = registry
        .insert(metadata.clone())
        .and_then(|_| store.save(&registry))
    {
        return rollback_codex_setup(&command.name, error.into());
    }
    println!("Connected {} ({})", metadata.label, metadata.connection_id);
    Ok(())
}

fn rollback_codex_setup(connection_id: &str, original: anyhow::Error) -> Result<()> {
    rollback_codex_setup_with(original, || delete_codex_auth(connection_id))
}

fn rollback_codex_setup_with<E>(
    original: anyhow::Error,
    cleanup: impl FnOnce() -> Result<(), E>,
) -> Result<()> {
    match cleanup() {
        Ok(()) => Err(original),
        Err(_) => Err(anyhow::anyhow!(
            "Codex account setup failed and credential cleanup also failed"
        )),
    }
}

async fn run_provider_list() -> Result<()> {
    let home = find_codex_home()?;
    let providers = OmniRouteRegistry::load(&home.join(REGISTRY_FILE))?;
    for connection in providers.connections() {
        println!("{} ({})", connection.connection_id, connection.provider_id);
    }
    let codex_registry = codex_store()?.load()?;
    for account in codex_registry.profiles() {
        println!("{} ({})", account.connection_id, account.provider_id);
    }
    Ok(())
}

async fn run_provider_validate(command: ProviderConnectionCommand) -> Result<()> {
    let id = CodexAccountProfileId::new(command.connection_id)?;
    let store = codex_store()?;
    let mut registry = store.load()?;
    let account = registry
        .get_mut(&id)
        .ok_or_else(|| anyhow::anyhow!("provider connection was not found"))?;
    if account.state == CodexAccountProfileState::Disabled {
        anyhow::bail!("provider connection is disabled");
    }
    account.validation = match codex_core::retrieve_codex_envelope(&account.connection_id) {
        Ok(_) if account.state == CodexAccountProfileState::Connected => {
            ConnectionValidationStatus::Valid
        }
        _ => ConnectionValidationStatus::Invalid,
    };
    account.last_validated_at = Some(now());
    let connection_id = account.connection_id.clone();
    let validation = account.validation;
    store.save(&registry)?;
    println!("{connection_id}: {validation:?}");
    Ok(())
}

async fn run_provider_logout(command: ProviderConnectionCommand) -> Result<()> {
    let id = CodexAccountProfileId::new(command.connection_id)?;
    let store = codex_store()?;
    let mut registry = store.load()?;
    let account = registry
        .get_mut(&id)
        .ok_or_else(|| anyhow::anyhow!("provider connection was not found"))?;
    delete_codex_auth(&account.connection_id)?;
    account.state = CodexAccountProfileState::Unconfigured;
    account.validation = ConnectionValidationStatus::Unvalidated;
    account.account_email = None;
    account.account_id = None;
    account.plan_label = None;
    account.last_authenticated_at = None;
    account.last_validated_at = None;
    let connection_id = account.connection_id.clone();
    store.save(&registry)?;
    println!("Logged out {connection_id}");
    Ok(())
}

async fn run_omniroute_models(command: ProviderModelsCommand) -> Result<()> {
    let path = registry_path()?;
    let registry = OmniRouteRegistry::load(&path)?;
    let connection = registry
        .get(&command.connection_id)
        .ok_or_else(|| anyhow::anyhow!("provider connection was not found"))?;
    let (cancellation, signal_task) = cancellation_with_ctrl_c();
    let result = list_omniroute_models(connection, cancellation).await;
    signal_task.abort();
    let models = result?;
    let limit = command.limit.min(MAX_DISPLAY_MODELS);
    println!("connection: {}", connection.connection_id);
    println!("provider: {}", connection.provider_id);
    println!("base_url: {}", connection.base_url);
    println!("model_count: {}", models.len());
    for model in models.iter().take(limit) {
        println!("{model}");
    }
    if models.len() > limit {
        println!("... truncated to {limit} models");
    }
    Ok(())
}

async fn run_provider_status(command: ProviderStatusCommand) -> Result<()> {
    let path = registry_path()?;
    let registry = OmniRouteRegistry::load(&path)?;
    if let Some(connection_id) = command.connection_id.as_deref()
        && registry.get(connection_id).is_none()
    {
        let id = CodexAccountProfileId::new(connection_id.to_string())?;
        let codex_registry = codex_store()?.load()?;
        let account = codex_registry
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("provider connection was not found"))?;
        println!("{}", account.connection_id);
        println!("  provider: {}", account.provider_id);
        println!("  state: {:?}", account.state);
        println!("  enabled: {}", account.enabled);
        println!("  validated: {:?}", account.validation);
        if let Some(email) = account.account_email.as_deref() {
            println!("  account: {email}");
        }
        return Ok(());
    }
    let connections: Vec<_> = match command.connection_id {
        Some(connection_id) => vec![
            registry
                .get(&connection_id)
                .ok_or_else(|| anyhow::anyhow!("provider connection was not found"))?,
        ],
        None => registry.connections().collect(),
    };
    for connection in connections {
        println!("{connection}");
        println!("  provider: {}", connection.provider_id);
        println!("  base_url: {}", connection.base_url);
        println!("  enabled: {}", connection.enabled);
        println!("  validated: {:?}", connection.validation.status);
    }
    Ok(())
}

async fn run_omniroute_invoke(command: ProviderInvokeCommand) -> Result<()> {
    if command.prompt.trim().is_empty() || command.prompt.len() > MAX_PROMPT_BYTES {
        anyhow::bail!("prompt must be non-empty and at most {MAX_PROMPT_BYTES} bytes");
    }
    if command.max_output_tokens == 0 {
        anyhow::bail!("max output tokens must be positive");
    }
    let (cancellation, signal_task) = cancellation_with_ctrl_c();
    let result = if command.connection.starts_with("codex-") {
        let selection = ProviderSelection::new(
            command.connection,
            codex_core::CODEX_PROVIDER_ID,
            command.model.clone(),
        )?;
        let accounts =
            CodexAccountProfileRegistry::load(&find_codex_home()?.join(CODEX_REGISTRY_FILE))?;
        invoke_codex(
            selection,
            accounts,
            ProviderInvocationRequest {
                provider: codex_core::CODEX_PROVIDER_ID.to_string(),
                model: command.model,
                system: None,
                user: command.prompt,
                max_output_tokens: command.max_output_tokens,
            },
            cancellation,
        )
        .await
    } else {
        let path = registry_path()?;
        let registry = OmniRouteRegistry::load(&path)?;
        let selection = ProviderSelection::new(
            command.connection,
            OMNIROUTE_PROVIDER_ID,
            command.model.clone(),
        )?;
        let connection = selection.resolve(&registry)?;
        invoke_omniroute(
            connection.clone(),
            ProviderInvocationRequest {
                provider: OMNIROUTE_PROVIDER_ID.to_string(),
                model: command.model,
                system: None,
                user: command.prompt,
                max_output_tokens: command.max_output_tokens,
            },
            cancellation,
        )
        .await
    };
    signal_task.abort();
    println!("{}", result?.text);
    Ok(())
}

async fn run_openrouter() -> Result<()> {
    eprintln!("Opening OpenRouter authorization…");
    let cancellation = codex_core::OpenRouterSetupCancellation::new();
    let signal_cancellation = cancellation.clone();
    let signal_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal_cancellation.cancel();
        }
    });
    let result = setup_openrouter(
        OpenRouterSetupRequest {
            connection_id: "openrouter-default".to_string(),
            label: "OpenRouter".to_string(),
            credential_reference: "openrouter-default".to_string(),
        },
        cancellation,
        |started| {
            eprintln!(
                "If the browser did not open, copy this URL into your browser:\n\n{}",
                started.authorization_url()
            );
            if started.browser_launch() == BrowserLaunchStatus::Failed {
                eprintln!("Browser launch failed; continuing to wait for authorization…");
            }
            eprintln!("Waiting for authorization…");
        },
    )
    .await;
    signal_task.abort();
    result?;
    eprintln!("OpenRouter authorization completed.");
    Ok(())
}

#[cfg(test)]
#[path = "provider_cmd_tests.rs"]
mod provider_cmd_tests;
