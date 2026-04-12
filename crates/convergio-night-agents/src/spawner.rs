//! Claude CLI agent spawner for night runs.

use std::process::Command;

use convergio_db::pool::ConnPool;
use rusqlite::params;
use tracing::error;

/// Spawn a Claude CLI agent for a night run.
pub fn spawn_claude_agent(pool: &ConnPool, run_id: i64, model: &str, prompt: &str) {
    let claude_bin = resolve_claude_bin();
    let Some(bin) = claude_bin else {
        mark_run_failed(pool, run_id, "claude binary not found");
        return;
    };
    let result = Command::new(&bin)
        .args(["--dangerously-skip-permissions"])
        .args(["--model", model])
        .args(["--max-turns", "30"])
        .args(["-p", prompt])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();
    match result {
        Ok(output) if output.status.success() => {
            let outcome = String::from_utf8_lossy(&output.stdout);
            let summary = if outcome.len() > 2000 {
                format!("{}...", &outcome[..2000])
            } else {
                outcome.to_string()
            };
            mark_run_completed(pool, run_id, &summary);
        }
        Ok(output) => {
            let err = String::from_utf8_lossy(&output.stderr);
            mark_run_failed(pool, run_id, &err);
        }
        Err(e) => mark_run_failed(pool, run_id, &e.to_string()),
    }
}

fn resolve_claude_bin() -> Option<String> {
    if let Ok(bin) = std::env::var("CONVERGIO_CLAUDE_BIN") {
        return Some(bin);
    }
    let candidates = [
        dirs::home_dir().map(|d| d.join(".local/bin/claude").to_string_lossy().to_string()),
        dirs::home_dir().map(|d| d.join(".claude/bin/claude").to_string_lossy().to_string()),
        Some("/usr/local/bin/claude".into()),
        Some("/opt/homebrew/bin/claude".into()),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|c| std::path::Path::new(c).exists())
}

pub fn mark_run_completed(pool: &ConnPool, run_id: i64, outcome: &str) {
    if let Ok(conn) = pool.get() {
        let _ = conn.execute(
            "UPDATE night_runs SET status = 'completed', \
             completed_at = datetime('now'), outcome = ?1 WHERE id = ?2",
            params![outcome, run_id],
        );
    }
}

pub fn mark_run_failed(pool: &ConnPool, run_id: i64, error_msg: &str) {
    error!(run_id, error_msg, "night run failed");
    if let Ok(conn) = pool.get() {
        let _ = conn.execute(
            "UPDATE night_runs SET status = 'failed', \
             completed_at = datetime('now'), error_message = ?1 WHERE id = ?2",
            params![error_msg, run_id],
        );
    }
}
