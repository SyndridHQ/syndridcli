use codex_utils_absolute_path::AbsolutePathBuf;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

/// Read the selected repository's index and working-tree status.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct GitStatusParams {
    /// Absolute working directory inside the repository to inspect.
    pub cwd: AbsolutePathBuf,
    /// Maximum number of status entries to retain. The runtime applies its
    /// bounded default and maximum when omitted or set above the supported cap.
    #[ts(optional = nullable)]
    pub limit: Option<u32>,
}

/// A single side of a porcelain-v1 Git status record.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
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

/// Typed file status reported by the Syndrid runtime.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct GitStatusEntry {
    /// Current path in the worktree.
    pub path: String,
    /// Original path when Git reports a rename or copy.
    pub previous_path: Option<String>,
    /// Index/staging-area state.
    pub index_status: GitStatusCode,
    /// Working-tree state.
    pub worktree_status: GitStatusCode,
}

/// Bounded repository status returned by `git/status`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct GitStatusResponse {
    pub entries: Vec<GitStatusEntry>,
    /// True when more entries existed than the runtime response retained.
    pub truncated: bool,
}

/// Read the repository's linked-worktree inventory.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct GitWorktreeListParams {
    /// Absolute working directory inside the repository to inspect.
    pub cwd: AbsolutePathBuf,
    /// Maximum number of worktree entries to retain. The runtime applies its
    /// bounded default and maximum when omitted or set above the supported cap.
    #[ts(optional = nullable)]
    pub limit: Option<u32>,
}

/// A linked Git worktree reported by the Syndrid runtime.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct GitWorktreeEntry {
    /// Native absolute worktree path as reported by Git.
    pub path: String,
    pub head: Option<String>,
    /// Local branch name without the `refs/heads/` prefix when attached.
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
    pub locked: bool,
    pub lock_reason: Option<String>,
    pub prunable: bool,
    pub prune_reason: Option<String>,
    /// True for the worktree containing the request cwd.
    pub current: bool,
}

/// Bounded linked-worktree inventory returned by `git/worktree/list`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct GitWorktreeListResponse {
    pub entries: Vec<GitWorktreeEntry>,
    /// True when more entries existed than the runtime response retained.
    pub truncated: bool,
}

/// Mutate the Git index for a bounded set of exact repository-relative paths.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct GitPathMutationParams {
    /// Absolute working directory inside the repository to mutate.
    pub cwd: AbsolutePathBuf,
    /// Exact repository-relative paths previously supplied by the runtime.
    pub paths: Vec<String>,
}

/// Result of a bounded Git index mutation.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct GitPathMutationResponse {
    /// Number of paths accepted by the completed mutation.
    pub updated: u32,
}
