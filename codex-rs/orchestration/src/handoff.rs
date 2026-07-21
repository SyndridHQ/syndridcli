use std::fmt;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::AgentId;
use crate::AgentRole;
use crate::DataQuality;
use crate::ForecastConfidence;
use crate::TaskId;
use crate::WorkflowId;

/// Maximum size for one handoff text field before a persisted evidence reference is needed.
pub const MAX_HANDOFF_TEXT_BYTES: usize = 4096;

/// Validated bounded text for summaries and metadata references.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct BoundedText(String);

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BoundedTextError {
    #[error("text exceeds the 4096-byte handoff limit")]
    TooLong,
}

impl BoundedText {
    pub fn new(value: impl Into<String>) -> Result<Self, BoundedTextError> {
        let value = value.into();
        if value.len() > MAX_HANDOFF_TEXT_BYTES {
            return Err(BoundedTextError::TooLong);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for BoundedText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for BoundedText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Bounded, data-only handoff shape; it never stores hidden chain-of-thought or raw tool output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "StructuredHandoffWire")]
pub struct StructuredHandoff {
    workflow_id: WorkflowId,
    task_id: TaskId,
    source_agent_id: AgentId,
    destination_role: AgentRole,
    task_summary: BoundedText,
    assigned_objective: BoundedText,
    scope: BoundedText,
    findings: Vec<BoundedText>,
    files_inspected: Vec<BoundedText>,
    files_changed: Vec<BoundedText>,
    commands_run: Vec<BoundedText>,
    command_outcomes: Vec<BoundedText>,
    verification_evidence_references: Vec<BoundedText>,
    blockers: Vec<BoundedText>,
    risks: Vec<BoundedText>,
    confidence: ForecastConfidence,
    unresolved_questions: Vec<BoundedText>,
    recommended_next_action: BoundedText,
    detailed_evidence_references: Vec<BoundedText>,
    evidence_quality: DataQuality,
}

#[derive(Clone, Debug, Deserialize)]
struct StructuredHandoffWire {
    workflow_id: WorkflowId,
    task_id: TaskId,
    source_agent_id: AgentId,
    destination_role: AgentRole,
    task_summary: BoundedText,
    assigned_objective: BoundedText,
    scope: BoundedText,
    findings: Vec<BoundedText>,
    files_inspected: Vec<BoundedText>,
    files_changed: Vec<BoundedText>,
    commands_run: Vec<BoundedText>,
    command_outcomes: Vec<BoundedText>,
    verification_evidence_references: Vec<BoundedText>,
    blockers: Vec<BoundedText>,
    risks: Vec<BoundedText>,
    confidence: ForecastConfidence,
    unresolved_questions: Vec<BoundedText>,
    recommended_next_action: BoundedText,
    detailed_evidence_references: Vec<BoundedText>,
    evidence_quality: DataQuality,
}

impl From<StructuredHandoff> for StructuredHandoffWire {
    fn from(value: StructuredHandoff) -> Self {
        Self {
            workflow_id: value.workflow_id,
            task_id: value.task_id,
            source_agent_id: value.source_agent_id,
            destination_role: value.destination_role,
            task_summary: value.task_summary,
            assigned_objective: value.assigned_objective,
            scope: value.scope,
            findings: value.findings,
            files_inspected: value.files_inspected,
            files_changed: value.files_changed,
            commands_run: value.commands_run,
            command_outcomes: value.command_outcomes,
            verification_evidence_references: value.verification_evidence_references,
            blockers: value.blockers,
            risks: value.risks,
            confidence: value.confidence,
            unresolved_questions: value.unresolved_questions,
            recommended_next_action: value.recommended_next_action,
            detailed_evidence_references: value.detailed_evidence_references,
            evidence_quality: value.evidence_quality,
        }
    }
}

impl From<StructuredHandoffWire> for StructuredHandoff {
    fn from(value: StructuredHandoffWire) -> Self {
        Self::new(
            value.workflow_id,
            value.task_id,
            value.source_agent_id,
            value.destination_role,
            value.task_summary,
            value.assigned_objective,
            value.scope,
            value.findings,
            value.files_inspected,
            value.files_changed,
            value.commands_run,
            value.command_outcomes,
            value.verification_evidence_references,
            value.blockers,
            value.risks,
            value.confidence,
            value.unresolved_questions,
            value.recommended_next_action,
            value.detailed_evidence_references,
            value.evidence_quality,
        )
    }
}

impl StructuredHandoff {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workflow_id: WorkflowId,
        task_id: TaskId,
        source_agent_id: AgentId,
        destination_role: AgentRole,
        task_summary: BoundedText,
        assigned_objective: BoundedText,
        scope: BoundedText,
        findings: Vec<BoundedText>,
        files_inspected: Vec<BoundedText>,
        files_changed: Vec<BoundedText>,
        commands_run: Vec<BoundedText>,
        command_outcomes: Vec<BoundedText>,
        verification_evidence_references: Vec<BoundedText>,
        blockers: Vec<BoundedText>,
        risks: Vec<BoundedText>,
        confidence: ForecastConfidence,
        unresolved_questions: Vec<BoundedText>,
        recommended_next_action: BoundedText,
        detailed_evidence_references: Vec<BoundedText>,
        evidence_quality: DataQuality,
    ) -> Self {
        Self {
            workflow_id,
            task_id,
            source_agent_id,
            destination_role,
            task_summary,
            assigned_objective,
            scope,
            findings,
            files_inspected,
            files_changed,
            commands_run,
            command_outcomes,
            verification_evidence_references,
            blockers,
            risks,
            confidence,
            unresolved_questions,
            recommended_next_action,
            detailed_evidence_references,
            evidence_quality,
        }
    }

    pub fn workflow_id(&self) -> &WorkflowId {
        &self.workflow_id
    }
    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }
    pub fn source_agent_id(&self) -> &AgentId {
        &self.source_agent_id
    }
    pub fn destination_role(&self) -> AgentRole {
        self.destination_role
    }
    pub fn task_summary(&self) -> &BoundedText {
        &self.task_summary
    }
    pub fn evidence_quality(&self) -> DataQuality {
        self.evidence_quality
    }
}
