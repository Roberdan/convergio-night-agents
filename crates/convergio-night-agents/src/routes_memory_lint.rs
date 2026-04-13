//! HTTP API routes for memory lint results.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Json;
use rusqlite::params;
use serde::Deserialize;
use serde_json::json;

use crate::routes::{safe_err, NightAgentsState};

#[derive(Debug, Deserialize, Default)]
pub struct LintListQuery {
    pub project: Option<String>,
    pub category: Option<String>,
    pub severity: Option<String>,
    pub limit: Option<u32>,
}

/// GET /api/night-agents/memory-lint — list findings.
pub async fn list_findings(
    State(state): State<Arc<NightAgentsState>>,
    Query(q): Query<LintListQuery>,
) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(safe_err("list_findings", &e)),
    };
    let limit = q.limit.unwrap_or(100).min(500) as i64;

    // Build dynamic WHERE clause
    let mut conditions = vec!["dismissed = 0".to_string()];
    let mut bind_values: Vec<String> = Vec::new();

    if let Some(ref proj) = q.project {
        bind_values.push(proj.clone());
        conditions.push(format!("project_name = ?{}", bind_values.len()));
    }
    if let Some(ref cat) = q.category {
        bind_values.push(cat.clone());
        conditions.push(format!("category = ?{}", bind_values.len()));
    }
    if let Some(ref sev) = q.severity {
        bind_values.push(sev.clone());
        conditions.push(format!("severity = ?{}", bind_values.len()));
    }

    let where_clause = conditions.join(" AND ");
    let sql = format!(
        "SELECT id, project_name, file_path, line, category, severity, \
         rule, message, suggestion, run_at \
         FROM memory_lint_results WHERE {where_clause} \
         ORDER BY \
           CASE severity WHEN 'error' THEN 0 \
           WHEN 'warning' THEN 1 ELSE 2 END, \
           id DESC LIMIT ?{}",
        bind_values.len() + 1
    );

    let result = {
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(e) => return Json(safe_err("list_findings", &e)),
        };
        // We need to bind dynamically — use a simple approach
        let all_params: Vec<Box<dyn rusqlite::types::ToSql>> = bind_values
            .iter()
            .map(|v| Box::new(v.clone()) as Box<dyn rusqlite::types::ToSql>)
            .chain(std::iter::once(
                Box::new(limit) as Box<dyn rusqlite::types::ToSql>
            ))
            .collect();
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            all_params.iter().map(|p| p.as_ref()).collect();

        stmt.query_map(param_refs.as_slice(), |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "project_name": row.get::<_, String>(1)?,
                "file_path": row.get::<_, String>(2)?,
                "line": row.get::<_, Option<i64>>(3)?,
                "category": row.get::<_, String>(4)?,
                "severity": row.get::<_, String>(5)?,
                "rule": row.get::<_, String>(6)?,
                "message": row.get::<_, String>(7)?,
                "suggestion": row.get::<_, Option<String>>(8)?,
                "run_at": row.get::<_, String>(9)?,
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
        .unwrap_or_default()
    };

    Json(json!({"findings": result, "count": result.len()}))
}

/// GET /api/night-agents/memory-lint/summary — aggregated summary.
pub async fn lint_summary(State(state): State<Arc<NightAgentsState>>) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(safe_err("lint_summary", &e)),
    };
    let mut stmt = match conn.prepare(
        "SELECT project_name, \
         COUNT(*) as total, \
         SUM(CASE WHEN severity = 'error' THEN 1 ELSE 0 END), \
         SUM(CASE WHEN severity = 'warning' THEN 1 ELSE 0 END), \
         SUM(CASE WHEN severity = 'info' THEN 1 ELSE 0 END), \
         SUM(CASE WHEN category = 'stale' THEN 1 ELSE 0 END), \
         SUM(CASE WHEN category = 'duplicate' THEN 1 ELSE 0 END), \
         SUM(CASE WHEN category = 'contradiction' THEN 1 ELSE 0 END), \
         SUM(CASE WHEN category = 'alignment' THEN 1 ELSE 0 END) \
         FROM memory_lint_results WHERE dismissed = 0 \
         GROUP BY project_name ORDER BY total DESC",
    ) {
        Ok(s) => s,
        Err(e) => return Json(safe_err("lint_summary", &e)),
    };
    let rows: Vec<serde_json::Value> = stmt
        .query_map([], |row| {
            Ok(json!({
                "project_name": row.get::<_, String>(0)?,
                "total": row.get::<_, i64>(1)?,
                "errors": row.get::<_, i64>(2)?,
                "warnings": row.get::<_, i64>(3)?,
                "info": row.get::<_, i64>(4)?,
                "stale": row.get::<_, i64>(5)?,
                "duplicates": row.get::<_, i64>(6)?,
                "contradictions": row.get::<_, i64>(7)?,
                "alignment": row.get::<_, i64>(8)?,
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    Json(json!(rows))
}

/// POST /api/night-agents/memory-lint/trigger — trigger lint on all projects.
pub async fn trigger_lint(State(state): State<Arc<NightAgentsState>>) -> Json<serde_json::Value> {
    let pool = state.pool.clone();
    tokio::spawn(async move {
        crate::memory_lint::lint_all_projects(&pool);
    });
    Json(json!({"status": "triggered"}))
}

/// POST /api/night-agents/projects/:id/memory-lint — lint single project.
pub async fn trigger_project_lint(
    State(state): State<Arc<NightAgentsState>>,
    Path(id): Path<i64>,
) -> Json<serde_json::Value> {
    let pool = state.pool.clone();
    tokio::spawn(async move {
        crate::memory_lint::lint_project_by_id(&pool, id);
    });
    Json(json!({"status": "triggered", "project_id": id}))
}

/// POST /api/night-agents/memory-lint/:id/dismiss — dismiss a finding.
pub async fn dismiss_finding(
    State(state): State<Arc<NightAgentsState>>,
    Path(id): Path<i64>,
) -> Json<serde_json::Value> {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => return Json(safe_err("dismiss_finding", &e)),
    };
    match conn.execute(
        "UPDATE memory_lint_results SET dismissed = 1 WHERE id = ?1",
        params![id],
    ) {
        Ok(n) if n > 0 => Json(json!({"status": "dismissed"})),
        Ok(_) => Json(json!({"error": "not found"})),
        Err(e) => Json(safe_err("dismiss_finding", &e)),
    }
}
