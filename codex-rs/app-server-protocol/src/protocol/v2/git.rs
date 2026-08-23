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
