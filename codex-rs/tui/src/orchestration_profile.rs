//! Session-owned persistence for the local Syndrid orchestration default.

use crate::legacy_core::ExecutionModeSelection;
use crate::legacy_core::ExecutionPolicy;
use crate::legacy_core::OrchestrationMode;
use crate::legacy_core::OrchestrationStrategyAvailability;
use crate::legacy_core::ResolvedOrchestrationPolicy;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::io::ErrorKind;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tempfile::NamedTempFile;
use thiserror::Error;

pub(crate) const PROFILE_FILE_NAME: &str = "syndrid-orchestration-profile.json";
const PROFILE_SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_PROFILE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OrchestrationProfileSelection {
    pub(crate) strategy: OrchestrationMode,
    pub(crate) preset: ExecutionModeSelection,
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub(crate) enum OrchestrationProfileWarning {
    #[error("saved orchestration default is malformed")]
    Malformed,
    #[error("saved orchestration default is missing required fields")]
    MissingField,
    #[error("saved orchestration default uses an unsupported schema version")]
    UnsupportedSchema,
    #[error("saved orchestration default is too large")]
    Oversized,
    #[error("saved orchestration default has an unknown strategy")]
    UnknownStrategy,
    #[error("saved orchestration default has an unknown preset")]
    UnknownPreset,
    #[error("saved orchestration default has invalid custom policy data")]
    InvalidCustom,
    #[error("saved orchestration default selects an unavailable strategy")]
    StrategyUnavailable,
    #[error("saved orchestration default could not be read")]
    Io,
}

impl OrchestrationProfileWarning {
    pub(crate) fn user_message(&self) -> String {
        format!(
            "Saved orchestration default was not applied: {self}. The session is using Single / Balanced."
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub(crate) enum OrchestrationProfileSaveError {
    #[error("the selected orchestration policy is unavailable")]
    StrategyUnavailable,
    #[error("the selected orchestration policy is invalid")]
    InvalidPolicy,
    #[error("the local orchestration default could not be serialized")]
    Serialization,
    #[error("the local orchestration default could not be written")]
    Io,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedOrchestrationProfile {
    schema_version: Option<u32>,
    strategy: Option<String>,
    preset: Option<String>,
    #[serde(default)]
    custom_policy: Option<ExecutionPolicy>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SavedDefaultState {
    Missing,
    Loaded(OrchestrationProfileSelection),
    Invalid(OrchestrationProfileWarning),
}

struct OrchestrationProfileStoreInner {
    saved_default: SavedDefaultState,
}

/// Trusted local storage for the saved orchestration default.
pub(crate) struct OrchestrationProfileStore {
    path: PathBuf,
    inner: Mutex<OrchestrationProfileStoreInner>,
}

pub(crate) struct OrchestrationProfileLoad {
    pub(crate) store: Arc<OrchestrationProfileStore>,
    pub(crate) selection: Option<OrchestrationProfileSelection>,
    pub(crate) warning: Option<OrchestrationProfileWarning>,
}

impl OrchestrationProfileStore {
    pub(crate) fn load(codex_home: &Path) -> OrchestrationProfileLoad {
        Self::load_from_path(codex_home.join(PROFILE_FILE_NAME))
    }

    fn load_from_path(path: PathBuf) -> OrchestrationProfileLoad {
        let (saved_default, selection, warning) = match load_profile(&path) {
            Ok(None) => (SavedDefaultState::Missing, None, None),
            Ok(Some(selection)) => {
                let selection_for_state = selection.clone();
                (
                    SavedDefaultState::Loaded(selection),
                    Some(selection_for_state),
                    None,
                )
            }
            Err(warning) => {
                let warning_for_state = warning.clone();
                (
                    SavedDefaultState::Invalid(warning),
                    None,
                    Some(warning_for_state),
                )
            }
        };
        OrchestrationProfileLoad {
            store: Arc::new(Self {
                path,
                inner: Mutex::new(OrchestrationProfileStoreInner { saved_default }),
            }),
            selection,
            warning,
        }
    }

    pub(crate) fn saved_default_label(&self) -> String {
        let Ok(inner) = self.inner.lock() else {
            return "Unavailable".to_string();
        };
        match &inner.saved_default {
            SavedDefaultState::Missing => "None (Single / Balanced)".to_string(),
            SavedDefaultState::Loaded(selection) => selection_label(selection),
            SavedDefaultState::Invalid(warning) => format!("Unavailable ({warning})"),
        }
    }

    pub(crate) fn save(
        &self,
        selection: OrchestrationProfileSelection,
    ) -> Result<(), OrchestrationProfileSaveError> {
        let resolved =
            ResolvedOrchestrationPolicy::resolve(selection.strategy, selection.preset.clone())
                .map_err(|_| OrchestrationProfileSaveError::InvalidPolicy)?;
        if !matches!(
            resolved.availability(),
            OrchestrationStrategyAvailability::Available
        ) {
            return Err(OrchestrationProfileSaveError::StrategyUnavailable);
        }
        let persisted = PersistedOrchestrationProfile {
            schema_version: Some(PROFILE_SCHEMA_VERSION),
            strategy: Some(strategy_value(selection.strategy).to_string()),
            preset: Some(preset_value(&selection.preset).to_string()),
            custom_policy: match selection.preset {
                ExecutionModeSelection::Custom(ref policy) => Some(policy.clone()),
                _ => None,
            },
        };
        let mut bytes = serde_json::to_vec_pretty(&persisted)
            .map_err(|_| OrchestrationProfileSaveError::Serialization)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_PROFILE_BYTES {
            return Err(OrchestrationProfileSaveError::Serialization);
        }
        let parent = self
            .path
            .parent()
            .ok_or(OrchestrationProfileSaveError::Io)?;
        fs::create_dir_all(parent).map_err(|_| OrchestrationProfileSaveError::Io)?;
        let mut temporary =
            NamedTempFile::new_in(parent).map_err(|_| OrchestrationProfileSaveError::Io)?;
        temporary
            .write_all(&bytes)
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|_| OrchestrationProfileSaveError::Io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            temporary
                .as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| OrchestrationProfileSaveError::Io)?;
        }
        temporary
            .persist(&self.path)
            .map_err(|_| OrchestrationProfileSaveError::Io)?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| OrchestrationProfileSaveError::Io)?;
        inner.saved_default = SavedDefaultState::Loaded(selection);
        Ok(())
    }
}

fn load_profile(
    path: &Path,
) -> Result<Option<OrchestrationProfileSelection>, OrchestrationProfileWarning> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(OrchestrationProfileWarning::Io),
    };
    if metadata.len() > MAX_PROFILE_BYTES as u64 {
        return Err(OrchestrationProfileWarning::Oversized);
    }
    let bytes = fs::read(path).map_err(|_| OrchestrationProfileWarning::Io)?;
    let persisted: PersistedOrchestrationProfile =
        serde_json::from_slice(&bytes).map_err(|_| OrchestrationProfileWarning::Malformed)?;
    if persisted.schema_version != Some(PROFILE_SCHEMA_VERSION) {
        return Err(OrchestrationProfileWarning::UnsupportedSchema);
    }
    let strategy = persisted
        .strategy
        .ok_or(OrchestrationProfileWarning::MissingField)
        .and_then(|value| parse_strategy(&value))?;
    let preset_value = persisted
        .preset
        .ok_or(OrchestrationProfileWarning::MissingField)?;
    let preset = parse_preset(&preset_value, persisted.custom_policy)?;
    let resolved = ResolvedOrchestrationPolicy::resolve(strategy, preset.clone())
        .map_err(|_| OrchestrationProfileWarning::InvalidCustom)?;
    if !matches!(
        resolved.availability(),
        OrchestrationStrategyAvailability::Available
    ) {
        return Err(OrchestrationProfileWarning::StrategyUnavailable);
    }
    Ok(Some(OrchestrationProfileSelection { strategy, preset }))
}

fn parse_strategy(value: &str) -> Result<OrchestrationMode, OrchestrationProfileWarning> {
    match value {
        "single" => Ok(OrchestrationMode::Single),
        "manual" => Ok(OrchestrationMode::Manual),
        "recommended" => Ok(OrchestrationMode::Recommended),
        "automatic" => Ok(OrchestrationMode::Automatic),
        "adaptive" => Ok(OrchestrationMode::Adaptive),
        _ => Err(OrchestrationProfileWarning::UnknownStrategy),
    }
}

fn parse_preset(
    value: &str,
    custom_policy: Option<ExecutionPolicy>,
) -> Result<ExecutionModeSelection, OrchestrationProfileWarning> {
    match value {
        "fast" => Ok(ExecutionModeSelection::Fast),
        "balanced" => Ok(ExecutionModeSelection::Balanced),
        "usage_saver" => Ok(ExecutionModeSelection::UsageSaver),
        "deep" => Ok(ExecutionModeSelection::Deep),
        "custom" => custom_policy
            .map(ExecutionModeSelection::custom)
            .ok_or(OrchestrationProfileWarning::InvalidCustom),
        _ => Err(OrchestrationProfileWarning::UnknownPreset),
    }
}

fn strategy_value(strategy: OrchestrationMode) -> &'static str {
    match strategy {
        OrchestrationMode::Single => "single",
        OrchestrationMode::Manual => "manual",
        OrchestrationMode::Recommended => "recommended",
        OrchestrationMode::Automatic => "automatic",
        OrchestrationMode::Adaptive => "adaptive",
    }
}

