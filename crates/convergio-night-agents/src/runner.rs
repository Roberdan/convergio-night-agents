//! Night agent dispatch and lifecycle management.

use convergio_db::pool::ConnPool;
use rusqlite::params;
use tracing::{error, info, warn};

use crate::smart_spawner::spawn_smart;
use crate::spawner::spawn_claude_agent;

/// Dispatch all enabled night agents whose schedule matches now.
pub async fn dispatch_all(pool: &ConnPool) {
    info!("night-dispatch: starting nightly cycle");
    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            error!("night-dispatch: db pool error: {e}");
            return;
        }
    };

    let defs: Vec<(i64, String, String, String, String, i64)> = {
        let mut stmt = match conn.prepare(
            "SELECT id, name, schedule, agent_prompt, model, \
             max_runtime_secs \
             FROM night_agent_defs WHERE enabled = 1",
        ) {
            Ok(s) => s,
            Err(e) => {
                error!("night-dispatch: query error: {e}");
                return;
            }
        };
        stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    let now = chrono::Local::now();
    let mut dispatched = 0u32;

    for (def_id, name, schedule, _prompt, _model, _max_rt) in &defs {
        if !cron_matches_now(schedule, &now) {
            continue;
        }
        // Atomic insert-if-no-active — prevents TOCTOU race
        let inserted = conn
            .execute(
                "INSERT INTO night_runs (agent_def_id, status, started_at, \
                 node_name) \
                 SELECT ?1, 'running', datetime('now'), ?2 \
                 WHERE NOT EXISTS ( \
                     SELECT 1 FROM night_runs \
                     WHERE agent_def_id = ?1 \
                     AND status IN ('pending', 'running') \
                 )",
                params![def_id, local_node_name()],
            )
            .unwrap_or(0);
        if inserted == 0 {
            info!(agent = %name, "skip: already active");
            continue;
        }
        let run_id = conn.last_insert_rowid();
        info!(agent = %name, run_id, "dispatched");
        dispatched += 1;

        // Smart spawn: "auto" → MLX for simple, Claude CLI for complex
        let prompt_text = _prompt.clone();
        let model_str = _model.clone();
        let pool_clone = pool.clone();
        tokio::spawn(async move {
            if model_str == "auto"
                || model_str.starts_with("mlx:")
                || model_str.starts_with("local:")
            {
                spawn_smart(&pool_clone, run_id, &model_str, &prompt_text).await;
            } else {
                spawn_claude_agent(&pool_clone, run_id, &model_str, &prompt_text);
            }
        });
    }
    info!(dispatched, total = defs.len(), "night-dispatch: cycle done");
}

/// Dispatch a single night agent by definition ID (manual trigger).
pub async fn dispatch_single(pool: &ConnPool, def_id: i64) {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            error!("dispatch_single: db error: {e}");
            return;
        }
    };
    let name: String = match conn.query_row(
        "SELECT name FROM night_agent_defs WHERE id = ?1 AND enabled = 1",
        params![def_id],
        |r| r.get(0),
    ) {
        Ok(n) => n,
        Err(_) => {
            warn!(def_id, "agent not found or disabled");
            return;
        }
    };

    // Atomic insert-if-no-active — prevents duplicate concurrent runs
    let inserted = conn
        .execute(
            "INSERT INTO night_runs (agent_def_id, status, started_at, \
             node_name) \
             SELECT ?1, 'running', datetime('now'), ?2 \
             WHERE NOT EXISTS ( \
                 SELECT 1 FROM night_runs \
                 WHERE agent_def_id = ?1 \
                 AND status IN ('pending', 'running') \
             )",
            params![def_id, local_node_name()],
        )
        .unwrap_or(0);
    if inserted == 0 {
        warn!(def_id, agent = %name, "skip trigger: already active");
        return;
    }
    let run_id = conn.last_insert_rowid();
    info!(agent = %name, run_id, "manually triggered");

    let row: Result<(String, String), _> = conn.query_row(
        "SELECT agent_prompt, model FROM night_agent_defs WHERE id = ?1",
        params![def_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    );
    if let Ok((prompt, model)) = row {
        let pool_clone = pool.clone();
        tokio::spawn(async move {
            if model == "auto" || model.starts_with("mlx:") || model.starts_with("local:") {
                spawn_smart(&pool_clone, run_id, &model, &prompt).await;
            } else {
                spawn_claude_agent(&pool_clone, run_id, &model, &prompt);
            }
        });
    }
}

/// Reap runs stuck in 'running' state beyond max_runtime_secs.
pub fn reap_stale(pool: &ConnPool) {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            error!("night-reaper: db error: {e}");
            return;
        }
    };
    let reaped = conn
        .execute(
            "UPDATE night_runs SET status = 'failed', \
             completed_at = datetime('now'), \
             error_message = 'reaped: exceeded max runtime' \
             WHERE status = 'running' \
             AND started_at < datetime('now', '-2 hours')",
            [],
        )
        .unwrap_or(0);
    if reaped > 0 {
        warn!(reaped, "night-reaper: marked stale runs as failed");
    }
}

/// Simple cron matcher — checks if current time matches a cron expr.
/// Supports: `*`, `*/N`, and specific numbers. 5 fields.
fn cron_matches_now(cron: &str, now: &chrono::DateTime<chrono::Local>) -> bool {
    let parts: Vec<&str> = cron.split_whitespace().collect();
    if parts.len() != 5 {
        return false;
    }
    let minute = now.format("%M").to_string().parse::<u32>().unwrap_or(0);
    let hour = now.format("%H").to_string().parse::<u32>().unwrap_or(0);
    let day = now.format("%d").to_string().parse::<u32>().unwrap_or(1);
    let month = now.format("%m").to_string().parse::<u32>().unwrap_or(1);
    let weekday = now.format("%u").to_string().parse::<u32>().unwrap_or(1);

    field_matches(parts[0], minute)
        && field_matches(parts[1], hour)
        && field_matches(parts[2], day)
        && field_matches(parts[3], month)
        && field_matches(parts[4], weekday)
}

fn field_matches(field: &str, value: u32) -> bool {
    if field == "*" {
        return true;
    }
    if let Some(step) = field.strip_prefix("*/") {
        if let Ok(n) = step.parse::<u32>() {
            return n > 0 && value.is_multiple_of(n);
        }
    }
    // Range: "N-M" matches any value in [N, M] inclusive
    if let Some((lo, hi)) = field.split_once('-') {
        if let (Ok(lo), Ok(hi)) = (lo.parse::<u32>(), hi.parse::<u32>()) {
            return value >= lo && value <= hi;
        }
    }
    if let Ok(n) = field.parse::<u32>() {
        return n == value;
    }
    false
}

fn local_node_name() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_star_matches_any() {
        assert!(field_matches("*", 0));
        assert!(field_matches("*", 59));
    }

    #[test]
    fn field_step_matches() {
        assert!(field_matches("*/5", 0));
        assert!(field_matches("*/5", 15));
        assert!(!field_matches("*/5", 3));
    }

    #[test]
    fn field_exact_matches() {
        assert!(field_matches("0", 0));
        assert!(!field_matches("0", 1));
        assert!(field_matches("23", 23));
    }

    #[test]
    fn field_range_matches() {
        assert!(field_matches("0-6", 0));
        assert!(field_matches("0-6", 3));
        assert!(field_matches("0-6", 6));
        assert!(!field_matches("0-6", 7));
        assert!(!field_matches("1-5", 0));
        assert!(field_matches("1-5", 1));
        assert!(field_matches("1-5", 5));
    }
}
