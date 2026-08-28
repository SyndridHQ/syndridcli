use std::collections::HashSet;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use thiserror::Error;
use tokio::process::Command;
use tokio::time::Duration;
use tokio::time::timeout;

pub const MAX_GIT_MUTATION_PATHS: usize = 256;
pub const MAX_GIT_MUTATION_PATH_CHARS: usize = 32_768;
pub const MAX_GIT_MUTATION_TOTAL_CHARS: usize = 1_048_576;

const GIT_MUTATION_COMMAND_TIMEOUT: Duration = Duration::from_secs(/* secs */ 5);
const DISABLED_HOOKS_PATH: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

#[derive(Debug, Error)]
pub enum GitPathMutationError {
    #[error("{cwd:?} is not a git repository")]
    NotAGitRepository { cwd: PathBuf },
    #[error("at least one repository-relative path is required")]
    EmptyPaths,
    #[error("too many git paths: {count} exceeds the {limit} path limit")]
    TooManyPaths { count: usize, limit: usize },
    #[error("git path is invalid: {path:?}")]
    InvalidPath { path: String },
    #[error("git path is too long")]
    PathTooLong,
    #[error("combined git path input is too large")]
    PathsTooLarge,
    #[error("git {operation} timed out")]
    TimedOut { operation: &'static str },
    #[error("failed to start git {operation}: {source}")]
    Spawn {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("git {operation} failed: {stderr}")]
    CommandFailed {
        operation: &'static str,
        stderr: String,
    },
}

pub async fn stage_git_paths(cwd: &Path, paths: &[String]) -> Result<usize, GitPathMutationError> {
    mutate_git_paths(cwd, paths, GitPathMutation::Stage).await
}

pub async fn unstage_git_paths(
    cwd: &Path,
    paths: &[String],
) -> Result<usize, GitPathMutationError> {
    mutate_git_paths(cwd, paths, GitPathMutation::Unstage).await
}

#[derive(Clone, Copy)]
enum GitPathMutation {
    Stage,
    Unstage,
}

impl GitPathMutation {
    fn operation(self) -> &'static str {
        match self {
            Self::Stage => "stage",
            Self::Unstage => "unstage",
        }
    }
}

async fn mutate_git_paths(
    cwd: &Path,
    paths: &[String],
    mutation: GitPathMutation,
) -> Result<usize, GitPathMutationError> {
    validate_paths(paths)?;
    let unique_paths = deduplicate_paths(paths);
    let Some(repo_root) = crate::get_git_repo_root(cwd) else {
        return Err(GitPathMutationError::NotAGitRepository {
            cwd: cwd.to_path_buf(),
        });
    };

    let operation = mutation.operation();
    let has_head = match mutation {
        GitPathMutation::Stage => true,
        GitPathMutation::Unstage => repository_has_head(&repo_root, operation).await?,
    };

    let mut command = git_mutation_command();
    match mutation {
        GitPathMutation::Stage => {
            command.arg("add").arg("--");
        }
        GitPathMutation::Unstage if has_head => {
            command.args(["reset", "--quiet", "HEAD", "--"]);
        }
        GitPathMutation::Unstage => {
            // An unborn branch has no HEAD to reset against. Every staged entry is
            // therefore an addition, so removing the exact path from the index is
            // the equivalent unstage operation while preserving the worktree file.
            // Force is required when the worktree changed again after staging: Git
            // otherwise refuses to remove an index entry that differs from both the
            // worktree and HEAD (which does not exist on an unborn branch).
            command.args(["rm", "--cached", "--quiet", "--force", "--"]);
        }
    }

    command
        .args(&unique_paths)
        .current_dir(&repo_root)
        .kill_on_drop(true);
    let output = match timeout(GIT_MUTATION_COMMAND_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(source)) => return Err(GitPathMutationError::Spawn { operation, source }),
        Err(_) => return Err(GitPathMutationError::TimedOut { operation }),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitPathMutationError::CommandFailed {
            operation,
            stderr: truncate_error(&stderr),
        });
    }

    Ok(unique_paths.len())
}

