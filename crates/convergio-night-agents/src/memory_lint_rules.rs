//! Lint rules for memory files — stale entries and cross-file duplicates.

use std::collections::HashMap;
use std::path::Path;

use crate::memory_lint_types::{LintCategory, LintFinding, LintSeverity};

/// Stale-entry patterns that indicate outdated content.
const STALE_MARKERS: &[&str] = &[
    "partial",
    "WIP",
    "TODO:",
    "FIXME:",
    "HACK:",
    "in progress",
    "not yet",
    "TBD",
    "DEPRECATED",
    "old approach",
    "legacy",
];

/// Phase reference pattern: "Fase N" or "Phase N".
fn is_phase_ref(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    for prefix in &["fase ", "phase "] {
        if let Some(idx) = lower.find(prefix) {
            let rest = &line[idx + prefix.len()..];
            let phase_id: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
            if !phase_id.is_empty() {
                return Some(format!("{}{}", &prefix.trim(), phase_id));
            }
        }
    }
    None
}

/// Check a single file for stale entries.
pub fn check_stale(project: &str, path: &Path, content: &str) -> Vec<LintFinding> {
    let rel = path.display().to_string();
    let mut findings = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        for marker in STALE_MARKERS {
            if trimmed.contains(marker) {
                findings.push(LintFinding {
                    project_name: project.into(),
                    file_path: rel.clone(),
                    line: Some((i + 1) as u32),
                    category: LintCategory::Stale,
                    severity: LintSeverity::Warning,
                    rule: "stale-marker".into(),
                    message: format!("Possible stale entry: contains '{marker}'"),
                    suggestion: Some("Review and remove if no longer relevant".into()),
                });
            }
        }
        if let Some(phase) = is_phase_ref(trimmed) {
            findings.push(LintFinding {
                project_name: project.into(),
                file_path: rel.clone(),
                line: Some((i + 1) as u32),
                category: LintCategory::Stale,
                severity: LintSeverity::Info,
                rule: "phase-ref".into(),
                message: format!("References '{phase}' — verify still active"),
                suggestion: Some("Remove if phase is completed".into()),
            });
        }
    }
    findings
}

/// Normalize a line for duplicate comparison.
pub(crate) fn normalize_line(line: &str) -> String {
    line.trim()
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Check for duplicates across multiple files.
pub fn check_duplicates(project: &str, files: &[(&Path, &str)]) -> Vec<LintFinding> {
    let mut findings = Vec::new();
    let mut seen: HashMap<String, Vec<(String, u32)>> = HashMap::new();

    for (path, content) in files {
        let rel = path.display().to_string();
        for (i, line) in content.lines().enumerate() {
            let norm = normalize_line(line);
            if norm.len() < 20 {
                continue;
            }
            seen.entry(norm)
                .or_default()
                .push((rel.clone(), (i + 1) as u32));
        }
    }

    for (text, locations) in &seen {
        if locations.len() < 2 {
            continue;
        }
        let unique_files: Vec<&str> = locations
            .iter()
            .map(|(f, _)| f.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        if unique_files.len() < 2 {
            continue;
        }
        let first = &locations[0];
        let others: Vec<String> = locations[1..]
            .iter()
            .map(|(f, l)| format!("{f}:{l}"))
            .collect();
        let preview = if text.len() > 60 {
            format!("{}...", &text[..60])
        } else {
            text.clone()
        };
        findings.push(LintFinding {
            project_name: project.into(),
            file_path: first.0.clone(),
            line: Some(first.1),
            category: LintCategory::Duplicate,
            severity: LintSeverity::Warning,
            rule: "cross-file-dup".into(),
            message: format!("Duplicated in: {}", others.join(", ")),
            suggestion: Some(format!("Content: \"{preview}\"")),
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_markers_detected() {
        let content = "Use the new API\nThis is WIP\nAll good here\n";
        let findings = check_stale("test", Path::new("test.md"), content);
        assert!(findings.iter().any(|f| f.rule == "stale-marker"));
    }

    #[test]
    fn phase_refs_detected() {
        let content = "Fase 23d partial work done\nPhase 5 complete\n";
        let findings = check_stale("test", Path::new("test.md"), content);
        assert!(findings.iter().any(|f| f.rule == "phase-ref"));
    }

    #[test]
    fn duplicates_across_files() {
        let f1 = (
            Path::new("a.md"),
            "This is a unique and specific line about the project setup\n",
        );
        let f2 = (
            Path::new("b.md"),
            "This is a unique and specific line about the project setup\n",
        );
        let findings = check_duplicates("test", &[f1, f2]);
        assert!(!findings.is_empty());
    }

    #[test]
    fn normalize_line_strips_punctuation() {
        let norm = normalize_line("  Hello, World! -- test  ");
        assert_eq!(norm, "hello world test");
    }
}
