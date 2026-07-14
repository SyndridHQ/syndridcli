use anyhow::Result;
use pretty_assertions::assert_eq;
use std::process::Command;

fn run_binary(binary: &str, arg: &str) -> Result<String> {
    let output = Command::new(codex_utils_cargo_bin::cargo_bin(binary)?)
        .arg(arg)
        .output()?;
    assert!(
        output.status.success(),
        "{binary} {arg} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

fn root_branding_line(help: &str) -> Option<&str> {
    help.lines().find(|line| !line.trim().is_empty())
}

#[test]
fn codex_help_preserves_codex_branding() -> Result<()> {
    let help = run_binary("codex", "--help")?;

    assert_eq!(root_branding_line(&help), Some("Codex CLI"));
    assert!(help.contains("Usage: codex [OPTIONS] [PROMPT]"));
    assert!(help.contains("codex [OPTIONS] <COMMAND> [ARGS]"));
    Ok(())
}

#[test]
fn codex_version_preserves_codex_branding() -> Result<()> {
    let version = run_binary("codex", "--version")?;

    assert_eq!(
        version,
        format!("codex-cli {}\n", env!("CARGO_PKG_VERSION"))
    );
    Ok(())
}

#[test]
fn syndrid_help_uses_syndrid_branding() -> Result<()> {
    let help = run_binary("syndrid", "--help")?;

    assert_eq!(root_branding_line(&help), Some("SyndridCLI"));
    assert!(help.contains("Usage: syndrid [OPTIONS] [PROMPT]"));
    assert!(help.contains("syndrid [OPTIONS] <COMMAND> [ARGS]"));
    Ok(())
}

#[test]
fn syndrid_version_uses_product_name() -> Result<()> {
    let version = run_binary("syndrid", "--version")?;

    assert_eq!(
        version,
        format!("SyndridCLI {}\n", env!("CARGO_PKG_VERSION"))
    );
    Ok(())
}
