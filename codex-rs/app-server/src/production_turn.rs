use crate::ProductionSessionRuntime;
use std::sync::Arc;

/// Trusted runtime authorization for selecting a production turn runner.
///
/// This is intentionally separate from `PublicBrand`, provider selection, and
/// user input. Callers that do not provide a capability retain Codex
/// compatibility through `Default`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProductionExecutionCapability {
    /// Use the existing Codex thread/turn execution path.
    #[default]
    CodexCompatibility,
    /// Select the future Syndrid orchestration turn boundary.
    SyndridOrchestration,
}

/// The immutable runner selected for one admitted production turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProductionTurnPath {
    CodexCompatibility,
    SyndridOrchestration,
}

/// Selects one production turn path from trusted runtime authorization.
///
/// The router has no provider, policy, tool, cancellation, or UI state. A
/// caller captures the returned path at turn admission and must use that value
/// for the lifetime of the turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProductionTurnRouter {
    capability: ProductionExecutionCapability,
}

/// Trusted in-process authorization captured before a production turn is admitted.
///
/// The capability selects the path; the optional runtime supplies the already assembled
/// execution authority. Keeping them together prevents a turn from observing a capability and
/// runtime from different session states.
#[derive(Clone, Default)]
pub(crate) struct ProductionTurnAuthorization {
    pub(crate) capability: ProductionExecutionCapability,
    pub(crate) runtime: Option<Arc<ProductionSessionRuntime>>,
}

impl ProductionTurnAuthorization {
    pub(crate) fn new(
        capability: ProductionExecutionCapability,
        runtime: Option<Arc<ProductionSessionRuntime>>,
    ) -> Self {
        Self {
            capability,
            runtime,
        }
    }

    pub(crate) fn path(&self) -> ProductionTurnPath {
        ProductionTurnRouter::new(self.capability).select()
    }
}

impl ProductionTurnRouter {
    pub(crate) const fn new(capability: ProductionExecutionCapability) -> Self {
        Self { capability }
    }

    pub(crate) const fn select(self) -> ProductionTurnPath {
        match self.capability {
            ProductionExecutionCapability::CodexCompatibility => {
                ProductionTurnPath::CodexCompatibility
            }
            ProductionExecutionCapability::SyndridOrchestration => {
                ProductionTurnPath::SyndridOrchestration
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::production_turn::ProductionTurnAuthorization;
    use codex_utils_cli::PublicBrand;

    use super::ProductionExecutionCapability;
    use super::ProductionTurnPath;
    use super::ProductionTurnRouter;

    #[test]
    fn missing_capability_defaults_to_codex_compatibility() {
        assert_eq!(
            ProductionTurnRouter::new(ProductionExecutionCapability::default()).select(),
            ProductionTurnPath::CodexCompatibility
        );
    }

    #[test]
    fn explicit_codex_capability_selects_codex_compatibility() {
        assert_eq!(
            ProductionTurnRouter::new(ProductionExecutionCapability::CodexCompatibility).select(),
            ProductionTurnPath::CodexCompatibility
        );
    }

    #[test]
    fn public_brand_does_not_select_the_execution_path() {
        for brand in [PublicBrand::Codex, PublicBrand::Syndrid] {
            let _ = brand;
            assert_eq!(
                ProductionTurnRouter::new(ProductionExecutionCapability::default()).select(),
                ProductionTurnPath::CodexCompatibility
            );
        }
    }

    #[test]
    fn explicit_codex_capability_is_not_overridden_by_public_brand() {
        for brand in [PublicBrand::Codex, PublicBrand::Syndrid] {
            let _ = brand;
            assert_eq!(
                ProductionTurnRouter::new(ProductionExecutionCapability::CodexCompatibility)
                    .select(),
                ProductionTurnPath::CodexCompatibility
            );
        }
    }

    #[test]
    fn explicit_syndrid_selection_does_not_fall_back_to_codex() {
        assert_eq!(
            ProductionTurnRouter::new(ProductionExecutionCapability::SyndridOrchestration).select(),
            ProductionTurnPath::SyndridOrchestration
        );
    }

    #[test]
    fn selected_path_is_immutable_after_admission() {
        let selected_path =
            ProductionTurnRouter::new(ProductionExecutionCapability::SyndridOrchestration).select();
        let later_router =
            ProductionTurnRouter::new(ProductionExecutionCapability::CodexCompatibility);

        assert_eq!(selected_path, ProductionTurnPath::SyndridOrchestration);
        assert_eq!(
            later_router.select(),
            ProductionTurnPath::CodexCompatibility
        );
        assert_eq!(selected_path, ProductionTurnPath::SyndridOrchestration);
    }

    #[test]
    fn unavailable_syndrid_authorization_does_not_fall_back_to_codex() {
        let authorization = ProductionTurnAuthorization::new(
            ProductionExecutionCapability::SyndridOrchestration,
            None,
        );

        assert_eq!(
            authorization.path(),
            ProductionTurnPath::SyndridOrchestration
        );
        assert!(authorization.runtime.is_none());
    }

    #[test]
    fn default_app_server_runtime_options_keep_codex_compatibility() {
        assert_eq!(
            crate::AppServerRuntimeOptions::default().production_execution_capability,
            ProductionExecutionCapability::CodexCompatibility
        );
    }
}
