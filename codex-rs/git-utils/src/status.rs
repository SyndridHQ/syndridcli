use std::borrow::Cow;
use std::path::Path;
use tokio::process::Command;
use tokio::time::Duration;
use tokio::time::timeout;

/// Maximum number of working-tree entries retained by callers that use the
/// default status parser. Keeping this bounded prevents unexpectedly large
/// repositories from turning a read-only status request into an unbounded
/// allocation.
pub const DEFAULT_GIT_STATUS_ENTRY_LIMIT: usize = 2_500;

const GIT_STATUS_COMMAND_TIMEOUT: Duration = Duration::from_secs(/*secs*/ 5);
const DISABLED_HOOKS_PATH: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitStatusCode {
    Unmodified,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Unmerged,
    Untracked,
    Ignored,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitStatusEntry {
    pub path: String,
    pub previous_path: Option<String>,
    pub index_status: GitStatusCode,
    pub worktree_status: GitStatusCode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitStatusSnapshot {
    pub entries: Vec<GitStatusEntry>,
    pub truncated: bool,
}

/// Read the repository index/worktree status using bounded, non-interactive Git.
///
/// This keeps status execution inside SyndridCLI rather than desktop clients.
/// The command has the same five-second ceiling used by the existing Git
/// metadata helpers, disables repository hooks/fsmonitor helpers, and uses
/// Git's normal untracked-directory rollup so a large untracked tree does not
/// explode into one process-output record per descendant before the parser can
/// apply its retained-entry limit.
pub async fn read_git_status(cwd: &Path, entry_limit: usize) -> Option<GitStatusSnapshot> {
    crate::get_git_repo_root(cwd)?;

    let mut command = Command::new("git");
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["-c", &format!("core.hooksPath={DISABLED_HOOKS_PATH}")])
        .args(["-c", "core.fsmonitor=false"])
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=normal"])
        .current_dir(cwd)
        .kill_on_drop(true);

    let output = match timeout(GIT_STATUS_COMMAND_TIMEOUT, command.output()).await {
        Ok(Ok(output)) if output.status.success() => output,
        _ => return None,
    };

    Some(parse_porcelain_v1_z(&output.stdout, entry_limit))
}

/// Parse `git status --porcelain=v1 -z` output.
///
/// NUL-delimited porcelain is intentionally used instead of line-oriented
/// output so filenames containing whitespace or newlines remain lossless. For
/// rename/copy records Git emits the destination path first and the source path
/// in the following NUL-delimited field when `-z` is enabled.
pub fn parse_porcelain_v1_z(output: &[u8], entry_limit: usize) -> GitStatusSnapshot {
    let mut fields = output.split(|byte| *byte == 0).peekable();
    let mut entries = Vec::new();
    let mut truncated = false;

    while let Some(record) = fields.next() {
        if record.is_empty() {
            continue;
        }
        if record.len() < 3 || record[2] != b' ' {
            continue;
        }

        let index_status = decode_status(record[0], true);
        let worktree_status = decode_status(record[1], false);
        let path = decode_path(&record[3..]);
        if path.is_empty() {
            continue;
        }

        let previous_path =
            if matches!(index_status, GitStatusCode::Renamed | GitStatusCode::Copied)
                || matches!(
                    worktree_status,
                    GitStatusCode::Renamed | GitStatusCode::Copied
                )
            {
                fields
                    .next()
                    .filter(|value| !value.is_empty())
                    .map(decode_path)
                    .filter(|value| !value.is_empty())
            } else {
                None
            };

        if entries.len() >= entry_limit {
            truncated = true;
            continue;
        }

        entries.push(GitStatusEntry {
            path,
            previous_path,
            index_status,
            worktree_status,
        });
    }

    GitStatusSnapshot { entries, truncated }
}

fn decode_status(code: u8, index: bool) -> GitStatusCode {
    match code {
        b'M' => GitStatusCode::Modified,
        b'A' => GitStatusCode::Added,
        b'D' => GitStatusCode::Deleted,
        b'R' => GitStatusCode::Renamed,
        b'C' => GitStatusCode::Copied,
        b'U' => GitStatusCode::Unmerged,
        b'?' if !index => GitStatusCode::Untracked,
        b'!' if !index => GitStatusCode::Ignored,
        _ => GitStatusCode::Unmodified,
    }
}

fn decode_path(bytes: &[u8]) -> String {
    match String::from_utf8_lossy(bytes) {
        Cow::Borrowed(value) => value.to_string(),
        Cow::Owned(value) => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_staged_unstaged_untracked_and_deleted_entries() {
        let status = parse_porcelain_v1_z(
            b"M  staged.rs\0 M unstaged.rs\0?? new file.txt\0 D removed.rs\0",
            DEFAULT_GIT_STATUS_ENTRY_LIMIT,
        );

        assert!(!status.truncated);
        assert_eq!(status.entries.len(), 4);
        assert_eq!(status.entries[0].index_status, GitStatusCode::Modified);
        assert_eq!(status.entries[0].worktree_status, GitStatusCode::Unmodified);
        assert_eq!(status.entries[1].index_status, GitStatusCode::Unmodified);
        assert_eq!(status.entries[1].worktree_status, GitStatusCode::Modified);
        assert_eq!(status.entries[2].path, "new file.txt");
        assert_eq!(status.entries[2].worktree_status, GitStatusCode::Untracked);
        assert_eq!(status.entries[3].worktree_status, GitStatusCode::Deleted);
    }

    #[test]
    fn parses_nul_delimited_rename_without_arrow_syntax() {
        let status = parse_porcelain_v1_z(
            b"R  destination name.rs\0source name.rs\0",
            DEFAULT_GIT_STATUS_ENTRY_LIMIT,
        );

        assert_eq!(status.entries.len(), 1);
        assert_eq!(status.entries[0].path, "destination name.rs");
        assert_eq!(
            status.entries[0].previous_path.as_deref(),
            Some("source name.rs")
        );
        assert_eq!(status.entries[0].index_status, GitStatusCode::Renamed);
    }

    #[test]
    fn preserves_embedded_newlines_in_paths() {
        let status = parse_porcelain_v1_z(b"?? line\nbreak.txt\0", DEFAULT_GIT_STATUS_ENTRY_LIMIT);

        assert_eq!(status.entries[0].path, "line\nbreak.txt");
    }

    #[test]
    fn reports_truncation_while_consuming_rename_source_fields() {
        let status = parse_porcelain_v1_z(
            b"M  first.rs\0R  renamed.rs\0old.rs\0?? third.rs\0",
            /*entry_limit*/ 1,
        );

        assert!(status.truncated);
        assert_eq!(status.entries.len(), 1);
        assert_eq!(status.entries[0].path, "first.rs");
    }

    #[test]
    fn parses_unmerged_status_codes() {
        let status = parse_porcelain_v1_z(b"UU conflict.rs\0", DEFAULT_GIT_STATUS_ENTRY_LIMIT);

        assert_eq!(status.entries[0].index_status, GitStatusCode::Unmerged);
        assert_eq!(status.entries[0].worktree_status, GitStatusCode::Unmerged);
    }
}