fn git_mutation_command() -> Command {
    let mut command = Command::new("git");
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("--literal-pathspecs")
        .args(["-c", &format!("core.hooksPath={DISABLED_HOOKS_PATH}")])
        .args(["-c", "core.fsmonitor=false"]);
    command
}

async fn repository_has_head(
    cwd: &Path,
    operation: &'static str,
) -> Result<bool, GitPathMutationError> {
    let mut command = git_mutation_command();
    command
        .args(["rev-parse", "--verify", "--quiet", "HEAD"])
        .current_dir(cwd)
        .kill_on_drop(true);
    let output = match timeout(GIT_MUTATION_COMMAND_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(source)) => return Err(GitPathMutationError::Spawn { operation, source }),
        Err(_) => return Err(GitPathMutationError::TimedOut { operation }),
    };
    Ok(output.status.success())
}

fn validate_paths(paths: &[String]) -> Result<(), GitPathMutationError> {
    if paths.is_empty() {
        return Err(GitPathMutationError::EmptyPaths);
    }
    if paths.len() > MAX_GIT_MUTATION_PATHS {
        return Err(GitPathMutationError::TooManyPaths {
            count: paths.len(),
            limit: MAX_GIT_MUTATION_PATHS,
        });
    }

    let mut total_chars = 0usize;
    for path in paths {
        if path.is_empty() || path.contains('\0') {
            return Err(GitPathMutationError::InvalidPath { path: path.clone() });
        }
        let char_count = path.chars().count();
        if char_count > MAX_GIT_MUTATION_PATH_CHARS {
            return Err(GitPathMutationError::PathTooLong);
        }
        total_chars = total_chars.saturating_add(char_count);
        if total_chars > MAX_GIT_MUTATION_TOTAL_CHARS {
            return Err(GitPathMutationError::PathsTooLarge);
        }

        let parsed = Path::new(path);
        if parsed.is_absolute()
            || parsed.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(GitPathMutationError::InvalidPath { path: path.clone() });
        }
    }
    Ok(())
}

fn deduplicate_paths(paths: &[String]) -> Vec<&str> {
    let mut seen = HashSet::with_capacity(paths.len());
    paths
        .iter()
        .filter_map(|path| {
            let path = path.as_str();
            seen.insert(path).then_some(path)
        })
        .collect()
}

