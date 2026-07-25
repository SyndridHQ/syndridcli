use super::subagent_tools::SubagentSessionBudget;
use super::subagent_tools::SubagentToolError;
use super::subagent_tools::SubagentToolKind;
use super::subagent_tools::SubagentToolPolicy;
use super::subagent_tools::execute_tool;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::fs;
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;

fn policy(root: &std::path::Path) -> SubagentToolPolicy {
    SubagentToolPolicy::for_workspace(root, SubagentSessionBudget::default()).unwrap()
}

#[tokio::test]
async fn read_file_returns_bounded_utf8_lines() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("note.txt"), "one\ntwo\nthree\n").unwrap();
    let policy = policy(directory.path()).approve(SubagentToolKind::ReadFile);

    let result = execute_tool(
        &policy,
        SubagentToolKind::ReadFile,
        "call-1",
        r#"{"path":"note.txt","start_line":2,"max_lines":1}"#,
        &CancellationToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(result.content, "2: two");
    assert!(!result.truncated);
}

#[tokio::test]
async fn read_file_rejects_absolute_traversal_drive_and_directory_paths() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("note.txt"), "content").unwrap();
    fs::create_dir(directory.path().join("folder")).unwrap();
    let policy = policy(directory.path()).approve(SubagentToolKind::ReadFile);
    for path in [
        directory.path().join("note.txt").display().to_string(),
        "../note.txt".to_string(),
        "C:\\outside.txt".to_string(),
        "\\\\server\\share\\outside.txt".to_string(),
        "folder".to_string(),
    ] {
        let arguments = json!({ "path": path }).to_string();
        assert!(matches!(
            execute_tool(
                &policy,
                SubagentToolKind::ReadFile,
                "call-1",
                &arguments,
                &CancellationToken::new(),
            )
            .await,
            Err(SubagentToolError::InvalidPath)
                | Err(SubagentToolError::NotARegularFile)
                | Err(SubagentToolError::PathOutsideWorkspace)
        ));
    }
}

#[tokio::test]
async fn read_file_rejects_invalid_utf8_and_oversized_files() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("binary.bin"), [0xff, 0xfe]).unwrap();
    let invalid_policy = policy(directory.path()).approve(SubagentToolKind::ReadFile);
    let invalid = execute_tool(
        &invalid_policy,
        SubagentToolKind::ReadFile,
        "call-1",
        r#"{"path":"binary.bin"}"#,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(invalid, Err(SubagentToolError::InvalidUtf8)));

    let mut budget = SubagentSessionBudget::default();
    budget.max_file_bytes = 1;
    fs::write(directory.path().join("large.txt"), "12").unwrap();
    let policy = SubagentToolPolicy::for_workspace(directory.path(), budget)
        .unwrap()
        .approve(SubagentToolKind::ReadFile);

    let oversized = execute_tool(
        &policy,
        SubagentToolKind::ReadFile,
        "call-1",
        r#"{"path":"large.txt"}"#,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(oversized, Err(SubagentToolError::FileTooLarge)));
}

#[tokio::test]
async fn search_text_is_literal_deterministic_and_bounded() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("b.txt"), "needle b\n").unwrap();
    fs::write(directory.path().join("a.txt"), "needle a\n").unwrap();
    let mut budget = SubagentSessionBudget::default();
    budget.max_search_results = 1;
    let policy = SubagentToolPolicy::for_workspace(directory.path(), budget)
        .unwrap()
        .approve(SubagentToolKind::SearchText);

    let result = execute_tool(
        &policy,
        SubagentToolKind::SearchText,
        "call-1",
        r#"{"query":"needle"}"#,
        &CancellationToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(result.content, "a.txt:1: needle a");
    assert!(result.truncated);
}

#[tokio::test]
async fn search_text_skips_binary_and_rejects_invalid_scope() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("binary.bin"), [0xff, 0xfe]).unwrap();
    let policy = policy(directory.path()).approve(SubagentToolKind::SearchText);
    let result = execute_tool(
        &policy,
        SubagentToolKind::SearchText,
        "call-1",
        r#"{"query":"needle","path":"../"}"#,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(result, Err(SubagentToolError::InvalidPath)));
}

#[tokio::test]
async fn git_status_uses_fixed_read_only_shape() {
    let directory = tempdir().unwrap();
    let policy = policy(directory.path()).approve(SubagentToolKind::GitStatus);
    let result = execute_tool(
        &policy,
        SubagentToolKind::GitStatus,
        "call-1",
        "{}",
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(
        result,
        Ok(_) | Err(SubagentToolError::GitStatusFailed)
    ));
}

#[tokio::test]
async fn unapproved_tools_and_malformed_inputs_execute_nothing() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("note.txt"), "content").unwrap();
    let policy = policy(directory.path());
    assert!(matches!(
        execute_tool(
            &policy,
            SubagentToolKind::ReadFile,
            "call-1",
            r#"{"path":"note.txt"}"#,
            &CancellationToken::new(),
        )
        .await,
        Err(SubagentToolError::ToolNotApproved)
    ));
    let approved = policy.approve(SubagentToolKind::ReadFile);
    assert!(matches!(
        execute_tool(
            &approved,
            SubagentToolKind::ReadFile,
            "call-1",
            r#"{"path":"note.txt","extra":true}"#,
            &CancellationToken::new(),
        )
        .await,
        Err(SubagentToolError::InvalidInput)
    ));
}

#[tokio::test]
async fn cancellation_before_execution_is_safe() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("note.txt"), "content").unwrap();
    let policy = policy(directory.path()).approve(SubagentToolKind::ReadFile);
    let cancellation = CancellationToken::new();
    cancellation.cancel();

    assert!(matches!(
        execute_tool(
            &policy,
            SubagentToolKind::ReadFile,
            "call-1",
            r#"{"path":"note.txt"}"#,
            &cancellation,
        )
        .await,
        Err(SubagentToolError::Cancelled)
    ));
}

#[test]
fn policy_debug_excludes_workspace_path_contents() {
    let directory = tempdir().unwrap();
    let debug = format!("{:?}", policy(directory.path()));
    assert!(!debug.contains(&directory.path().display().to_string()));
}
