//! Knowledge sync — scan tracked repos, detect changes, update
//! knowledge_base entries automatically.

use std::path::Path;

use convergio_db::pool::ConnPool;
use rusqlite::params;
use tracing::{error, info, warn};

use crate::knowledge_helpers::{
    detect_key_changes, git_head, git_log_since, scan_profile, upsert_knowledge,
};

/// Sync all enabled tracked projects.
pub fn sync_all_projects(pool: &ConnPool) {
    info!("knowledge-sync: starting scan cycle");
    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            error!("knowledge-sync: db error: {e}");
            return;
        }
    };

    let projects: Vec<(i64, String, String, Option<String>)> = {
        let mut stmt = match conn.prepare(
            "SELECT id, name, repo_path, last_scan_hash \
             FROM tracked_projects WHERE enabled = 1",
        ) {
            Ok(s) => s,
            Err(e) => {
                error!("knowledge-sync: query error: {e}");
                return;
            }
        };
        stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    let mut synced = 0u32;
    for (id, name, repo_path, last_hash) in &projects {
        if sync_project(&conn, *id, name, repo_path, last_hash.as_deref()) {
            synced += 1;
        }
    }
    info!(synced, total = projects.len(), "knowledge-sync: done");
}

/// Sync a single project by DB id.
pub fn sync_project_by_id(pool: &ConnPool, project_id: i64) {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            error!("sync_project_by_id: db error: {e}");
            return;
        }
    };
    let row: Result<(String, String, Option<String>), _> = conn.query_row(
        "SELECT name, repo_path, last_scan_hash \
         FROM tracked_projects WHERE id = ?1 AND enabled = 1",
        params![project_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    );
    match row {
        Ok((name, path, hash)) => {
            sync_project(&conn, project_id, &name, &path, hash.as_deref());
        }
        Err(e) => warn!(project_id, "project not found: {e}"),
    }
}

/// Run sync for a single project. Returns true if changes detected.
fn sync_project(
    conn: &rusqlite::Connection,
    project_id: i64,
    name: &str,
    repo_path: &str,
    last_hash: Option<&str>,
) -> bool {
    let path = Path::new(repo_path);
    if !path.is_dir() {
        warn!(name, repo_path, "repo path not found, skipping");
        return false;
    }
    let current_hash = match git_head(path) {
        Some(h) => h,
        None => {
            warn!(name, "not a git repo or git failed");
            return false;
        }
    };
    if last_hash == Some(current_hash.as_str()) {
        info!(name, "no changes since last scan");
        return false;
    }

    let profile = scan_profile(path);
    let changelog = if let Some(prev) = last_hash {
        git_log_since(path, prev)
    } else {
        "initial scan".to_string()
    };
    let key_changes = detect_key_changes(path, last_hash);

    upsert_knowledge(conn, name, "stack-profile", &profile);
    upsert_knowledge(conn, name, "recent-changes", &changelog);
    if !key_changes.is_empty() {
        upsert_knowledge(conn, name, "key-changes", &key_changes);
    }

    let profile_json =
        serde_json::to_string(&serde_json::json!({"profile": &profile})).unwrap_or_default();
    let _ = conn.execute(
        "UPDATE tracked_projects SET \
         last_scan_at = datetime('now'), \
         last_scan_hash = ?1, \
         scan_profile_json = ?2 \
         WHERE id = ?3",
        params![current_hash, profile_json, project_id],
    );

    info!(name, hash = %current_hash, "knowledge synced");
    true
}
