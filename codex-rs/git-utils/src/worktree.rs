use std::borrow::Cow;
use std::path::Path;
use tokio::process::Command;
use tokio::time::Duration;
use tokio::time::timeout;

/// Maximum number of linked worktrees retained by the default inventory read.
/// Worktree inventories are normally small, but the app-server contract remains
/// bounded so an explicit UI read cannot grow without limit.
pub const DEFAULT_GIT_WORKTREE_LIMIT: usize = 256;

const GIT_WORKTREE_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const DISABLED_HOOKS_PATH: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitWorktreeEntry {
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
    pub locked: bool,
    pub lock_reason: Option<String>,
    pub prunable: bool,
    pub prune_reason: Option<String>,
    pub current: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitWorktreeSnapshot {
    pub entries: Vec<GitWorktreeEntry>,
    pub truncated: bool,
}

/// Read the repository's linked-worktree inventory using bounded,
/// non-interactive Git.
///
/// `--porcelain -z` is used so paths containing whitespace or newlines remain
/// unambiguous. This is an explicit read only: it does not prune, lock, unlock,
/// create, move, or remove worktrees.
pub async fn read_git_worktrees(cwd: &Path, entry_limit: usize) -> Option<GitWorktreeSnapshot> {
    let current_root = crate::get_git_repo_root(cwd)?;
    if entry_limit == 0 {
        return Some(GitWorktreeSnapshot {
            entries: Vec::new(),
            // A valid repository always contributes at least its primary worktree,
            // so a zero-cap inventory is necessarily truncated. Avoid spawning Git
            // just to rediscover that fact.
            truncated: true,
        });
    }

    let mut command = Command::new("git");
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["-c", &format!("core.hooksPath={DISABLED_HOOKS_PATH}")])
        .args(["-c", "core.fsmonitor=false"])
        .args(["worktree", "list", "--porcelain", "-z"])
        .current_dir(cwd)
        .kill_on_drop(true);

    let output = match timeout(GIT_WORKTREE_COMMAND_TIMEOUT, command.output()).await {
        Ok(Ok(output)) if output.status.success() => output,
        _ => return None,
    };

    Some(parse_worktree_porcelain_z(
        &output.stdout,
        entry_limit,
        current_root.as_path(),
    ))
}

/// Parse `git worktree list --porcelain -z` output.
///
/// With `-z`, each attribute is NUL-terminated and each worktree record is
/// followed by an additional NUL. Unknown attributes are ignored so the parser
/// remains forward-compatible with newer Git versions.
pub fn parse_worktree_porcelain_z(
    output: &[u8],
    entry_limit: usize,
    current_root: &Path,
) -> GitWorktreeSnapshot {
    let mut entries = Vec::new();
    let mut truncated = false;
    let mut pending = PendingWorktree::default();

    for field in output.split(|byte| *byte == 0) {
        if field.is_empty() {
            if let Some(entry) = pending.take(current_root) {
                if entries.len() < entry_limit {
                    entries.push(entry);
                } else {
                    truncated = true;
                }
            }
            continue;
        }

        if let Some(path) = field.strip_prefix(b"worktree ") {
            if pending.path.is_some()
                && let Some(entry) = pending.take(current_root)
            {
                if entries.len() < entry_limit {
                    entries.push(entry);
                } else {
                    truncated = true;
                }
            }
            pending.path = Some(decode(path));
        } else if let Some(head) = field.strip_prefix(b"HEAD ") {
            pending.head = non_empty(decode(head));
        } else if let Some(branch) = field.strip_prefix(b"branch ") {
            let branch = decode(branch);
            pending.branch = non_empty(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch.as_str())
                    .to_string(),
            );
        } else if field == b"detached" {
            pending.detached = true;
        } else if field == b"bare" {
            pending.bare = true;
        } else if let Some(reason) = marker_reason(field, b"locked") {
            pending.locked = true;
            pending.lock_reason = reason;
        } else if let Some(reason) = marker_reason(field, b"prunable") {
            pending.prunable = true;
            pending.prune_reason = reason;
        }
    }

    if let Some(entry) = pending.take(current_root) {
        if entries.len() < entry_limit {
            entries.push(entry);
        } else {
            truncated = true;
        }
    }

    GitWorktreeSnapshot { entries, truncated }
}

#[derive(Default)]
struct PendingWorktree {
    path: Option<String>,
    head: Option<String>,
    branch: Option<String>,
    detached: bool,
    bare: bool,
    locked: bool,
    lock_reason: Option<String>,
    prunable: bool,
    prune_reason: Option<String>,
}

