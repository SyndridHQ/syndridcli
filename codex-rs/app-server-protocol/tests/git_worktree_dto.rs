use codex_app_server_protocol::GitWorktreeListParams;
use codex_app_server_protocol::GitWorktreeListResponse;
use serde_json::json;

fn absolute_test_cwd() -> &'static str {
    if cfg!(windows) {
        r"C:\syndrid-test-repo"
    } else {
        "/syndrid-test-repo"
    }
}

#[test]
fn worktree_list_params_preserve_absolute_native_cwd_and_limit() {
    let params: GitWorktreeListParams = serde_json::from_value(json!({
        "cwd": absolute_test_cwd(),
        "limit": 37
    }))
    .expect("worktree list params should accept an absolute native cwd");

    let encoded = serde_json::to_value(params).expect("worktree list params should serialize");
    assert_eq!(encoded["cwd"], absolute_test_cwd());
    assert_eq!(encoded["limit"], 37);
}

#[test]
fn worktree_list_params_reject_relative_cwd() {
    serde_json::from_value::<GitWorktreeListParams>(json!({
        "cwd": "relative/repository",
        "limit": null
    }))
    .expect_err("worktree list cwd must remain absolute at the protocol boundary");
}

#[test]
fn worktree_response_round_trip_preserves_runtime_metadata() {
    let value = json!({
        "entries": [{
            "path": "/repo/feature-😀\ncontinued",
            "head": "0123456789abcdef0123456789abcdef01234567",
            "branch": "feature/worktrees",
            "detached": false,
            "bare": false,
            "locked": true,
            "lockReason": "desktop session owns this worktree",
            "prunable": true,
            "pruneReason": "gitdir file points to non-existent location",
            "current": true
        }],
        "truncated": true
    });

    let response: GitWorktreeListResponse = serde_json::from_value(value.clone())
        .expect("worktree response should deserialize without losing runtime metadata");
    let encoded =
        serde_json::to_value(response).expect("worktree response should serialize after round trip");

    assert_eq!(encoded, value);
}
