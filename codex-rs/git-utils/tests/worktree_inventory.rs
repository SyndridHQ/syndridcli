use codex_git_utils::read_git_worktrees;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git should be available in the test environment");

    assert!(
        output.status.success(),
        "git {args:?} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[tokio::test]
async fn reads_real_linked_worktrees_and_marks_calling_worktree_current() {
    let repository = tempdir().expect("create repository tempdir");
    git(repository.path(), &["init"]);
    git(repository.path(), &["config", "user.email", "syndrid-test@example.invalid"]);
    git(repository.path(), &["config", "user.name", "Syndrid Test"]);

    fs::write(repository.path().join("README.md"), "worktree integration test\n")
        .expect("write initial file");
    git(repository.path(), &["add", "README.md"]);
    git(repository.path(), &["commit", "-m", "initial"]);

    let linked_parent = tempdir().expect("create linked-worktree parent");
    let linked = linked_parent.path().join("feature-worktree");
    let linked_arg = linked.to_string_lossy().into_owned();
    git(
        repository.path(),
        &["worktree", "add", "-b", "feature-worktree", &linked_arg],
    );

    let snapshot = read_git_worktrees(&linked, 16)
        .await
        .expect("linked worktree inventory should load");

    assert!(!snapshot.truncated);
    assert_eq!(snapshot.entries.len(), 2);
    assert_eq!(snapshot.entries.iter().filter(|entry| entry.current).count(), 1);

    let current = snapshot
        .entries
        .iter()
        .find(|entry| entry.current)
        .expect("calling linked worktree should be marked current");
    assert_eq!(current.branch.as_deref(), Some("feature-worktree"));
    assert!(!current.detached);
    assert!(!current.bare);

    let bounded = read_git_worktrees(&linked, 1)
        .await
        .expect("bounded linked worktree inventory should load");
    assert!(bounded.truncated);
    assert_eq!(bounded.entries.len(), 1);
}
