use anyhow::Result;
use clap::Subcommand;
use codex_cli::read_api_key_from_stdin;
use codex_core::BrowserLaunchStatus;
use codex_core::OMNIROUTE_DEFAULT_BASE_URL;
use codex_core::OMNIROUTE_PROVIDER_ID;
use codex_core::OmniRouteConnectionSetupRequest;
use codex_core::OmniRouteRegistry;
use codex_core::OpenRouterSetupCancellation as CancellationToken;
use codex_core::OpenRouterSetupRequest;
use codex_core::ProviderInvocationRequest;
use codex_core::ProviderSelection;
use codex_core::config::find_codex_home;
use codex_core::delete_omniroute_credential;
use codex_core::invoke_omniroute;
use codex_core::list_omniroute_models;
use codex_core::setup_omniroute;
use codex_core::setup_openrouter;
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
const MAX_PROMPT_BYTES: usize = 16 * 1024;
const MAX_DISPLAY_MODELS: usize = 512;

pub async fn run(command: ProviderCommand) -> Result<()> {
    match command.subcommand {
        ProviderSubcommand::Connect(ProviderConnectCommand { provider }) => match provider {
            ProviderConnectProvider::Openrouter => run_openrouter().await,
            ProviderConnectProvider::Omniroute(command) => run_omniroute_connect(command).await,
        },
        ProviderSubcommand::Models(command) => run_omniroute_models(command).await,
        ProviderSubcommand::Status(command) => run_provider_status(command).await,
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
    let path = registry_path()?;
    let registry = OmniRouteRegistry::load(&path)?;
    let selection = ProviderSelection::new(
        command.connection,
        OMNIROUTE_PROVIDER_ID,
        command.model.clone(),
    )?;
    let connection = selection.resolve(&registry)?;
    let (cancellation, signal_task) = cancellation_with_ctrl_c();
    let result = invoke_omniroute(
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
    .await;
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
