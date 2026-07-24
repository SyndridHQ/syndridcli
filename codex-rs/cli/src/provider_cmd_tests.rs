use super::*;
use clap::Parser;

#[derive(Debug, Parser)]
struct TestCli {
    #[command(flatten)]
    command: ProviderCommand,
}

#[test]
fn codex_provider_commands_parse_for_multiple_named_connections() {
    for name in ["codex-personal", "codex-secondary"] {
        let cli = TestCli::try_parse_from(["syndrid", "connect", "codex", "--name", name])
            .expect("connect command");
        assert!(matches!(
            cli.command.subcommand,
            ProviderSubcommand::Connect(ProviderConnectCommand {
                provider: ProviderConnectProvider::Codex(_)
            })
        ));
    }
}

#[test]
fn codex_provider_management_commands_parse() {
    for args in [
        vec!["syndrid", "list"],
        vec!["syndrid", "status", "codex-personal"],
        vec!["syndrid", "validate", "codex-personal"],
        vec!["syndrid", "logout", "codex-personal"],
    ] {
        TestCli::try_parse_from(args).expect("provider command");
    }
}

#[test]
fn invocation_debug_contains_no_prompt_or_secret_material() {
    let command = ProviderInvokeCommand {
        connection: "codex-personal".to_string(),
        model: "model".to_string(),
        prompt: "prompt-token-sentinel".to_string(),
        max_output_tokens: 32,
    };
    let debug = format!("{command:?}");
    assert!(!debug.contains("prompt-token-sentinel"));
    assert!(!debug.contains("access-token-sentinel"));
    assert!(!debug.contains("credential-reference-sentinel"));
}

#[test]
fn rollback_preserves_original_error_or_reports_bounded_cleanup_failure() {
    let original = anyhow::anyhow!("safe original failure");
    let preserved = super::rollback_codex_setup_with(original, || Ok::<(), &str>(()));
    assert_eq!(
        preserved.expect_err("original failure").to_string(),
        "safe original failure"
    );

    let cleanup_failed =
        super::rollback_codex_setup_with(anyhow::anyhow!("safe original failure"), || {
            Err::<(), _>("cleanup failed")
        });
    let message = cleanup_failed
        .expect_err("bounded cleanup failure")
        .to_string();
    assert_eq!(
        message,
        "Codex account setup failed and credential cleanup also failed"
    );
    assert!(!message.contains("safe original failure"));
    assert!(!message.contains("access-token-sentinel"));
}
