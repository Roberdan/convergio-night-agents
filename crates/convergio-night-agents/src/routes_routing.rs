//! HTTP API routes for smart routing monitoring and management.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Json;
use rusqlite::params;
use serde_json::json;

use crate::routes::NightAgentsState;

/// GET /api/night-agents/routing/stats — routing statistics.
pub async fn routing_stats(State(state): State<Arc<NightAgentsState>>) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})),
    };

    let total_defs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM night_agent_defs WHERE enabled = 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let auto_defs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM night_agent_defs WHERE enabled = 1 AND model = 'auto'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let mlx_defs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM night_agent_defs WHERE enabled = 1 AND model LIKE 'mlx:%'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let cloud_defs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM night_agent_defs WHERE enabled = 1 \
             AND model NOT IN ('auto') AND model NOT LIKE 'mlx:%' AND model NOT LIKE 'local:%'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Recent run stats by model usage (last 7 days)
    let runs_7d: Vec<serde_json::Value> = {
        let mut stmt = match conn.prepare(
            "SELECT d.model, r.status, COUNT(*) \
                 FROM night_runs r JOIN night_agent_defs d ON d.id = r.agent_def_id \
                 WHERE r.started_at > datetime('now', '-7 days') \
                 GROUP BY d.model, r.status ORDER BY d.model",
        ) {
            Ok(s) => s,
            Err(e) => return Json(json!({"error": format!("query failed: {e}")})),
        };
        stmt.query_map([], |row| {
            Ok(json!({
                "model": row.get::<_, String>(0)?,
                "status": row.get::<_, String>(1)?,
                "count": row.get::<_, i64>(2)?,
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    Json(json!({
        "definitions": {
            "total": total_defs,
            "auto": auto_defs,
            "mlx": mlx_defs,
            "cloud": cloud_defs,
        },
        "runs_last_7_days": runs_7d,
        "default_model": "auto",
        "routing_strategy": "T1/T2 → MLX local ($0), T3/T4 → Claude CLI cloud",
    }))
}

/// POST /api/night-agents/:id/routing — set routing mode for an agent def.
pub async fn set_routing(
    State(state): State<Arc<NightAgentsState>>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})),
    };

    let model = body.get("model").and_then(|v| v.as_str()).unwrap_or("auto");

    let valid = matches!(
        model,
        "auto" | "claude-haiku-4-5" | "claude-sonnet-4" | "claude-opus-4"
    ) || model.starts_with("mlx:")
        || model.starts_with("local:");

    if !valid {
        return Json(json!({"error": format!("invalid model: {model}")}));
    }

    match conn.execute(
        "UPDATE night_agent_defs SET model = ?1, updated_at = datetime('now') WHERE id = ?2",
        params![model, id],
    ) {
        Ok(1) => Json(json!({"status": "updated", "model": model})),
        Ok(_) => Json(json!({"error": "not found"})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

/// POST /api/night-agents/routing/migrate-all — set all defs to "auto".
pub async fn migrate_all_to_auto(
    State(state): State<Arc<NightAgentsState>>,
) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(json!({"error": e.to_string()})),
    };
    let updated = conn
        .execute(
            "UPDATE night_agent_defs SET model = 'auto', \
             updated_at = datetime('now') WHERE model != 'auto' AND enabled = 1",
            [],
        )
        .unwrap_or(0);
    Json(json!({"status": "migrated", "updated": updated}))
}
