use super::*;

#[test]
fn device_code_prompt_renders_phishing_warning() {
    let prompt = device_code_prompt(
        "https://example.com/device",
        "ABCD-EFGH",
        PublicBrand::Codex,
    );

    assert!(prompt.contains(
        "\x1b[90mContinue only if you started this login in Codex. If a website or another person gave you this code, cancel.\x1b[0m"
    ));
}

#[test]
fn syndrid_device_code_prompt_names_provider() {
    let prompt = device_code_prompt(
        "https://example.com/device",
        "ABCD-EFGH",
        PublicBrand::Syndrid,
    );

    assert!(prompt.contains("Welcome to SyndridCLI"));
    assert!(prompt.contains("Authentication is provided by OpenAI/ChatGPT"));
    assert!(prompt.contains("started this login in SyndridCLI"));
}
