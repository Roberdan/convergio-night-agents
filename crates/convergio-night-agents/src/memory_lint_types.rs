//! Types for memory lint findings.

use serde::{Deserialize, Serialize};

/// Severity level for a lint finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LintSeverity {
    Error,
    Warning,
    Info,
}

impl LintSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

/// Category of lint rule that produced a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintCategory {
    Stale,
    Duplicate,
    Contradiction,
    Alignment,
}

impl LintCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stale => "stale",
            Self::Duplicate => "duplicate",
            Self::Contradiction => "contradiction",
            Self::Alignment => "alignment",
        }
    }
}

/// A single lint finding for a memory file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintFinding {
    pub project_name: String,
    pub file_path: String,
    pub line: Option<u32>,
    pub category: LintCategory,
    pub severity: LintSeverity,
    pub rule: String,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Stored lint result row from DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintResultRow {
    pub id: i64,
    pub project_name: String,
    pub file_path: String,
    pub line: Option<i64>,
    pub category: String,
    pub severity: String,
    pub rule: String,
    pub message: String,
    pub suggestion: Option<String>,
    pub dismissed: bool,
    pub run_at: String,
}

/// Summary of a lint run for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintSummary {
    pub project_name: String,
    pub total: usize,
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
    pub stale: usize,
    pub duplicates: usize,
    pub contradictions: usize,
    pub alignment: usize,
}

impl LintSummary {
    pub fn from_findings(project_name: &str, findings: &[LintFinding]) -> Self {
        Self {
            project_name: project_name.to_string(),
            total: findings.len(),
            errors: findings
                .iter()
                .filter(|f| f.severity == LintSeverity::Error)
                .count(),
            warnings: findings
                .iter()
                .filter(|f| f.severity == LintSeverity::Warning)
                .count(),
            info: findings
                .iter()
                .filter(|f| f.severity == LintSeverity::Info)
                .count(),
            stale: findings
                .iter()
                .filter(|f| f.category == LintCategory::Stale)
                .count(),
            duplicates: findings
                .iter()
                .filter(|f| f.category == LintCategory::Duplicate)
                .count(),
            contradictions: findings
                .iter()
                .filter(|f| f.category == LintCategory::Contradiction)
                .count(),
            alignment: findings
                .iter()
                .filter(|f| f.category == LintCategory::Alignment)
                .count(),
        }
    }
}
