use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::WorkAccess;

/// Failure when an assignment would exceed a workflow or parent access ceiling.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PermissionEnvelopeError {
    #[error("parent access exceeds the workflow ceiling")]
    ParentExceedsWorkflow,
    #[error("assignment access exceeds the workflow ceiling")]
    AssignmentExceedsWorkflow,
    #[error("assignment access exceeds the parent ceiling")]
    AssignmentExceedsParent,
}

/// Orchestration-level access ceilings; Codex remains the permission truth.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "PermissionEnvelopeWire")]
pub struct PermissionEnvelope {
    workflow_ceiling: WorkAccess,
    parent_ceiling: WorkAccess,
    assignment_access: WorkAccess,
}

#[derive(Deserialize)]
struct PermissionEnvelopeWire {
    workflow_ceiling: WorkAccess,
    parent_ceiling: WorkAccess,
    assignment_access: WorkAccess,
}

impl TryFrom<PermissionEnvelopeWire> for PermissionEnvelope {
    type Error = PermissionEnvelopeError;

    fn try_from(value: PermissionEnvelopeWire) -> Result<Self, Self::Error> {
        Self::new(
            value.workflow_ceiling,
            value.parent_ceiling,
            value.assignment_access,
        )
    }
}

impl PermissionEnvelope {
    pub const fn new(
        workflow_ceiling: WorkAccess,
        parent_ceiling: WorkAccess,
        assignment_access: WorkAccess,
    ) -> Result<Self, PermissionEnvelopeError> {
        if !workflow_ceiling.allows(parent_ceiling) {
            return Err(PermissionEnvelopeError::ParentExceedsWorkflow);
        }
        if !workflow_ceiling.allows(assignment_access) {
            return Err(PermissionEnvelopeError::AssignmentExceedsWorkflow);
        }
        if !parent_ceiling.allows(assignment_access) {
            return Err(PermissionEnvelopeError::AssignmentExceedsParent);
        }
        Ok(Self {
            workflow_ceiling,
            parent_ceiling,
            assignment_access,
        })
    }

    pub const fn workflow_ceiling(self) -> WorkAccess {
        self.workflow_ceiling
    }

    pub const fn parent_ceiling(self) -> WorkAccess {
        self.parent_ceiling
    }

    pub const fn assignment_access(self) -> WorkAccess {
        self.assignment_access
    }
}
