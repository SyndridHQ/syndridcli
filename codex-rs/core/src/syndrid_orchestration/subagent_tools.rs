use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const DEFAULT_MAX_PROVIDER_TURNS: usize = 8;
const DEFAULT_MAX_TOOL_CALLS: usize = 16;
const DEFAULT_MAX_CALLS_PER_TOOL: usize = 8;
const DEFAULT_MAX_TOOL_INPUT_BYTES: usize = 16 * 1024;
const DEFAULT_MAX_TOOL_OUTPUT_BYTES: usize = 32 * 1024;
const DEFAULT_MAX_AGGREGATE_TOOL_OUTPUT_BYTES: usize = 128 * 1024;
const DEFAULT_MAX_FILE_BYTES: usize = 256 * 1024;
const DEFAULT_MAX_FILE_READ_BYTES: usize = 32 * 1024;
const DEFAULT_MAX_FILE_READ_LINES: usize = 512;
const DEFAULT_MAX_SEARCH_RESULTS: usize = 64;
const DEFAULT_MAX_SEARCH_FILES: usize = 256;
const DEFAULT_MAX_SEARCH_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_MAX_GIT_STATUS_ENTRIES: usize = 256;
const DEFAULT_MAX_GIT_OUTPUT_BYTES: usize = 64 * 1024;
const DEFAULT_SESSION_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_TOOL_TIMEOUT: Duration = Duration::from_secs(5);

/// The only repository tools that O6B can approve.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentToolKind {
    ReadFile,
    SearchText,
    GitStatus,
}

impl SubagentToolKind {
    pub(crate) fn provider_name(self) -> &'static str {
        match self {
            Self::ReadFile => "read_file",
            Self::SearchText => "search_text",
            Self::GitStatus => "git_status",
        }
    }
}

/// Hard bounds for one sequential approved-tool session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubagentSessionBudget {
    pub max_provider_turns: usize,
    pub max_tool_calls: usize,
    pub max_calls_per_tool: usize,
    pub max_tool_input_bytes: usize,
    pub max_tool_output_bytes: usize,
    pub max_aggregate_tool_output_bytes: usize,
    pub max_file_bytes: usize,
    pub max_file_read_bytes: usize,
    pub max_file_read_lines: usize,
    pub max_search_results: usize,
    pub max_search_files: usize,
    pub max_search_bytes: usize,
    pub max_git_status_entries: usize,
    pub max_git_output_bytes: usize,
    pub session_timeout: Duration,
    pub per_tool_timeout: Duration,
}

impl Default for SubagentSessionBudget {
    fn default() -> Self {
        Self {
            max_provider_turns: DEFAULT_MAX_PROVIDER_TURNS,
            max_tool_calls: DEFAULT_MAX_TOOL_CALLS,
            max_calls_per_tool: DEFAULT_MAX_CALLS_PER_TOOL,
            max_tool_input_bytes: DEFAULT_MAX_TOOL_INPUT_BYTES,
            max_tool_output_bytes: DEFAULT_MAX_TOOL_OUTPUT_BYTES,
            max_aggregate_tool_output_bytes: DEFAULT_MAX_AGGREGATE_TOOL_OUTPUT_BYTES,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_file_read_bytes: DEFAULT_MAX_FILE_READ_BYTES,
            max_file_read_lines: DEFAULT_MAX_FILE_READ_LINES,
            max_search_results: DEFAULT_MAX_SEARCH_RESULTS,
            max_search_files: DEFAULT_MAX_SEARCH_FILES,
            max_search_bytes: DEFAULT_MAX_SEARCH_BYTES,
            max_git_status_entries: DEFAULT_MAX_GIT_STATUS_ENTRIES,
            max_git_output_bytes: DEFAULT_MAX_GIT_OUTPUT_BYTES,
            session_timeout: DEFAULT_SESSION_TIMEOUT,
            per_tool_timeout: DEFAULT_TOOL_TIMEOUT,
        }
    }
}

/// Caller-owned approval and workspace boundary for one subagent session.
#[derive(Clone)]
pub struct SubagentToolPolicy {
    approved_tools: BTreeSet<SubagentToolKind>,
    workspace_root: Option<PathBuf>,
    budget: SubagentSessionBudget,
}

