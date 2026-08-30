use codex_app_server_protocol::ClientRequest;
use serde_json::json;

fn absolute_test_cwd() -> &'static str {
    if cfg!(windows) {
        r"C:\syndrid-test-repo"
    } else {
        "/syndrid-test-repo"
    }
}

#[test]
fn worktree_list_is_registered_on_the_public_client_request_wire() {
    let value = json!({
        "method": "git/worktree/list",
        "id": 42,
        "params": {
            "cwd": absolute_test_cwd(),
            "limit": 37
        }
    });

    let request: ClientRequest = serde_json::from_value(value.clone())
        .expect("git/worktree/list should deserialize through ClientRequest");

    assert_eq!(request.method(), "git/worktree/list");
    assert_eq!(request.serialization_scope(), None);
    assert_eq!(
        serde_json::to_value(request).expect("worktree request should serialize"),
        value
    );
}

#[test]
fn worktree_list_wire_rejects_relative_cwd() {
    serde_json::from_value::<ClientRequest>(json!({
        "method": "git/worktree/list",
        "id": 43,
        "params": {
            "cwd": "relative/repository",
            "limit": null
        }
    }))
    .expect_err("git/worktree/list must require an absolute native cwd");
}
