use crate::scene_plan::handoff::CutId;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    Parse,
    Validation,
    NotReady,
    Sequence,
    Coverage,
}

impl IssueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            IssueKind::Parse => "parse",
            IssueKind::Validation => "validation",
            IssueKind::NotReady => "not_ready",
            IssueKind::Sequence => "sequence",
            IssueKind::Coverage => "coverage",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub kind: IssueKind,
    pub cut: Option<CutId>,
    pub message: String,
}

impl Issue {
    pub fn new(kind: IssueKind, cut: Option<CutId>, message: impl Into<String>) -> Self {
        Issue {
            kind,
            cut,
            message: message.into(),
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.cut {
            Some(id) => write!(f, "[{}] {id}: {}", self.kind.as_str(), self.message),
            None => write!(f, "[{}] {}", self.kind.as_str(), self.message),
        }
    }
}

pub fn retry_feedback(issues: &[Issue]) -> String {
    issues
        .iter()
        .map(|i| format!("- {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Error)]
pub enum CommitError {
    #[error("scene plan chunk rejected with {} issue(s):\n{}", .0.len(), retry_feedback(.0))]
    Rejected(Vec<Issue>),
    #[error("host failure before commit; nothing changed: {0}")]
    Host(String),
    #[error("commit failed and was rolled back: {0}")]
    RolledBack(String),
    #[error("commit failed ({failure}) and rollback also failed ({rollback_failure}); manual repair needed")]
    RollbackFailed {
        failure: String,
        rollback_failure: String,
    },
}

impl CommitError {
    pub fn issues(&self) -> &[Issue] {
        match self {
            CommitError::Rejected(issues) => issues,
            _ => &[],
        }
    }
}
