//! Lint checks for contradictions and alignment issues.

use std::path::Path;

use crate::memory_lint_types::{LintCategory, LintFinding, LintSeverity};

/// Check for contradictions: references to files/paths that don't exist.
pub fn check_contradictions(
    project: &str,
    path: &Path,
    content: &str,
    repo_root: &Path,
) -> Vec<LintFinding> {
    let rel = path.display().to_string();
    let mut findings = Vec::new();

    for (i, line) in content.lines().enumerate() {
        for candidate in extract_path_refs(line) {
            let full = repo_root.join(&candidate);
            if !full.exists() && looks_like_real_path(&candidate) {
                findings.push(LintFinding {
                    project_name: project.into(),
                    file_path: rel.clone(),
                    line: Some((i + 1) as u32),
                    category: LintCategory::Contradiction,
                    severity: LintSeverity::Error,
                    rule: "missing-ref".into(),
                    message: format!("References '{candidate}' but file not found"),
                    suggestion: Some("Update or remove the reference".into()),
                });
            }
        }
    }
    findings
}

/// Check alignment: MEMORY.md index vs actual files in memory dir.
pub fn check_alignment(
    project: &str,
    memory_dir: &Path,
    index_content: Option<&str>,
) -> Vec<LintFinding> {
    let mut findings = Vec::new();
    let actual_files = list_md_files(memory_dir);

    if let Some(index) = index_content {
        for referenced in extract_referenced_files(index) {
            if !actual_files.iter().any(|f| f.ends_with(&referenced)) {
                findings.push(LintFinding {
                    project_name: project.into(),
                    file_path: "MEMORY.md".into(),
                    line: None,
                    category: LintCategory::Alignment,
                    severity: LintSeverity::Error,
                    rule: "index-missing-file".into(),
                    message: format!("MEMORY.md references '{referenced}' but file not found"),
                    suggestion: Some("Remove from index or create the file".into()),
                });
            }
        }
        for actual in &actual_files {
            let name = Path::new(actual)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name == "MEMORY.md" {
                continue;
            }
            if !index.contains(&name) {
                findings.push(LintFinding {
                    project_name: project.into(),
                    file_path: name.clone(),
                    line: None,
                    category: LintCategory::Alignment,
                    severity: LintSeverity::Warning,
                    rule: "file-not-indexed".into(),
                    message: format!("File '{name}' exists but not in MEMORY.md"),
                    suggestion: Some("Add to MEMORY.md or remove if stale".into()),
                });
            }
        }
    } else if !actual_files.is_empty() {
        findings.push(LintFinding {
            project_name: project.into(),
            file_path: "memory/".into(),
            line: None,
            category: LintCategory::Alignment,
            severity: LintSeverity::Warning,
            rule: "no-index".into(),
            message: format!(
                "Memory dir has {} files but no MEMORY.md index",
                actual_files.len()
            ),
            suggestion: Some("Create MEMORY.md to index memory files".into()),
        });
    }
    findings
}

// --- Helpers ---

pub(crate) fn extract_path_refs(line: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('`') {
            let candidate = &after[..end];
            if candidate.contains('/') || candidate.contains('.') {
                refs.push(candidate.to_string());
            }
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    refs
}

fn looks_like_real_path(s: &str) -> bool {
    let extensions = [
        ".rs", ".ts", ".tsx", ".js", ".md", ".toml", ".json", ".yaml",
    ];
    let has_ext = extensions.iter().any(|e| s.ends_with(e));
    let has_slash = s.contains('/');
    (has_ext || has_slash) && !s.starts_with("http") && !s.contains(' ')
}

fn list_md_files(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("md") {
                Some(p.display().to_string())
            } else {
                None
            }
        })
        .collect()
}

fn extract_referenced_files(index_content: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for line in index_content.lines() {
        for part in extract_path_refs(line) {
            if part.ends_with(".md") {
                refs.push(part);
            }
        }
        let mut rest = line;
        while let Some(start) = rest.find("](") {
            let after = &rest[start + 2..];
            if let Some(end) = after.find(')') {
                let target = &after[..end];
                if target.ends_with(".md") && !target.starts_with("http") {
                    refs.push(target.to_string());
                }
                rest = &after[end + 1..];
            } else {
                break;
            }
        }
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_backtick_paths() {
        let refs = extract_path_refs("see `src/main.rs` and `lib.rs`");
        assert_eq!(refs, vec!["src/main.rs", "lib.rs"]);
    }

    #[test]
    fn looks_like_path_filters() {
        assert!(looks_like_real_path("src/main.rs"));
        assert!(looks_like_real_path("daemon/crates/foo/bar.toml"));
        assert!(!looks_like_real_path("https://example.com/foo.rs"));
        assert!(!looks_like_real_path("some text here"));
    }

    #[test]
    fn extract_md_links() {
        let refs = extract_referenced_files("See [docs](notes.md) and [other](info.md)");
        assert_eq!(refs, vec!["notes.md", "info.md"]);
    }
}
