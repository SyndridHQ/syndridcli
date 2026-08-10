use super::MAX_PROFILE_BYTES;
use super::OrchestrationProfileSaveError;
use super::OrchestrationProfileSelection;
use super::OrchestrationProfileStore;
use super::OrchestrationProfileWarning;
use super::PROFILE_FILE_NAME;
use crate::legacy_core::ExecutionModeSelection;
use crate::legacy_core::OrchestrationMode;
use crate::legacy_core::SessionExecutionPolicyState;
use crate::legacy_core::SessionPolicySource;
use std::fs;
use tempfile::tempdir;

fn profile_path() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempdir().expect("temporary profile directory");
    let path = directory.path().join(PROFILE_FILE_NAME);
    (directory, path)
}

fn write_profile(path: &std::path::Path, value: &str) {
    fs::write(path, value).expect("write profile fixture");
}

fn selection(
    strategy: OrchestrationMode,
    preset: ExecutionModeSelection,
) -> OrchestrationProfileSelection {
    OrchestrationProfileSelection { strategy, preset }
}

#[test]
fn missing_profile_uses_no_saved_default() {
    let (_directory, path) = profile_path();
    let loaded = OrchestrationProfileStore::load_from_path(path);
    assert_eq!(loaded.selection, None);
    assert_eq!(loaded.warning, None);
    assert_eq!(
        loaded.store.saved_default_label(),
        "None (Single / Balanced)"
    );
}

#[test]
fn valid_single_balanced_profile_loads() {
    let (_directory, path) = profile_path();
    write_profile(
        &path,
        r#"{"schema_version":1,"strategy":"single","preset":"balanced"}"#,
    );
    let loaded = OrchestrationProfileStore::load_from_path(path);
    assert_eq!(
        loaded.selection,
        Some(selection(
            OrchestrationMode::Single,
            ExecutionModeSelection::Balanced,
        ))
    );
    assert_eq!(loaded.warning, None);
}