fn preset_value(preset: &ExecutionModeSelection) -> &'static str {
    match preset {
        ExecutionModeSelection::Fast => "fast",
        ExecutionModeSelection::Balanced => "balanced",
        ExecutionModeSelection::UsageSaver => "usage_saver",
        ExecutionModeSelection::Deep => "deep",
        ExecutionModeSelection::Custom(_) => "custom",
    }
}

pub(crate) fn selection_label(selection: &OrchestrationProfileSelection) -> String {
    format!(
        "{} / {}",
        strategy_label(selection.strategy),
        preset_label(&selection.preset)
    )
}

fn strategy_label(strategy: OrchestrationMode) -> &'static str {
    match strategy {
        OrchestrationMode::Single => "Single",
        OrchestrationMode::Manual => "Manual",
        OrchestrationMode::Recommended => "Recommended",
        OrchestrationMode::Automatic => "Automatic",
        OrchestrationMode::Adaptive => "Adaptive",
    }
}

fn preset_label(preset: &ExecutionModeSelection) -> &'static str {
    match preset {
        ExecutionModeSelection::Fast => "Fast",
        ExecutionModeSelection::Balanced => "Balanced",
        ExecutionModeSelection::UsageSaver => "Usage Saver",
        ExecutionModeSelection::Deep => "Deep",
        ExecutionModeSelection::Custom(_) => "Custom",
    }
}

#[cfg(test)]
#[path = "orchestration_profile_tests.rs"]
mod tests;