fn truncate_error(stderr: &str) -> String {
    const MAX_ERROR_CHARS: usize = 8_192;
    if stderr.chars().count() <= MAX_ERROR_CHARS {
        return stderr.trim().to_string();
    }
    let mut value = stderr.chars().take(MAX_ERROR_CHARS).collect::<String>();
    value.push('…');
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn accepts_repository_relative_paths() {
        assert!(
            validate_paths(&[
                "src/lib.rs".to_string(),
                "file with spaces.txt".to_string(),
                "nested/.hidden".to_string(),
                "literal[brackets]*?.txt".to_string(),
            ])
            .is_ok()
        );
    }

    #[test]
    fn rejects_absolute_and_parent_paths() {
        assert!(validate_paths(&["../outside".to_string()]).is_err());
        assert!(validate_paths(&["nested/../../outside".to_string()]).is_err());
        assert!(validate_paths(&["/absolute/path".to_string()]).is_err());
    }

    #[test]
    fn rejects_empty_nul_and_excessive_path_sets() {
        assert!(validate_paths(&[]).is_err());
        assert!(validate_paths(&["bad\0path".to_string()]).is_err());
        let paths = vec!["file".to_string(); MAX_GIT_MUTATION_PATHS + 1];
        assert!(validate_paths(&paths).is_err());
    }

    #[test]
    fn deduplicates_paths_without_changing_order() {
        let paths = vec![
            "a.txt".to_string(),
            "b.txt".to_string(),
            "a.txt".to_string(),
            "c.txt".to_string(),
            "b.txt".to_string(),
        ];
        assert_eq!(deduplicate_paths(&paths), vec!["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn bounds_error_text() {
        let text = "x".repeat(9_000);
        let bounded = truncate_error(&text);
        assert_eq!(bounded.chars().count(), 8_193);
        assert!(bounded.ends_with('…'));
    }

    #[tokio::test]
    async fn stage_treats_pathspec_metacharacters_literally() {
        let repo = tempfile::tempdir().expect("create temporary repository");
        let init = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(repo.path())
            .status()
            .await
            .expect("start git init");
        assert!(init.success());

        let literal_path = "file[ab].txt";
        let pattern_match = "filea.txt";
        fs::write(repo.path().join(literal_path), "literal\n").expect("write literal path");
        fs::write(repo.path().join(pattern_match), "pattern match\n").expect("write matching path");

        let updated = stage_git_paths(repo.path(), &[literal_path.to_string()])
            .await
            .expect("stage literal path");
        assert_eq!(updated, 1);

        let output = Command::new("git")
            .args(["ls-files", "--cached", "-z"])
            .current_dir(repo.path())
            .output()
            .await
            .expect("read staged paths");
        assert!(output.status.success());
        let staged = String::from_utf8(output.stdout)
            .expect("git paths are utf-8 for this fixture");
        let staged = staged
            .split('\0')
            .filter(|path| !path.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(staged, vec![literal_path]);
    }

    #[tokio::test]
    async fn stage_repo_root_relative_path_from_subdirectory() {
        let repo = tempfile::tempdir().expect("create temporary repository");
        let init = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(repo.path())
            .status()
            .await
            .expect("start git init");
        assert!(init.success());

        let nested = repo.path().join("nested");
        fs::create_dir(&nested).expect("create nested directory");
        let path = "nested/from-root.txt";
        fs::write(repo.path().join(path), "root-relative\n").expect("write nested fixture");

        let updated = stage_git_paths(&nested, &[path.to_string()])
            .await
            .expect("stage repo-root-relative path from nested cwd");
        assert_eq!(updated, 1);

        let output = Command::new("git")
            .args(["ls-files", "--cached", "-z"])
            .current_dir(repo.path())
            .output()
            .await
            .expect("read staged paths");
        assert!(output.status.success());
        assert_eq!(output.stdout, b"nested/from-root.txt\0");
    }

    #[tokio::test]
    async fn unstage_works_before_first_commit() {
        let repo = tempfile::tempdir().expect("create temporary repository");
        let init = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(repo.path())
            .status()
            .await
            .expect("start git init");
        assert!(init.success());

        let path = "first-file.txt";
        fs::write(repo.path().join(path), "first commit\n").expect("write fixture");
        stage_git_paths(repo.path(), &[path.to_string()])
            .await
            .expect("stage fixture");

        let updated = unstage_git_paths(repo.path(), &[path.to_string()])
            .await
            .expect("unstage fixture before first commit");
        assert_eq!(updated, 1);
        assert!(repo.path().join(path).exists());

        let output = Command::new("git")
            .args(["ls-files", "--cached", "-z"])
            .current_dir(repo.path())
            .output()
            .await
            .expect("read staged paths");
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
    }

    #[tokio::test]
    async fn unstage_preserves_modified_worktree_before_first_commit() {
        let repo = tempfile::tempdir().expect("create temporary repository");
        let init = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(repo.path())
            .status()
            .await
            .expect("start git init");
        assert!(init.success());

        let path = "first-file.txt";
        fs::write(repo.path().join(path), "staged version\n").expect("write staged fixture");
        stage_git_paths(repo.path(), &[path.to_string()])
            .await
            .expect("stage fixture");
        fs::write(repo.path().join(path), "newer worktree version\n")
            .expect("modify fixture after staging");

        let updated = unstage_git_paths(repo.path(), &[path.to_string()])
            .await
            .expect("unstage modified fixture before first commit");
        assert_eq!(updated, 1);
        assert_eq!(
            fs::read_to_string(repo.path().join(path)).expect("read preserved worktree fixture"),
            "newer worktree version\n",
        );

        let output = Command::new("git")
            .args(["ls-files", "--cached", "-z"])
            .current_dir(repo.path())
            .output()
            .await
            .expect("read staged paths");
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
    }
}
