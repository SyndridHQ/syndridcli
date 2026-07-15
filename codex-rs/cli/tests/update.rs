use anyhow::Result;
use predicates::str::contains;
use std::path::Path;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("codex")?);
    cmd.env("CODEX_HOME", codex_home);
    Ok(cmd)
}

fn syndrid_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("syndrid")?);
    cmd.env("CODEX_HOME", codex_home);
    Ok(cmd)
}

#[test]
fn syndrid_update_uses_manual_release_message_without_openai_actions() -> Result<()> {
    let codex_home = TempDir::new()?;
    let output = syndrid_command(codex_home.path())?.arg("update").output()?;
    let stderr = String::from_utf8(output.stderr)?;

    assert!(!output.status.success());
    assert!(stderr.contains(
        "SyndridCLI automatic updates are not available yet.\n\
Download the latest release from:\n\
https://github.com/SyndridHQ/syndridcli/releases/latest"
    ));
    for forbidden in [
        "@openai/codex",
        "brew upgrade --cask codex",
        "chatgpt.com/codex",
        "github.com/openai/codex",
        "developers.openai.com/codex",
    ] {
        assert!(
            !stderr.contains(forbidden),
            "unexpected update target: {forbidden}"
        );
    }

    Ok(())
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn update_does_not_start_interactive_prompt() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .arg("update")
        .assert()
        .failure()
        .stderr(contains("`codex update` is not available in debug builds"));

    Ok(())
}
