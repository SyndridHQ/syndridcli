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
    for (method, is_expected_variant) in [("git/stage", true), ("git/unstage", false)] {
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

#[test]
fn git_stage_and_unstage_accept_literal_unicode_and_newline_paths() {
    for method in ["git/stage", "git/unstage"] {
        let request: ClientRequest = serde_json::from_value(json!({
            "method": method,
            "id": 1,
            "params": {
                "cwd": absolute_test_cwd(),
                "paths": ["src/literal-[name]-😀\ncontinued.rs"]
            }
        }))
        .expect("git mutation wire params should preserve valid literal path strings");

        assert_eq!(request.method(), method);
    }
}

#[test]
fn git_stage_and_unstage_require_explicit_paths() {
    for method in ["git/stage", "git/unstage"] {
        let error = serde_json::from_value::<ClientRequest>(json!({
            "method": method,
            "id": 1,
            "params": {
                "cwd": absolute_test_cwd()
            }
        }))
        .expect_err("git mutation wire params must require the paths field");

        assert!(error.to_string().contains("paths"));
    }
}

#[test]
fn git_stage_and_unstage_require_absolute_cwd() {
    for method in ["git/stage", "git/unstage"] {
        serde_json::from_value::<ClientRequest>(json!({
            "method": method,
            "id": 1,
            "params": {
                "cwd": "relative/repository",
                "paths": ["src/example.rs"]
            }
        }))
        .expect_err("git mutation cwd must remain absolute at the protocol boundary");
    }
}
