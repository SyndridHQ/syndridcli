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

const GIT_MUTATION_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
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
    if crate::get_git_repo_root(cwd).is_none() {
        return Err(GitPathMutationError::NotAGitRepository {
            cwd: cwd.to_path_buf(),
        });
    }

    let mut command = Command::new("git");
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["-c", &format!("core.hooksPath={DISABLED_HOOKS_PATH}")])
        .args(["-c", "core.fsmonitor=false"]);

    match mutation {
        GitPathMutation::Stage => {
            command.arg("add").arg("--");
        }
        GitPathMutation::Unstage => {
            command.args(["reset", "--quiet", "HEAD", "--"]);
        }
    }

    command.args(paths).current_dir(cwd).kill_on_drop(true);
    let operation = mutation.operation();
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

    Ok(paths.len())
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

    #[test]
    fn accepts_repository_relative_paths() {
        assert!(validate_paths(&[
            "src/lib.rs".to_string(),
            "file with spaces.txt".to_string(),
            "nested/.hidden".to_string(),
        ])
        .is_ok());
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
    fn bounds_error_text() {
        let text = "x".repeat(9_000);
        let bounded = truncate_error(&text);
        assert_eq!(bounded.chars().count(), 8_193);
        assert!(bounded.ends_with('…'));
    }
}
