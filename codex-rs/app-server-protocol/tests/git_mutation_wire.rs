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
fn git_stage_and_unstage_are_registered_wire_methods() {
    for (method, is_expected_variant) in [
        ("git/stage", true),
        ("git/unstage", false),
    ] {
        let request: ClientRequest = serde_json::from_value(json!({
            "method": method,
            "id": 1,
            "params": {
                "cwd": absolute_test_cwd(),
                "paths": ["src/example.rs"]
            }
        }))
        .expect("git mutation method should deserialize through the central request registry");

        assert_eq!(request.method(), method);
        assert_eq!(request.serialization_scope(), None);

        if is_expected_variant {
            assert!(matches!(request, ClientRequest::GitStage { .. }));
        } else {
            assert!(matches!(request, ClientRequest::GitUnstage { .. }));
        }
    }
}
