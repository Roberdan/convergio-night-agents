//! ci-optimizer — analyzes CI workflows for optimization opportunities.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiOptReport {
    pub project: String,
    pub workflows_analyzed: usize,
    pub findings: Vec<CiFinding>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiFinding {
    pub file: String,
    pub category: String,
    pub severity: String,
    pub message: String,
    pub suggestion: String,
}

/// Analyze all workflows under `repo_path/.github/workflows/`.
pub fn analyze_ci(repo_path: &str, project_name: &str) -> CiOptReport {
    let wf_dir = Path::new(repo_path).join(".github/workflows");
    let mut findings = Vec::new();
    let mut count = 0usize;

    if !wf_dir.exists() {
        return CiOptReport {
            project: project_name.to_string(),
            workflows_analyzed: 0,
            findings: vec![],
            summary: "No .github/workflows directory found".into(),
        };
    }

    let entries = match fs::read_dir(&wf_dir) {
        Ok(e) => e,
        Err(_) => {
            return CiOptReport {
                project: project_name.to_string(),
                workflows_analyzed: 0,
                findings: vec![],
                summary: "Cannot read workflows directory".into(),
            };
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if !name.ends_with(".yml") && !name.ends_with(".yaml") {
            continue;
        }
        count += 1;
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let fname = name.to_string();
        findings.extend(check_missing_cache(&fname, &content));
        findings.extend(check_sequential_jobs(&fname, &content));
        findings.extend(check_oversized_steps(&fname, &content));
        findings.extend(check_missing_timeout(&fname, &content));
    }

    let summary = build_summary(count, &findings);
    CiOptReport {
        project: project_name.to_string(),
        workflows_analyzed: count,
        findings,
        summary,
    }
}

/// Convert report to JSON value.
pub fn report_to_json(report: &CiOptReport) -> Value {
    json!({
        "project": report.project,
        "workflows_analyzed": report.workflows_analyzed,
        "findings_count": report.findings.len(),
        "findings": report.findings,
        "summary": report.summary,
    })
}

fn check_missing_cache(file: &str, content: &str) -> Vec<CiFinding> {
    let mut findings = Vec::new();
    let has_install = content.contains("npm install")
        || content.contains("npm ci")
        || content.contains("cargo build")
        || content.contains("pip install");
    let has_cache = content.contains("actions/cache") || content.contains("Swatinem/rust-cache");

    if has_install && !has_cache {
        findings.push(CiFinding {
            file: file.to_string(),
            category: "missing-cache".into(),
            severity: "warning".into(),
            message: "Dependency install without cache step".into(),
            suggestion: "Add actions/cache or language-specific \
                         cache action to speed up builds"
                .into(),
        });
    }
    findings
}

fn check_sequential_jobs(file: &str, content: &str) -> Vec<CiFinding> {
    let mut findings = Vec::new();
    let needs_count = content.matches("needs:").count();
    let jobs = count_jobs(content);

    if jobs > 2 && needs_count >= jobs - 1 {
        findings.push(CiFinding {
            file: file.to_string(),
            category: "sequential-jobs".into(),
            severity: "info".into(),
            message: format!("{jobs} jobs all chained sequentially via needs:"),
            suggestion: "Consider parallelizing independent jobs \
                         (e.g., lint + test can run concurrently)"
                .into(),
        });
    }
    findings
}

fn check_oversized_steps(file: &str, content: &str) -> Vec<CiFinding> {
    let mut findings = Vec::new();
    let mut in_run = false;
    let mut run_lines = 0u32;
    let mut step_name = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("- name:") {
            if in_run && run_lines > 15 {
                findings.push(CiFinding {
                    file: file.to_string(),
                    category: "oversized-step".into(),
                    severity: "warning".into(),
                    message: format!("Step '{step_name}' has {run_lines} lines"),
                    suggestion: "Split into smaller steps or use a \
                                 composite action"
                        .into(),
                });
            }
            step_name = trimmed
                .strip_prefix("- name:")
                .unwrap_or("")
                .trim()
                .to_string();
            in_run = false;
            run_lines = 0;
        } else if trimmed.starts_with("run:") {
            in_run = true;
            run_lines = 0;
        } else if in_run {
            if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("- ") {
                run_lines += 1;
            }
            if !line.starts_with(' ') && !line.starts_with('\t') {
                in_run = false;
            }
        }
    }

    if in_run && run_lines > 15 {
        findings.push(CiFinding {
            file: file.to_string(),
            category: "oversized-step".into(),
            severity: "warning".into(),
            message: format!("Step '{step_name}' has {run_lines} lines"),
            suggestion: "Split into smaller steps or use a composite \
                         action"
                .into(),
        });
    }
    findings
}

fn check_missing_timeout(file: &str, content: &str) -> Vec<CiFinding> {
    let mut findings = Vec::new();
    let jobs = count_jobs(content);
    let timeouts = content.matches("timeout-minutes:").count();

    if jobs > 0 && timeouts == 0 {
        findings.push(CiFinding {
            file: file.to_string(),
            category: "missing-timeout".into(),
            severity: "info".into(),
            message: format!("{jobs} jobs without timeout-minutes"),
            suggestion: "Add timeout-minutes to prevent runaway jobs \
                         consuming CI minutes"
                .into(),
        });
    }
    findings
}

fn count_jobs(content: &str) -> usize {
    let mut count = 0;
    let mut in_jobs = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "jobs:" {
            in_jobs = true;
            continue;
        }
        if in_jobs && !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
            in_jobs = false;
        }
        if in_jobs
            && !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && line.starts_with("  ")
            && !line.starts_with("    ")
        {
            count += 1;
        }
    }
    count
}

fn build_summary(count: usize, findings: &[CiFinding]) -> String {
    if count == 0 {
        return "No workflow files found".into();
    }
    let mut cats: HashMap<&str, usize> = HashMap::new();
    for f in findings {
        *cats.entry(f.category.as_str()).or_insert(0) += 1;
    }
    let parts: Vec<String> = cats.iter().map(|(k, v)| format!("{v} {k}")).collect();
    if parts.is_empty() {
        format!("Analyzed {count} workflows — no issues found")
    } else {
        format!("Analyzed {count} workflows — {}", parts.join(", "))
    }
}

#[cfg(test)]
#[path = "ci_optimizer_tests.rs"]
mod tests;
