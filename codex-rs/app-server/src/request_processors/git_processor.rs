use super::*;

#[derive(Clone)]
pub(crate) struct GitRequestProcessor;

impl GitRequestProcessor {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn git_diff_to_remote(
        &self,
        params: GitDiffToRemoteParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.git_diff_to_origin(params.cwd)
            .await
            .map(|response| Some(response.into()))
    }

    async fn git_diff_to_origin(
        &self,
        cwd: PathBuf,
    ) -> Result<GitDiffToRemoteResponse, JSONRPCErrorError> {
        git_diff_to_remote(&cwd)
            .await
            .map(|value| {
                let changes = parse_git_diff_changes(&value.diff);
                GitDiffToRemoteResponse {
                    sha: value.sha,
                    diff: value.diff,
                    changes,
                }
            })
            .ok_or_else(|| {
                invalid_request(format!(
                    "failed to compute git diff to remote for cwd: {cwd:?}"
                ))
            })
    }
}

fn parse_git_diff_changes(diff: &str) -> Vec<codex_app_server_protocol::GitDiffChange> {
    let lines = diff.lines().collect::<Vec<_>>();
    let mut changes = Vec::new();
    let mut start = None;

    for (index, line) in lines.iter().enumerate() {
        if !line.starts_with("diff --git ") {
            continue;
        }
        if let Some(section_start) = start.replace(index) {
            if let Some(change) = parse_git_diff_section(&lines[section_start..index]) {
                changes.push(change);
            }
        }
    }

    if let Some(section_start) = start
        && let Some(change) = parse_git_diff_section(&lines[section_start..])
    {
        changes.push(change);
    }

    changes
}

fn parse_git_diff_section(
    lines: &[&str],
) -> Option<codex_app_server_protocol::GitDiffChange> {
    use codex_app_server_protocol::GitDiffChange;
    use codex_app_server_protocol::GitDiffChangeKind;

    let rename_from = lines
        .iter()
        .find_map(|line| line.strip_prefix("rename from ").map(clean_git_path));
    let rename_to = lines
        .iter()
        .find_map(|line| line.strip_prefix("rename to ").map(clean_git_path));
    let added_path = lines
        .iter()
        .find_map(|line| marker_path(line, "+++ "));
    let removed_path = lines
        .iter()
        .find_map(|line| marker_path(line, "--- "));
    let header_path = lines.first().and_then(|line| diff_header_destination_path(line));

    let kind = if rename_to.is_some() || rename_from.is_some() {
        GitDiffChangeKind::Renamed
    } else if lines.iter().any(|line| line.starts_with("new file mode ")) {
        GitDiffChangeKind::Added
    } else if lines.iter().any(|line| line.starts_with("deleted file mode ")) {
        GitDiffChangeKind::Deleted
    } else {
        GitDiffChangeKind::Modified
    };

    let path = match kind {
        GitDiffChangeKind::Renamed => rename_to
            .clone()
            .or(added_path.clone())
            .or(header_path.clone()),
        GitDiffChangeKind::Added => added_path.clone().or(header_path.clone()),
        GitDiffChangeKind::Deleted => removed_path.clone().or(header_path.clone()),
        GitDiffChangeKind::Modified => added_path
            .clone()
            .or(removed_path.clone())
            .or(header_path.clone()),
    }?;

    let (added_lines, removed_lines) = count_hunk_lines(lines);

    Some(GitDiffChange {
        path,
        previous_path: (kind == GitDiffChangeKind::Renamed).then_some(rename_from).flatten(),
        kind,
        added_lines,
        removed_lines,
    })
}

fn count_hunk_lines(lines: &[&str]) -> (u32, u32) {
    let mut in_hunk = false;
    let mut added_lines = 0_u32;
    let mut removed_lines = 0_u32;

    for line in lines {
        if line.starts_with("@@") {
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            continue;
        }
        if line.starts_with('+') {
            added_lines = added_lines.saturating_add(1);
        } else if line.starts_with('-') {
            removed_lines = removed_lines.saturating_add(1);
        }
    }

    (added_lines, removed_lines)
}

fn marker_path(line: &str, marker: &str) -> Option<String> {
    let path = line.strip_prefix(marker)?.trim();
    (path != "/dev/null").then(|| clean_git_path(path))
}

fn diff_header_destination_path(line: &str) -> Option<String> {
    let header = line.strip_prefix("diff --git ")?;
    let (_, destination) = header.rsplit_once(" b/")?;
    Some(clean_git_path(destination))
}

fn clean_git_path(path: &str) -> String {
    let path = path.trim();
    let path = path
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(path);
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_app_server_protocol::GitDiffChangeKind;

    #[test]
    fn parses_modified_added_deleted_and_renamed_files() {
        let diff = r#"diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,2 +1,3 @@
-old
+new
+extra
 keep
diff --git a/new file.txt b/new file.txt
new file mode 100644
--- /dev/null
+++ b/new file.txt
@@ -0,0 +1 @@
+created
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-removed
diff --git a/old.txt b/new.txt
similarity index 90%
rename from old.txt
rename to new.txt
--- a/old.txt
+++ b/new.txt
@@ -1 +1 @@
-before
+after
"#;

        let changes = parse_git_diff_changes(diff);
        assert_eq!(changes.len(), 4);

        assert_eq!(changes[0].path, "src/lib.rs");
        assert_eq!(changes[0].kind, GitDiffChangeKind::Modified);
        assert_eq!((changes[0].added_lines, changes[0].removed_lines), (2, 1));

        assert_eq!(changes[1].path, "new file.txt");
        assert_eq!(changes[1].kind, GitDiffChangeKind::Added);
        assert_eq!((changes[1].added_lines, changes[1].removed_lines), (1, 0));

        assert_eq!(changes[2].path, "gone.txt");
        assert_eq!(changes[2].kind, GitDiffChangeKind::Deleted);
        assert_eq!((changes[2].added_lines, changes[2].removed_lines), (0, 1));

        assert_eq!(changes[3].path, "new.txt");
        assert_eq!(changes[3].previous_path.as_deref(), Some("old.txt"));
        assert_eq!(changes[3].kind, GitDiffChangeKind::Renamed);
        assert_eq!((changes[3].added_lines, changes[3].removed_lines), (1, 1));
    }

    #[test]
    fn counts_header_like_content_inside_hunks() {
        let diff = r#"diff --git a/markers.txt b/markers.txt
index 1111111..2222222 100644
--- a/markers.txt
+++ b/markers.txt
@@ -1,2 +1,2 @@
---removed content
+++added content
 keep
"#;

        let changes = parse_git_diff_changes(diff);
        assert_eq!(changes.len(), 1);
        assert_eq!((changes[0].added_lines, changes[0].removed_lines), (1, 1));
    }

    #[test]
    fn returns_no_changes_for_empty_diff() {
        assert!(parse_git_diff_changes("").is_empty());
    }
}
