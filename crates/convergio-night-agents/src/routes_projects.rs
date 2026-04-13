//! HTTP API routes for tracked project management.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Json;
use rusqlite::params;
use serde_json::json;

use crate::routes::{safe_err, NightAgentsState};
use crate::types::CreateProjectBody;

pub async fn list_projects(State(state): State<Arc<NightAgentsState>>) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(safe_err("list_projects", &e)),
    };
    let mut stmt = match conn.prepare(
        "SELECT id, name, repo_path, remote_url, last_scan_at, \
         last_scan_hash, enabled, created_at \
         FROM tracked_projects ORDER BY name",
    ) {
        Ok(s) => s,
        Err(e) => return Json(safe_err("list_projects", &e)),
    };
    let rows: Vec<serde_json::Value> = stmt
        .query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "name": row.get::<_, String>(1)?,
                "repo_path": row.get::<_, String>(2)?,
                "remote_url": row.get::<_, Option<String>>(3)?,
                "last_scan_at": row.get::<_, Option<String>>(4)?,
                "last_scan_hash": row.get::<_, Option<String>>(5)?,
                "enabled": row.get::<_, bool>(6)?,
                "created_at": row.get::<_, String>(7)?,
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    Json(json!(rows))
}

pub async fn create_project(
    State(state): State<Arc<NightAgentsState>>,
    Json(body): Json<CreateProjectBody>,
) -> Json<serde_json::Value> {
    if let Err(e) = body.validate() {
        return Json(json!({"error": e}));
    }
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(safe_err("create_project", &e)),
    };
    match conn.execute(
        "INSERT INTO tracked_projects (name, repo_path, remote_url) \
         VALUES (?1, ?2, ?3)",
        params![body.name, body.repo_path, body.remote_url],
    ) {
        Ok(_) => {
            let id = conn.last_insert_rowid();
            Json(json!({"id": id, "status": "created"}))
        }
        Err(e) => Json(safe_err("create_project", &e)),
    }
}

pub async fn delete_project(
    State(state): State<Arc<NightAgentsState>>,
    Path(id): Path<i64>,
) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(safe_err("delete_project", &e)),
    };
    match conn.execute(
        "UPDATE tracked_projects SET enabled = 0 WHERE id = ?1",
        params![id],
    ) {
        Ok(n) if n > 0 => Json(json!({"status": "disabled"})),
        Ok(_) => Json(json!({"error": "not found"})),
        Err(e) => Json(safe_err("delete_project", &e)),
    }
}

pub async fn scan_project(
    State(state): State<Arc<NightAgentsState>>,
    Path(id): Path<i64>,
) -> Json<serde_json::Value> {
    let pool = state.pool.clone();
    tokio::spawn(async move {
        crate::knowledge::sync_project_by_id(&pool, id);
    });
    Json(json!({"status": "scan_triggered", "project_id": id}))
}