impl fmt::Debug for SubagentToolPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubagentToolPolicy")
            .field("approved_tools", &self.approved_tools)
            .field("has_workspace_root", &self.workspace_root.is_some())
            .field("budget", &self.budget)
            .finish()
    }
}

impl SubagentToolPolicy {
    pub fn empty() -> Self {
        Self {
            approved_tools: BTreeSet::new(),
            workspace_root: None,
            budget: SubagentSessionBudget::default(),
        }
    }

    pub fn for_workspace(
        workspace_root: impl Into<PathBuf>,
        budget: SubagentSessionBudget,
    ) -> Result<Self, SubagentToolError> {
        let root = dunce::canonicalize(workspace_root.into())
            .map_err(|_| SubagentToolError::InvalidWorkspace)?;
        if !root.is_dir() {
            return Err(SubagentToolError::InvalidWorkspace);
        }
        Ok(Self {
            approved_tools: BTreeSet::new(),
            workspace_root: Some(root),
            budget,
        })
    }

    pub fn approve(mut self, tool: SubagentToolKind) -> Self {
        self.approved_tools.insert(tool);
        self
    }

    pub(crate) fn approved_tools(&self) -> &BTreeSet<SubagentToolKind> {
        &self.approved_tools
    }

    pub(crate) fn workspace_root(&self) -> Option<&Path> {
        self.workspace_root.as_deref()
    }

    pub(crate) fn budget(&self) -> &SubagentSessionBudget {
        &self.budget
    }

    pub(crate) fn requires_workspace(&self) -> bool {
        !self.approved_tools.is_empty()
    }

    pub(crate) fn with_repair_limits(
        &self,
        max_provider_turns: usize,
        max_tool_calls: usize,
    ) -> Self {
        let mut budget = self.budget.clone();
        budget.max_provider_turns = budget.max_provider_turns.min(max_provider_turns);
        budget.max_tool_calls = budget.max_tool_calls.min(max_tool_calls);
        Self {
            approved_tools: self.approved_tools.clone(),
            workspace_root: self.workspace_root.clone(),
            budget,
        }
    }
}

/// A bounded, secret-free record of one attempted tool call.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SubagentToolCallRecord {
    pub tool: SubagentToolKind,
    pub call_id: String,
    pub descriptor: String,
    pub succeeded: bool,
    pub duration_ms: u128,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub truncated: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SubagentToolError {
    #[error("approved tool workspace is invalid")]
    InvalidWorkspace,
    #[error("approved tool workspace is required")]
    WorkspaceRequired,
    #[error("tool is not approved")]
    ToolNotApproved,
    #[error("tool input is invalid")]
    InvalidInput,
    #[error("tool input is too large")]
    InputTooLarge,
    #[error("tool path is invalid")]
    InvalidPath,
    #[error("tool path is outside the approved workspace")]
    PathOutsideWorkspace,
    #[error("tool path is not a regular file")]
    NotARegularFile,
    #[error("tool file is too large")]
    FileTooLarge,
    #[error("tool file is not valid UTF-8")]
    InvalidUtf8,
    #[error("tool output is too large")]
    OutputTooLarge,
    #[error("tool call identifier is invalid")]
    InvalidCallId,
    #[error("tool was cancelled")]
    Cancelled,
    #[error("git status failed safely")]
    GitStatusFailed,
}

