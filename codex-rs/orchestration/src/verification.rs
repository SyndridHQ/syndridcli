use serde::Deserialize;
use serde::Serialize;

use crate::BoundedText;
use crate::DataQuality;

/// Observed evidence category, without embedding its raw output.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Test,
    Build,
    Lint,
    Formatter,
    Snapshot,
    CommandResult,
    Diff,
    RepositoryStatus,
    FileExistence,
    Manual,
}

/// Observed result recorded for one evidence reference.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceResult {
    Passed,
    Failed,
    Blocked,
    Inconclusive,
}

/// Requirement describing evidence a future verifier must collect.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "VerificationRequirementWire")]
pub struct VerificationRequirement {
    kind: EvidenceKind,
    mandatory: bool,
    description: BoundedText,
}

#[derive(Clone, Debug, Deserialize)]
struct VerificationRequirementWire {
    kind: EvidenceKind,
    mandatory: bool,
    description: BoundedText,
}

impl From<VerificationRequirementWire> for VerificationRequirement {
    fn from(value: VerificationRequirementWire) -> Self {
        Self::new(value.kind, value.mandatory, value.description)
    }
}

impl VerificationRequirement {
    pub fn new(kind: EvidenceKind, mandatory: bool, description: BoundedText) -> Self {
        Self {
            kind,
            mandatory,
            description,
        }
    }

    pub fn kind(&self) -> EvidenceKind {
        self.kind
    }
    pub fn mandatory(&self) -> bool {
        self.mandatory
    }
    pub fn description(&self) -> &BoundedText {
        &self.description
    }
}

/// Bounded metadata pointing to observed evidence rather than copying output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "VerificationEvidenceWire")]
pub struct VerificationEvidence {
    kind: EvidenceKind,
    source_reference: BoundedText,
    observed_result: EvidenceResult,
    data_quality: DataQuality,
}

#[derive(Clone, Debug, Deserialize)]
struct VerificationEvidenceWire {
    kind: EvidenceKind,
    source_reference: BoundedText,
    observed_result: EvidenceResult,
    data_quality: DataQuality,
}

impl From<VerificationEvidenceWire> for VerificationEvidence {
    fn from(value: VerificationEvidenceWire) -> Self {
        Self::new(
            value.kind,
            value.source_reference,
            value.observed_result,
            value.data_quality,
        )
    }
}

impl VerificationEvidence {
    pub fn new(
        kind: EvidenceKind,
        source_reference: BoundedText,
        observed_result: EvidenceResult,
        data_quality: DataQuality,
    ) -> Self {
        Self {
            kind,
            source_reference,
            observed_result,
            data_quality,
        }
    }

    pub fn kind(&self) -> EvidenceKind {
        self.kind
    }
    pub fn source_reference(&self) -> &BoundedText {
        &self.source_reference
    }
    pub fn observed_result(&self) -> EvidenceResult {
        self.observed_result
    }
    pub fn data_quality(&self) -> DataQuality {
        self.data_quality
    }
}