impl PendingWorktree {
    fn take(&mut self, current_root: &Path) -> Option<GitWorktreeEntry> {
        let path = self.path.take()?;
        let entry = GitWorktreeEntry {
            current: Path::new(&path) == current_root,
            path,
            head: self.head.take(),
            branch: self.branch.take(),
            detached: self.detached,
            bare: self.bare,
            locked: self.locked,
            lock_reason: self.lock_reason.take(),
            prunable: self.prunable,
            prune_reason: self.prune_reason.take(),
        };
        self.detached = false;
        self.bare = false;
        self.locked = false;
        self.prunable = false;
        Some(entry)
    }
}

fn marker_reason(field: &[u8], marker: &[u8]) -> Option<Option<String>> {
    if field == marker {
        return Some(None);
    }
    let suffix = field.strip_prefix(marker)?;
    let reason = suffix.strip_prefix(b" ")?;
    Some(non_empty(decode(reason)))
}

fn decode(bytes: &[u8]) -> String {
    match String::from_utf8_lossy(bytes) {
        Cow::Borrowed(value) => value.to_string(),
        Cow::Owned(value) => value,
    }
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_detached_locked_and_prunable_worktrees() {
        let output = concat!(
            "worktree /repo\0",
            "HEAD 1111111111111111111111111111111111111111\0",
            "branch refs/heads/main\0\0",
            "worktree /repo-feature\0",
            "HEAD 2222222222222222222222222222222222222222\0",
            "detached\0",
            "locked maintenance window\0",
            "prunable gitdir file points to non-existent location\0\0",
        );

        let snapshot = parse_worktree_porcelain_z(output.as_bytes(), 10, Path::new("/repo"));
        assert!(!snapshot.truncated);
        assert_eq!(snapshot.entries.len(), 2);
        assert_eq!(snapshot.entries[0].path, "/repo");
        assert_eq!(snapshot.entries[0].branch.as_deref(), Some("main"));
        assert!(snapshot.entries[0].current);
        assert!(!snapshot.entries[0].detached);
        assert_eq!(
            snapshot.entries[1].head.as_deref(),
            Some("2222222222222222222222222222222222222222")
        );
        assert!(snapshot.entries[1].detached);
        assert!(snapshot.entries[1].locked);
        assert_eq!(
            snapshot.entries[1].lock_reason.as_deref(),
            Some("maintenance window")
        );
        assert!(snapshot.entries[1].prunable);
        assert_eq!(
            snapshot.entries[1].prune_reason.as_deref(),
            Some("gitdir file points to non-existent location")
        );
    }

    #[test]
    fn preserves_newlines_and_ignores_unknown_attributes() {
        let output = b"worktree /repo/line\nbreak\0HEAD abc\0future-field value\0\0";
        let snapshot = parse_worktree_porcelain_z(output, 10, Path::new("/other"));

        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].path, "/repo/line\nbreak");
        assert!(!snapshot.entries[0].current);
    }

    #[test]
    fn reports_truncation_without_stopping_record_parsing() {
        let output = b"worktree /one\0HEAD 1\0\0worktree /two\0HEAD 2\0\0worktree /three\0HEAD 3\0\0";
        let snapshot = parse_worktree_porcelain_z(output, 1, Path::new("/one"));

        assert!(snapshot.truncated);
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].path, "/one");
    }

    #[test]
    fn zero_limit_retains_no_entries_and_reports_truncation() {
        let output = b"worktree /one\0HEAD 1\0\0worktree /two\0HEAD 2\0\0";
        let snapshot = parse_worktree_porcelain_z(output, 0, Path::new("/one"));

        assert!(snapshot.truncated);
        assert!(snapshot.entries.is_empty());
    }

    #[test]
    fn record_state_does_not_leak_to_the_next_worktree() {
        let output = concat!(
            "worktree /locked\0",
            "HEAD 1\0",
            "detached\0",
            "bare\0",
            "locked maintenance\0",
            "prunable stale metadata\0\0",
            "worktree /clean\0",
            "HEAD 2\0",
            "branch refs/heads/clean\0\0",
        );
        let snapshot = parse_worktree_porcelain_z(output.as_bytes(), 10, Path::new("/clean"));

        assert_eq!(snapshot.entries.len(), 2);
        let clean = &snapshot.entries[1];
        assert_eq!(clean.path, "/clean");
        assert_eq!(clean.branch.as_deref(), Some("clean"));
        assert!(clean.current);
        assert!(!clean.detached);
        assert!(!clean.bare);
        assert!(!clean.locked);
        assert!(clean.lock_reason.is_none());
        assert!(!clean.prunable);
        assert!(clean.prune_reason.is_none());
    }

    #[test]
    fn accepts_final_record_without_extra_separator() {
        let output = b"worktree /repo\0HEAD abc\0branch refs/heads/feature\0";
        let snapshot = parse_worktree_porcelain_z(output, 10, Path::new("/repo"));

        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].branch.as_deref(), Some("feature"));
    }
}