pub(super) struct ToolExecution {
    pub(super) content: String,
    pub(super) input_bytes: usize,
    pub(super) descriptor: String,
    pub(super) truncated: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadFileArgs {
    path: String,
    start_line: Option<usize>,
    max_lines: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchTextArgs {
    query: String,
    path: Option<String>,
    max_results: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GitStatusArgs {}

pub(super) async fn execute_tool(
    policy: &SubagentToolPolicy,
    tool: SubagentToolKind,
    call_id: &str,
    arguments: &str,
    cancellation: &CancellationToken,
) -> Result<ToolExecution, SubagentToolError> {
    if !is_safe_call_id(call_id) {
        return Err(SubagentToolError::InvalidCallId);
    }
    if !policy.approved_tools.contains(&tool) {
        return Err(SubagentToolError::ToolNotApproved);
    }
    let root = policy
        .workspace_root()
        .ok_or(SubagentToolError::WorkspaceRequired)?;
    if arguments.len() > policy.budget.max_tool_input_bytes {
        return Err(SubagentToolError::InputTooLarge);
    }
    if cancellation.is_cancelled() {
        return Err(SubagentToolError::Cancelled);
    }
    match tool {
        SubagentToolKind::ReadFile => read_file(policy, root, arguments),
        SubagentToolKind::SearchText => search_text(policy, root, arguments),
        SubagentToolKind::GitStatus => git_status(policy, root, arguments, cancellation).await,
    }
}

fn read_file(
    policy: &SubagentToolPolicy,
    root: &Path,
    arguments: &str,
) -> Result<ToolExecution, SubagentToolError> {
    let args: ReadFileArgs =
        serde_json::from_str(arguments).map_err(|_| SubagentToolError::InvalidInput)?;
    let path = resolve_file_path(root, &args.path)?;
    let metadata = fs::metadata(&path).map_err(|_| SubagentToolError::NotARegularFile)?;
    if !metadata.is_file() {
        return Err(SubagentToolError::NotARegularFile);
    }
    let file_bytes = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    if file_bytes > policy.budget.max_file_bytes {
        return Err(SubagentToolError::FileTooLarge);
    }
    let bytes = fs::read(&path).map_err(|_| SubagentToolError::NotARegularFile)?;
    let text = String::from_utf8(bytes).map_err(|_| SubagentToolError::InvalidUtf8)?;
    let start_line = args.start_line.unwrap_or(1).max(1);
    let max_lines = args
        .max_lines
        .unwrap_or(policy.budget.max_file_read_lines)
        .min(policy.budget.max_file_read_lines);
    let mut lines = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        if line_number < start_line {
            continue;
        }
        if lines.len() >= max_lines {
            break;
        }
        lines.push(format!("{line_number}: {line}"));
    }
    let (content, truncated) = bound_output(
        lines.join("\n"),
        policy
            .budget
            .max_file_read_bytes
            .min(policy.budget.max_tool_output_bytes),
    );
    Ok(ToolExecution {
        content,
        input_bytes: arguments.len(),
        descriptor: format!("path_bytes={}", args.path.len()),
        truncated,
    })
}

fn search_text(
    policy: &SubagentToolPolicy,
    root: &Path,
    arguments: &str,
) -> Result<ToolExecution, SubagentToolError> {
    let args: SearchTextArgs =
        serde_json::from_str(arguments).map_err(|_| SubagentToolError::InvalidInput)?;
    if args.query.is_empty() {
        return Err(SubagentToolError::InvalidInput);
    }
    let scope = match args.path.as_deref() {
        Some(path) => resolve_existing_path(root, path)?,
        None => root.to_path_buf(),
    };
    let max_results = args
        .max_results
        .unwrap_or(policy.budget.max_search_results)
        .min(policy.budget.max_search_results);
    let mut files = Vec::new();
    collect_files(&scope, root, &mut files, policy.budget.max_search_files)?;
    let mut scanned_bytes = 0usize;
    let mut results = Vec::new();
    let mut truncated = false;
    for file in files {
        if results.len() >= max_results {
            truncated = true;
            break;
        }
        let bytes = match fs::read(&file) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        scanned_bytes = scanned_bytes.saturating_add(bytes.len());
        if scanned_bytes > policy.budget.max_search_bytes {
            truncated = true;
            break;
        }
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => continue,
        };
        for (line_number, line) in text.lines().enumerate() {
            if line.contains(&args.query) {
                let relative = file
                    .strip_prefix(root)
                    .map_err(|_| SubagentToolError::PathOutsideWorkspace)?;
                results.push(format!(
                    "{}:{}: {}",
                    relative.display(),
                    line_number + 1,
                    bounded_excerpt(line, 512)
                ));
                if results.len() >= max_results {
                    truncated = true;
                    break;
                }
            }
        }
    }
    let (content, output_truncated) =
        bound_output(results.join("\n"), policy.budget.max_tool_output_bytes);
    Ok(ToolExecution {
        content,
        input_bytes: arguments.len(),
        descriptor: format!("query_bytes={}", args.query.len()),
        truncated: truncated || output_truncated,
    })
}

async fn git_status(
    policy: &SubagentToolPolicy,
    root: &Path,
    arguments: &str,
    cancellation: &CancellationToken,
) -> Result<ToolExecution, SubagentToolError> {
    let _: GitStatusArgs =
        serde_json::from_str(arguments).map_err(|_| SubagentToolError::InvalidInput)?;
    let output = tokio::select! {
        _ = cancellation.cancelled() => return Err(SubagentToolError::Cancelled),
        result = tokio::time::timeout(
            policy.budget.per_tool_timeout,
            Command::new("git")
                .current_dir(root)
                .args(["status", "--short", "--porcelain=v1", "-z", "--untracked-files=all"])
                .output(),
        ) => result.map_err(|_| SubagentToolError::GitStatusFailed)?.map_err(|_| SubagentToolError::GitStatusFailed)?,
    };
    if output.stdout.len() > policy.budget.max_git_output_bytes
        || output.stderr.len() > policy.budget.max_git_output_bytes
        || !output.status.success()
    {
        return Err(SubagentToolError::GitStatusFailed);
    }
    let status =
        String::from_utf8(output.stdout).map_err(|_| SubagentToolError::GitStatusFailed)?;
    let mut entries = Vec::new();
    for record in status.split('\0').filter(|record| !record.is_empty()) {
        if entries.len() >= policy.budget.max_git_status_entries {
            break;
        }
        let safe_record = record
            .split_once(' ')
            .map(|(status, path)| format!("{} {}", status.trim(), path.len()))
            .unwrap_or_else(|| format!("record_bytes={}", record.len()));
        entries.push(safe_record);
    }
    let content = entries.join("\n");
    Ok(ToolExecution {
        input_bytes: arguments.len(),
        descriptor: "status".to_string(),
        truncated: status
            .split('\0')
            .filter(|record| !record.is_empty())
            .count()
            > policy.budget.max_git_status_entries,
        content,
    })
}

fn resolve_existing_path(root: &Path, relative: &str) -> Result<PathBuf, SubagentToolError> {
    let candidate = validate_relative_path(root, relative)?;
    let canonical = dunce::canonicalize(candidate).map_err(|_| SubagentToolError::InvalidPath)?;
    ensure_inside(root, &canonical)?;
    Ok(canonical)
}

fn resolve_file_path(root: &Path, relative: &str) -> Result<PathBuf, SubagentToolError> {
    let path = resolve_existing_path(root, relative)?;
    if !path.is_file() {
        return Err(SubagentToolError::NotARegularFile);
    }
    Ok(path)
}

fn validate_relative_path(root: &Path, relative: &str) -> Result<PathBuf, SubagentToolError> {
    if relative.is_empty() || relative.contains('\0') {
        return Err(SubagentToolError::InvalidPath);
    }
    if Path::new(relative).is_absolute()
        || relative.starts_with("\\\\")
        || relative.starts_with("//")
        || has_drive_prefix(relative)
    {
        return Err(SubagentToolError::InvalidPath);
    }
    if relative
        .split(['/', '\\'])
        .any(|component| component == "..")
    {
        return Err(SubagentToolError::InvalidPath);
    }
    let path = root.join(relative);
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(SubagentToolError::InvalidPath);
    }
    Ok(path)
}

fn ensure_inside(root: &Path, path: &Path) -> Result<(), SubagentToolError> {
    if path.starts_with(root) {
        Ok(())
    } else {
        Err(SubagentToolError::PathOutsideWorkspace)
    }
}

fn has_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn collect_files(
    path: &Path,
    root: &Path,
    files: &mut Vec<PathBuf>,
    max_files: usize,
) -> Result<(), SubagentToolError> {
    if files.len() >= max_files {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| SubagentToolError::InvalidPath)?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        let canonical = dunce::canonicalize(path).map_err(|_| SubagentToolError::InvalidPath)?;
        ensure_inside(root, &canonical)?;
        files.push(canonical);
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    let mut entries = fs::read_dir(path)
        .map_err(|_| SubagentToolError::InvalidPath)?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if files.len() >= max_files {
            break;
        }
        let _ = collect_files(&entry.path(), root, files, max_files);
    }
    Ok(())
}

fn bound_output(value: String, limit: usize) -> (String, bool) {
    if value.len() <= limit {
        return (value, false);
    }
    let mut end = limit;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_string(), true)
}

fn bounded_excerpt(value: &str, limit: usize) -> String {
    bound_output(value.to_string(), limit).0
}

fn is_safe_call_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
