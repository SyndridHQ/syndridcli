use super::*;
use codex_utils_cli::PublicBrand;

#[test]
fn syndrid_home_uses_separator_layout_without_legacy_banner() {
    let cell = SessionHeaderHistoryCell::new(
        "gpt-test".to_string(),
        None,
        false,
        std::path::PathBuf::from("C:\\workspace"),
        "0.0.0",
    )
    .with_public_brand(PublicBrand::Syndrid)
    .with_session_id("session-1".to_string());
    let lines = cell.display_lines(100);
    let rendered = lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<String>();

    assert!(rendered.contains("Session ID: session-1"));
    assert!(rendered.contains("SYNDRID CONNECTED"));
    assert!(rendered.contains("Welcome back!"));
    assert!(rendered.contains("Lifetime Tokens: —"));
    assert!(!rendered.contains("github.com/SyndridHQ"));
    assert!(!rendered.contains("Patch Notes"));
    assert!(!rendered.contains("╭"));
    assert!(!rendered.contains("╰"));
    assert!(!rendered.contains(".-(* *)-."));
}
