//! HTTP API routes for night agent run management.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Json;
use rusqlite::params;
use serde_json::json;

use crate::routes::{safe_err, NightAgentsState};
use crate::types::ListQuery;

pub async fn trigger_def(
    State(state): State<Arc<NightAgentsState>>,
    Path(id): Path<i64>,
) -> Json<serde_json::Value> {
    let pool = state.pool.clone();
    tokio::spawn(async move {
        crate::runner::dispatch_single(&pool, id).await;
    });
    Json(json!({"status": "triggered", "agent_def_id": id}))
}

pub async fn list_runs(
    State(state): State<Arc<NightAgentsState>>,
    Path(id): Path<i64>,
    Query(q): Query<ListQuery>,
) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(safe_err("list_runs", &e)),
    };
    let limit = q.limit.unwrap_or(20).min(100) as i64;
    let offset = q.offset.unwrap_or(0) as i64;
    let mut stmt = match conn.prepare(
        "SELECT id, status, node_name, pid, started_at, completed_at, \
         outcome, error_message, tokens_used, cost_usd \
         FROM night_runs WHERE agent_def_id = ?1 \
         ORDER BY id DESC LIMIT ?2 OFFSET ?3",
    ) {
        Ok(s) => s,
        Err(e) => return Json(safe_err("list_runs", &e)),
    };
    let rows: Vec<serde_json::Value> = stmt
        .query_map(params![id, limit, offset], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "status": row.get::<_, String>(1)?,
                "node_name": row.get::<_, Option<String>>(2)?,
                "pid": row.get::<_, Option<i64>>(3)?,
                "started_at": row.get::<_, Option<String>>(4)?,
                "completed_at": row.get::<_, Option<String>>(5)?,
                "outcome": row.get::<_, Option<String>>(6)?,
                "error_message": row.get::<_, Option<String>>(7)?,
                "tokens_used": row.get::<_, i64>(8)?,
                "cost_usd": row.get::<_, f64>(9)?,
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    Json(json!(rows))
}

pub async fn active_runs(State(state): State<Arc<NightAgentsState>>) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(safe_err("active_runs", &e)),
    };
    let mut stmt = match conn.prepare(
        "SELECT r.id, r.agent_def_id, d.name, r.status, r.node_name, \
         r.pid, r.started_at \
         FROM night_runs r \
         JOIN night_agent_defs d ON d.id = r.agent_def_id \
         WHERE r.status IN ('pending', 'running') ORDER BY r.id",
    ) {
        Ok(s) => s,
        Err(e) => return Json(safe_err("active_runs", &e)),
    };
    let rows: Vec<serde_json::Value> = stmt
        .query_map([], |row| {
            Ok(json!({
                "run_id": row.get::<_, i64>(0)?,
                "agent_def_id": row.get::<_, i64>(1)?,
                "agent_name": row.get::<_, String>(2)?,
                "status": row.get::<_, String>(3)?,
                "node_name": row.get::<_, Option<String>>(4)?,
                "pid": row.get::<_, Option<i64>>(5)?,
                "started_at": row.get::<_, Option<String>>(6)?,
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    Json(json!(rows))
}

pub async fn cancel_run(
    State(state): State<Arc<NightAgentsState>>,
    Path((_id, run_id)): Path<(i64, i64)>,
) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(safe_err("cancel_run", &e)),
    };
    match conn.execute(
        "UPDATE night_runs SET status = 'cancelled', \
         completed_at = datetime('now') \
         WHERE id = ?1 AND status IN ('pending', 'running')",
        params![run_id],
    ) {
        Ok(n) if n > 0 => Json(json!({"status": "cancelled"})),
        Ok(_) => Json(json!({"error": "run not found or not active"})),
        Err(e) => Json(safe_err("cancel_run", &e)),
    }
}