#[test]
fn valid_manual_fast_and_deep_profiles_load() {
    for preset in ["fast", "deep"] {
        let (_directory, path) = profile_path();
        write_profile(
            &path,
            &format!(r#"{{"schema_version":1,"strategy":"manual","preset":"{preset}"}}"#),
        );
        let loaded = OrchestrationProfileStore::load_from_path(path);
        assert_eq!(loaded.warning, None);
        assert_eq!(
            loaded.selection.as_ref().map(|value| value.strategy),
            Some(OrchestrationMode::Manual)
        );
        assert_eq!(
            loaded.selection.as_ref().map(|value| value.preset.clone()),
            Some(if preset == "fast" {
                ExecutionModeSelection::Fast
            } else {
                ExecutionModeSelection::Deep
            })
        );
    }
}

#[test]
fn loaded_selection_seeds_canonical_startup_state() {
    let cases = [
        (OrchestrationMode::Manual, ExecutionModeSelection::Fast),
        (OrchestrationMode::Single, ExecutionModeSelection::Deep),
    ];
    for (strategy, preset) in cases {
        let (_directory, path) = profile_path();
        let strategy_value = match strategy {
            OrchestrationMode::Single => "single",
            OrchestrationMode::Manual => "manual",
            OrchestrationMode::Recommended => "recommended",
            OrchestrationMode::Automatic => "automatic",
            OrchestrationMode::Adaptive => "adaptive",
        };
        let preset_value = match preset {
            ExecutionModeSelection::Fast => "fast",
            ExecutionModeSelection::Balanced => "balanced",
            ExecutionModeSelection::UsageSaver => "usage_saver",
            ExecutionModeSelection::Deep => "deep",
            ExecutionModeSelection::Custom(_) => "custom",
        };
        write_profile(
            &path,
            &format!(
                r#"{{"schema_version":1,"strategy":"{strategy_value}","preset":"{preset_value}"}}"#
            ),
        );
        let loaded = OrchestrationProfileStore::load_from_path(path)
            .selection
            .expect("selection");
        let state = SessionExecutionPolicyState::with_strategy_selection(
            loaded.strategy,
            loaded.preset.clone(),
            SessionPolicySource::Default,
        )
        .expect("canonical startup policy");
        assert_eq!(state.strategy().expect("strategy"), strategy);
        assert_eq!(state.selected_mode().expect("preset"), preset);
        assert_eq!(
            state
                .resolved_policy()
                .expect("resolved policy")
                .selected_mode(),
            &preset
        );
    }
}

#[test]
fn adaptive_profile_loads_without_schema_migration() {
    let (_directory, path) = profile_path();
    write_profile(
        &path,
        r#"{"schema_version":1,"strategy":"adaptive","preset":"balanced"}"#,
    );
    let loaded = OrchestrationProfileStore::load_from_path(path.clone());
    assert_eq!(
        loaded.selection,
        Some(selection(
            OrchestrationMode::Adaptive,
            ExecutionModeSelection::Balanced,
        ))
    );
    assert_eq!(loaded.warning, None);
    assert!(path.exists());
}

#[test]
fn malformed_unknown_and_unsupported_profiles_are_typed() {
    let cases = [
        ("{", OrchestrationProfileWarning::Malformed),
        (
            r#"{"schema_version":2,"strategy":"single","preset":"balanced"}"#,
            OrchestrationProfileWarning::UnsupportedSchema,
        ),
        (
            r#"{"schema_version":1,"strategy":"future","preset":"balanced"}"#,
            OrchestrationProfileWarning::UnknownStrategy,
        ),
        (
            r#"{"schema_version":1,"strategy":"single","preset":"future"}"#,
            OrchestrationProfileWarning::UnknownPreset,
        ),
        (
            r#"{"schema_version":1,"strategy":"single","preset":"custom"}"#,
            OrchestrationProfileWarning::InvalidCustom,
        ),
    ];
    for (contents, warning) in cases {
        let (_directory, path) = profile_path();
        write_profile(&path, contents);
        let loaded = OrchestrationProfileStore::load_from_path(path);
        assert_eq!(loaded.selection, None);
        assert_eq!(loaded.warning, Some(warning));
    }
}

#[test]
fn oversized_profile_is_rejected_without_deleting_it() {
    let (_directory, path) = profile_path();
    write_profile(&path, &"x".repeat(MAX_PROFILE_BYTES + 1));
    let loaded = OrchestrationProfileStore::load_from_path(path.clone());
    assert_eq!(loaded.warning, Some(OrchestrationProfileWarning::Oversized));
    assert!(path.exists());
}

#[test]
fn save_replaces_profile_atomically_and_does_not_change_session_selection() {
    let (_directory, path) = profile_path();
    let store = OrchestrationProfileStore::load_from_path(path.clone()).store;
    let session = selection(OrchestrationMode::Manual, ExecutionModeSelection::Fast);
    store.save(session.clone()).expect("save profile");
    assert_eq!(store.saved_default_label(), "Manual / Fast");
    assert!(
        fs::read_to_string(&path)
            .expect("read saved profile")
            .contains("manual")
    );
    store
        .save(selection(
            OrchestrationMode::Single,
            ExecutionModeSelection::Deep,
        ))
        .expect("replace profile");
    assert_eq!(store.saved_default_label(), "Single / Deep");
    assert!(
        fs::read_to_string(path)
            .expect("read replaced profile")
            .contains("deep")
    );
    assert_eq!(session.preset, ExecutionModeSelection::Fast);
}

#[test]
fn session_selection_change_does_not_auto_persist() {
    let (_directory, path) = profile_path();
    let store = OrchestrationProfileStore::load_from_path(path.clone()).store;
    store
        .save(selection(
            OrchestrationMode::Single,
            ExecutionModeSelection::Balanced,
        ))
        .expect("initial save");
    let before = fs::read(&path).expect("read initial profile");
    let state = SessionExecutionPolicyState::new().expect("session state");
    state
        .select_mode(
            ExecutionModeSelection::Fast,
            SessionPolicySource::ExplicitUserSelection,
        )
        .expect("session-only update");
    assert_eq!(
        state.selected_mode().expect("selected mode"),
        ExecutionModeSelection::Fast
    );
    assert_eq!(fs::read(path).expect("read unchanged profile"), before);
    assert_eq!(store.saved_default_label(), "Single / Balanced");
}

#[test]
fn saved_default_display_is_session_scoped() {
    let (_directory, path) = profile_path();
    let store = OrchestrationProfileStore::load_from_path(path).store;
    store
        .save(selection(
            OrchestrationMode::Single,
            ExecutionModeSelection::Balanced,
        ))
        .expect("save default");
    insta::assert_snapshot!(format!(
        "Current session: Manual / Fast\nSaved default: {}\nChanges are session-local until explicitly saved.",
        store.saved_default_label()
    ));
}

#[test]
fn save_adaptive_profile_preserves_existing_schema() {
    let (_directory, path) = profile_path();
    let store = OrchestrationProfileStore::load_from_path(path.clone()).store;
    store
        .save(selection(
            OrchestrationMode::Single,
            ExecutionModeSelection::Balanced,
        ))
        .expect("initial save");
    let before = fs::read(&path).expect("read initial profile");
    store
        .save(selection(
            OrchestrationMode::Adaptive,
            ExecutionModeSelection::Balanced,
        ))
        .expect("adaptive strategy should save");
    assert_ne!(fs::read(path).expect("read adaptive profile"), before);
    assert_eq!(store.saved_default_label(), "Adaptive / Balanced");
}

#[test]
fn failed_save_preserves_existing_profile() {
    let directory = tempdir().expect("temporary profile directory");
    let path = directory.path().join(PROFILE_FILE_NAME);
    let store = OrchestrationProfileStore::load_from_path(path.clone()).store;
    store
        .save(selection(
            OrchestrationMode::Single,
            ExecutionModeSelection::Balanced,
        ))
        .expect("initial save");
    fs::remove_file(&path).expect("remove initial profile");
    fs::create_dir(&path).expect("profile path directory");
    let error = store
        .save(selection(
            OrchestrationMode::Single,
            ExecutionModeSelection::Deep,
        ))
        .expect_err("directory destination must fail");
    assert_eq!(error, OrchestrationProfileSaveError::Io);
    assert_eq!(store.saved_default_label(), "Single / Balanced");
}
