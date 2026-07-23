use anyhow::Result;
use clap::Subcommand;
use codex_core::BrowserLaunchStatus;
use codex_core::OpenRouterSetupRequest;
use codex_core::setup_openrouter;

#[derive(Debug, clap::Args)]
pub struct ProviderCommand {
    #[command(subcommand)]
    pub subcommand: ProviderSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ProviderSubcommand {
    /// Connect an OpenRouter account through the browser.
    Connect(ProviderConnectCommand),
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
}

pub async fn run(command: ProviderCommand) -> Result<()> {
    match command.subcommand {
        ProviderSubcommand::Connect(ProviderConnectCommand {
            provider: ProviderConnectProvider::Openrouter,
        }) => run_openrouter().await,
    }
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
