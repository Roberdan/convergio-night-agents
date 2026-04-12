//! HTTP API routes for night agent management.

use axum::extract::{Path, State};
use axum::response::Json;
use axum::routing::{delete, get, post, put};
use axum::Router;
use rusqlite::params;
use serde_json::json;
use std::sync::Arc;

use crate::routes_memory_lint;
use crate::routes_projects;
use crate::routes_routing;
use crate::routes_runs;
use crate::types::CreateAgentBody;
use convergio_db::pool::ConnPool;

pub struct NightAgentsState {
    pub pool: ConnPool,
}

pub fn night_agents_routes(state: Arc<NightAgentsState>) -> Router {
    Router::new()
        .route("/api/night-agents", get(list_defs))
        .route("/api/night-agents", post(create_def))
        .route("/api/night-agents/:id", get(get_def))
        .route("/api/night-agents/:id", put(update_def))
        .route("/api/night-agents/:id", delete(delete_def))
        .route(
            "/api/night-agents/:id/trigger",
            post(routes_runs::trigger_def),
        )
        .route("/api/night-agents/:id/runs", get(routes_runs::list_runs))
        .route(
            "/api/night-agents/runs/active",
            get(routes_runs::active_runs),
        )
        .route(
            "/api/night-agents/:id/runs/:run_id/cancel",
            post(routes_runs::cancel_run),
        )
        .route(
            "/api/night-agents/projects",
            get(routes_projects::list_projects),
        )
        .route(
            "/api/night-agents/projects",
            post(routes_projects::create_project),
        )
        .route(
            "/api/night-agents/projects/:id",
            delete(routes_projects::delete_project),
        )
        .route(
            "/api/night-agents/projects/:id/scan",
            post(routes_projects::scan_project),
        )
        .route(
            "/api/night-agents/memory-lint",
            get(routes_memory_lint::list_findings),
        )
        .route(
            "/api/night-agents/memory-lint/summary",
            get(routes_memory_lint::lint_summary),
        )
        .route(
            "/api/night-agents/memory-lint/trigger",
            post(routes_memory_lint::trigger_lint),
        )
        .route(
            "/api/night-agents/projects/:id/memory-lint",
            post(routes_memory_lint::trigger_project_lint),
        )
        .route(
            "/api/night-agents/memory-lint/:id/dismiss",
            post(routes_memory_lint::dismiss_finding),
        )
        .route(
            "/api/night-agents/routing/stats",
            get(routes_routing::routing_stats),
        )
        .route(
            "/api/night-agents/:id/routing",
            post(routes_routing::set_routing),
        )
        .route(
            "/api/night-agents/routing/migrate-all",
            post(routes_routing::migrate_all_to_auto),
        )
        .with_state(state)
}

async fn list_defs(State(state): State<Arc<NightAgentsState>>) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})),
    };
    let mut stmt = match conn.prepare(
        "SELECT d.id, d.name, d.org_id, d.description, d.schedule, \
         d.model, d.enabled, d.max_runtime_secs, d.created_at, \
         d.updated_at, \
         (SELECT r.status FROM night_runs r \
          WHERE r.agent_def_id = d.id \
          ORDER BY r.id DESC LIMIT 1) as last_status, \
         (SELECT r.completed_at FROM night_runs r \
          WHERE r.agent_def_id = d.id \
          ORDER BY r.id DESC LIMIT 1) as last_run_at \
         FROM night_agent_defs d ORDER BY d.name",
    ) {
        Ok(s) => s,
        Err(e) => return Json(json!({"error": e.to_string()})),
    };
    let rows: Vec<serde_json::Value> = stmt
        .query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "name": row.get::<_, String>(1)?,
                "org_id": row.get::<_, Option<String>>(2)?,
                "description": row.get::<_, Option<String>>(3)?,
                "schedule": row.get::<_, String>(4)?,
                "model": row.get::<_, String>(5)?,
                "enabled": row.get::<_, bool>(6)?,
                "max_runtime_secs": row.get::<_, i64>(7)?,
                "created_at": row.get::<_, String>(8)?,
                "updated_at": row.get::<_, String>(9)?,
                "last_status": row.get::<_, Option<String>>(10)?,
                "last_run_at": row.get::<_, Option<String>>(11)?,
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    Json(json!(rows))
}

async fn create_def(
    State(state): State<Arc<NightAgentsState>>,
    Json(body): Json<CreateAgentBody>,
) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})),
    };
    match conn.execute(
        "INSERT INTO night_agent_defs \
         (name, org_id, description, schedule, agent_prompt, model, \
          max_runtime_secs) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            body.name,
            body.org_id,
            body.description,
            body.schedule,
            body.agent_prompt,
            body.model,
            body.max_runtime_secs,
        ],
    ) {
        Ok(_) => {
            let id = conn.last_insert_rowid();
            Json(json!({"id": id, "status": "created"}))
        }
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn get_def(
    State(state): State<Arc<NightAgentsState>>,
    Path(id): Path<i64>,
) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})),
    };
    let def = conn.query_row(
        "SELECT id, name, org_id, description, schedule, agent_prompt, \
         model, enabled, max_runtime_secs, created_at, updated_at \
         FROM night_agent_defs WHERE id = ?1",
        params![id],
        |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "name": row.get::<_, String>(1)?,
                "org_id": row.get::<_, Option<String>>(2)?,
                "description": row.get::<_, Option<String>>(3)?,
                "schedule": row.get::<_, String>(4)?,
                "agent_prompt": row.get::<_, String>(5)?,
                "model": row.get::<_, String>(6)?,
                "enabled": row.get::<_, bool>(7)?,
                "max_runtime_secs": row.get::<_, i64>(8)?,
                "created_at": row.get::<_, String>(9)?,
                "updated_at": row.get::<_, String>(10)?,
            }))
        },
    );
    match def {
        Ok(d) => Json(d),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn update_def(
    State(state): State<Arc<NightAgentsState>>,
    Path(id): Path<i64>,
    Json(body): Json<CreateAgentBody>,
) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})),
    };
    match conn.execute(
        "UPDATE night_agent_defs SET \
         name = ?1, org_id = ?2, description = ?3, schedule = ?4, \
         agent_prompt = ?5, model = ?6, max_runtime_secs = ?7, \
         updated_at = datetime('now') \
         WHERE id = ?8",
        params![
            body.name,
            body.org_id,
            body.description,
            body.schedule,
            body.agent_prompt,
            body.model,
            body.max_runtime_secs,
            id,
        ],
    ) {
        Ok(n) if n > 0 => Json(json!({"status": "updated"})),
        Ok(_) => Json(json!({"error": "not found"})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn delete_def(
    State(state): State<Arc<NightAgentsState>>,
    Path(id): Path<i64>,
) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})),
    };
    match conn.execute(
        "UPDATE night_agent_defs SET enabled = 0, \
         updated_at = datetime('now') WHERE id = ?1",
        params![id],
    ) {
        Ok(n) if n > 0 => Json(json!({"status": "disabled"})),
        Ok(_) => Json(json!({"error": "not found"})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}
