//! Memory lint orchestrator — scans tracked projects and runs all rules.

use std::path::{Path, PathBuf};

use convergio_db::pool::ConnPool;
use rusqlite::params;
use tracing::{error, info, warn};

use crate::memory_lint_checks;
use crate::memory_lint_rules;
use crate::memory_lint_types::{LintFinding, LintSummary};

/// Well-known memory file locations relative to a repo root.
const MEMORY_CANDIDATES: &[&str] = &[
    "MEMORY.md",
    "memory/MEMORY.md",
    ".claude/CLAUDE.md",
    "AGENTS.md",
    "CLAUDE.md",
];

/// Well-known memory directories.
const MEMORY_DIRS: &[&str] = &["memory", ".copilot-tracking/memory"];

/// Run lint on all tracked projects.
pub fn lint_all_projects(pool: &ConnPool) {
    info!("memory-lint: starting scan cycle");
    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            error!("memory-lint: db error: {e}");
            return;
        }
    };

    let projects: Vec<(i64, String, String)> = {
        let mut stmt = match conn
            .prepare("SELECT id, name, repo_path FROM tracked_projects WHERE enabled = 1")
        {
            Ok(s) => s,
            Err(e) => {
                error!("memory-lint: query error: {e}");
                return;
            }
        };
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    };

    let mut total_findings = 0usize;
    for (_id, name, repo_path) in &projects {
        let findings = lint_project(name, Path::new(repo_path));
        let summary = LintSummary::from_findings(name, &findings);
        total_findings += findings.len();
        store_findings(&conn, &findings);
        info!(
            project = %name,
            total = summary.total,
            errors = summary.errors,
            warnings = summary.warnings,
            "memory-lint: project scanned"
        );
    }
    info!(
        projects = projects.len(),
        total_findings, "memory-lint: cycle done"
    );
}

/// Lint a single project by ID (manual trigger).
pub fn lint_project_by_id(pool: &ConnPool, project_id: i64) {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            error!("lint_project_by_id: db error: {e}");
            return;
        }
    };
    let row: Result<(String, String), _> = conn.query_row(
        "SELECT name, repo_path FROM tracked_projects \
         WHERE id = ?1 AND enabled = 1",
        params![project_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    );
    match row {
        Ok((name, path)) => {
            let findings = lint_project(&name, Path::new(&path));
            store_findings(&conn, &findings);
            info!(project = %name, count = findings.len(), "lint complete");
        }
        Err(e) => warn!(project_id, "project not found: {e}"),
    }
}

/// Run all lint rules on a single project.
fn lint_project(project: &str, repo_root: &Path) -> Vec<LintFinding> {
    if !repo_root.is_dir() {
        warn!(project, path = %repo_root.display(), "repo not found");
        return Vec::new();
    }

    let memory_files = discover_memory_files(repo_root);
    if memory_files.is_empty() {
        info!(project, "no memory files found");
        return Vec::new();
    }

    let contents: Vec<(PathBuf, String)> = memory_files
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok().map(|c| (p.clone(), c)))
        .collect();

    let mut findings = Vec::new();

    // 1. Stale checks per file
    for (path, content) in &contents {
        findings.extend(memory_lint_rules::check_stale(project, path, content));
    }

    // 2. Cross-file duplicates
    let file_refs: Vec<(&Path, &str)> = contents
        .iter()
        .map(|(p, c)| (p.as_path(), c.as_str()))
        .collect();
    findings.extend(memory_lint_rules::check_duplicates(project, &file_refs));

    // 3. Contradictions (missing file references)
    for (path, content) in &contents {
        findings.extend(memory_lint_checks::check_contradictions(
            project, path, content, repo_root,
        ));
    }

    // 4. Alignment checks per memory directory
    for dir_name in MEMORY_DIRS {
        let dir = repo_root.join(dir_name);
        if dir.is_dir() {
            let index_path = dir.join("MEMORY.md");
            let index_content = std::fs::read_to_string(&index_path).ok();
            findings.extend(memory_lint_checks::check_alignment(
                project,
                &dir,
                index_content.as_deref(),
            ));
        }
    }

    findings
}

/// Discover memory-related files in a repo.
fn discover_memory_files(repo_root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    // Check well-known locations
    for candidate in MEMORY_CANDIDATES {
        let path = repo_root.join(candidate);
        if path.is_file() {
            files.push(path);
        }
    }
    // Scan memory directories for extra .md files
    for dir_name in MEMORY_DIRS {
        let dir = repo_root.join(dir_name);
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("md") && !files.contains(&p) {
                    files.push(p);
                }
            }
        }
    }
    files
}

/// Persist findings to DB (replaces previous results for the project).
fn store_findings(conn: &rusqlite::Connection, findings: &[LintFinding]) {
    if findings.is_empty() {
        return;
    }
    let project = &findings[0].project_name;
    // Clear old results for this project
    if let Err(e) = conn.execute(
        "DELETE FROM memory_lint_results WHERE project_name = ?1",
        params![project],
    ) {
        warn!(project = %project, "lint store: delete failed: {e}");
    }
    for f in findings {
        if let Err(e) = conn.execute(
            "INSERT INTO memory_lint_results \
             (project_name, file_path, line, category, severity, \
              rule, message, suggestion, run_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))",
            params![
                f.project_name,
                f.file_path,
                f.line.map(|l| l as i64),
                f.category.as_str(),
                f.severity.as_str(),
                f.rule,
                f.message,
                f.suggestion,
            ],
        ) {
            warn!(project = %project, rule = %f.rule, "lint store: insert failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_files_in_current_repo() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let files = discover_memory_files(&root);
        // Our repo has AGENTS.md at minimum
        assert!(
            files.iter().any(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().contains("AGENTS"))
                    .unwrap_or(false)
            }),
            "should find AGENTS.md: {files:?}"
        );
    }
}
