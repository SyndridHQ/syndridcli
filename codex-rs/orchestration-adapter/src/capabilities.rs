use codex_orchestration::DataQuality;
use serde::Deserialize;
use serde::Serialize;

/// Static capabilities reported by a future adapter implementation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterCapabilities {
    supports_child_spawn: bool,
    supports_handoff_delivery: bool,
    supports_cancellation: bool,
    supports_observation: bool,
    supports_read_only_children: bool,
    supports_writer_children: bool,
    supports_model_override: bool,
    supports_effort_override: bool,
    max_supported_children: Option<u16>,
    data_quality: DataQuality,
}

impl AdapterCapabilities {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        supports_child_spawn: bool,
        supports_handoff_delivery: bool,
        supports_cancellation: bool,
        supports_observation: bool,
        supports_read_only_children: bool,
        supports_writer_children: bool,
        supports_model_override: bool,
        supports_effort_override: bool,
        max_supported_children: Option<u16>,
        data_quality: DataQuality,
    ) -> Self {
        Self {
            supports_child_spawn,
            supports_handoff_delivery,
            supports_cancellation,
            supports_observation,
            supports_read_only_children,
            supports_writer_children,
            supports_model_override,
            supports_effort_override,
            max_supported_children,
            data_quality,
        }
    }

    pub fn supports_child_spawn(&self) -> bool {
        self.supports_child_spawn
    }
    pub fn supports_handoff_delivery(&self) -> bool {
        self.supports_handoff_delivery
    }
    pub fn supports_cancellation(&self) -> bool {
        self.supports_cancellation
    }
    pub fn supports_observation(&self) -> bool {
        self.supports_observation
    }
    pub fn supports_read_only_children(&self) -> bool {
        self.supports_read_only_children
    }
    pub fn supports_writer_children(&self) -> bool {
        self.supports_writer_children
    }
    pub fn supports_model_override(&self) -> bool {
        self.supports_model_override
    }
    pub fn supports_effort_override(&self) -> bool {
        self.supports_effort_override
    }
    pub fn max_supported_children(&self) -> Option<u16> {
        self.max_supported_children
    }
    pub fn data_quality(&self) -> DataQuality {
        self.data_quality
    }
}
