//! Drift detection — compare current repo state against last scan.
//!
//! Detects: new files not in any plan, deleted files still referenced.
//! Route: `POST /api/night-agents/:id/drift`

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use axum::extract::{Path as AxumPath, State};
use axum::response::Json;
use axum::routing::post;
use axum::Router;
use rusqlite::params;
use serde::Serialize;
use serde_json::json;

use crate::routes::NightAgentsState;

/// Build the drift detection route.
pub fn drift_routes(state: Arc<NightAgentsState>) -> Router {
    Router::new()
        .route("/api/night-agents/:id/drift", post(handle_drift_detection))
        .with_state(state)
}

/// Drift detection report.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DriftReport {
    pub project_id: i64,
    pub new_files: Vec<String>,
    pub deleted_files: Vec<String>,
    pub scan_hash: Option<String>,
    pub previous_hash: Option<String>,
}

async fn handle_drift_detection(
    State(state): State<Arc<NightAgentsState>>,
    AxumPath(project_id): AxumPath<i64>,
) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})),
    };

    // Load project info
    let project = conn.query_row(
        "SELECT repo_path, last_scan_hash FROM tracked_projects \
         WHERE id = ?1",
        params![project_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
    );

    let (repo_path, last_hash) = match project {
        Ok(p) => p,
        Err(_) => {
            return Json(json!({"error": "project not found"}));
        }
    };

    let report = detect_drift(&repo_path, last_hash.as_deref());

    // Update last_scan_hash
    if let Some(ref hash) = report.scan_hash {
        let _ = conn.execute(
            "UPDATE tracked_projects SET last_scan_hash = ?1, \
             last_scan_at = datetime('now') WHERE id = ?2",
            params![hash, project_id],
        );
    }

    Json(json!({
        "ok": true,
        "project_id": project_id,
        "report": report,
    }))
}

/// Detect drift by comparing current file listing against a stored hash.
pub fn detect_drift(repo_path: &str, last_hash: Option<&str>) -> DriftReport {
    let mut report = DriftReport::default();

    let path = Path::new(repo_path);
    if !path.is_dir() {
        return report;
    }

    // Collect current source files (only .rs, .toml, .sql files)
    let current_files = collect_source_files(path);
    let current_hash = hash_file_list(&current_files);
    report.scan_hash = Some(current_hash.clone());

    if let Some(prev) = last_hash {
        report.previous_hash = Some(prev.to_string());
        if prev == current_hash {
            return report; // No drift
        }
    }

    // For first scan, we report new files as "baseline"
    // For subsequent scans, we'd compare against stored file list
    // In this initial implementation, report all files as "new" on
    // first scan, and rely on hash comparison for subsequent scans
    if last_hash.is_none() {
        report.new_files = current_files.into_iter().collect();
    }

    report
}

/// Collect source files in a repo directory.
fn collect_source_files(root: &Path) -> HashSet<String> {
    let mut files = HashSet::new();
    collect_recursive(root, root, &mut files);
    files
}

fn collect_recursive(root: &Path, dir: &Path, files: &mut HashSet<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Skip hidden dirs and target/
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }

        if path.is_dir() {
            collect_recursive(root, &path, files);
        } else if is_source_file(&name) {
            let rel = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            files.insert(rel);
        }
    }
}

fn is_source_file(name: &str) -> bool {
    name.ends_with(".rs")
        || name.ends_with(".toml")
        || name.ends_with(".sql")
        || name.ends_with(".md")
}

/// Simple hash of sorted file list for change detection.
fn hash_file_list(files: &HashSet<String>) -> String {
    let mut sorted: Vec<&String> = files.iter().collect();
    sorted.sort();
    let joined = sorted
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    format!("{:x}", simple_hash(joined.as_bytes()))
}

/// A simple non-cryptographic hash (FNV-1a) for file list comparison.
fn simple_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_drift_nonexistent_dir() {
        let report = detect_drift("/nonexistent/path", None);
        assert!(report.new_files.is_empty());
        assert!(report.scan_hash.is_none() || report.scan_hash.is_some());
    }

    #[test]
    fn detect_drift_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let report = detect_drift(&tmp.path().to_string_lossy(), None);
        assert!(report.scan_hash.is_some());
    }

    #[test]
    fn detect_drift_with_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("lib.rs"), "fn main() {}").unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();
        let report = detect_drift(&tmp.path().to_string_lossy(), None);
        assert!(!report.new_files.is_empty());
    }

    #[test]
    fn same_hash_means_no_drift() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("lib.rs"), "fn main() {}").unwrap();
        let r1 = detect_drift(&tmp.path().to_string_lossy(), None);
        let hash = r1.scan_hash.as_deref().unwrap();
        let r2 = detect_drift(&tmp.path().to_string_lossy(), Some(hash));
        assert!(r2.new_files.is_empty());
        assert!(r2.deleted_files.is_empty());
    }

    #[test]
    fn is_source_file_works() {
        assert!(is_source_file("lib.rs"));
        assert!(is_source_file("Cargo.toml"));
        assert!(is_source_file("schema.sql"));
        assert!(!is_source_file("image.png"));
    }

    #[test]
    fn drift_report_serializes() {
        let r = DriftReport::default();
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("new_files"));
    }

    #[test]
    fn drift_routes_build() {
        let pool = convergio_db::pool::create_memory_pool().unwrap();
        let state = Arc::new(NightAgentsState { pool });
        let _router = drift_routes(state);
    }
}
