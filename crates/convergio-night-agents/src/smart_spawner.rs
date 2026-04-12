//! Smart spawner — dual-path execution for night agent tasks.
//!
//! - **Inference path** (T1/T2): uses ModelRouter → MLX local ($0)
//!   for simple analysis tasks (lint, classify, summarize).
//! - **Agent path** (T3/T4): spawns Claude CLI for tasks needing
//!   full coding agent capabilities (code review, refactoring).
//!
//! Falls back to Claude CLI if MLX/inference is unavailable.

use convergio_db::pool::ConnPool;
use rusqlite::params;
use tracing::{error, info, warn};

use crate::spawner::mark_run_completed;

/// Task complexity hint stored in the night_agent_defs.
/// Determines whether to use inference (local) or agent (cloud).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskComplexity {
    /// Simple analysis — text processing, no tools needed.
    /// Routes to MLX/local inference.
    Simple,
    /// Complex agent work — needs file access, terminal, git.
    /// Routes to Claude CLI or Copilot.
    Agent,
}

impl TaskComplexity {
    pub fn from_model_str(model: &str) -> Self {
        match model {
            m if m.starts_with("mlx:") => Self::Simple,
            m if m.starts_with("local:") => Self::Simple,
            "auto" => Self::Simple,
            _ => Self::Agent,
        }
    }

    /// Classify a prompt by inspecting its content.
    pub fn classify_prompt(prompt: &str) -> Self {
        let lower = prompt.to_lowercase();
        let agent_keywords = [
            "refactor",
            "implement",
            "create pr",
            "git commit",
            "write code",
            "edit file",
            "fix bug",
            "review code",
            "architecture",
        ];
        if agent_keywords.iter().any(|kw| lower.contains(kw)) {
            Self::Agent
        } else {
            Self::Simple
        }
    }
}

/// Spawn a night agent task using the smart dual-path strategy.
///
/// 1. Determine complexity from model field or prompt analysis
/// 2. Simple → inference path (ModelRouter → MLX/local)
/// 3. Agent → Claude CLI path (existing spawner)
pub async fn spawn_smart(pool: &ConnPool, run_id: i64, model: &str, prompt: &str) {
    let complexity = if model == "auto" {
        TaskComplexity::classify_prompt(prompt)
    } else {
        TaskComplexity::from_model_str(model)
    };

    info!(
        run_id,
        complexity = ?complexity,
        model,
        "smart-spawner: routing task"
    );

    match complexity {
        TaskComplexity::Simple => {
            spawn_inference(pool, run_id, prompt).await;
        }
        TaskComplexity::Agent => {
            crate::spawner::spawn_claude_agent(pool, run_id, model, prompt);
        }
    }
}

/// Inference path — call MLX/local model directly via HTTP.
/// No Claude CLI, no agent overhead, $0 cost.
async fn spawn_inference(pool: &ConnPool, run_id: i64, prompt: &str) {
    let model_name = resolve_mlx_model();
    info!(run_id, model = %model_name, "inference path: using local model");

    match crate::inference_bridge::call_local(&model_name, prompt).await {
        Ok(response) => {
            let summary = crate::spawner::truncate_safe(&response, 2000);
            mark_run_completed(pool, run_id, &summary);
        }
        Err(e) => {
            warn!(run_id, error = %e, "inference failed, falling back to Claude CLI");
            let fallback_model = "claude-haiku-4-5";
            crate::spawner::spawn_claude_agent(pool, run_id, fallback_model, prompt);
        }
    }
}

/// Resolve the MLX model name from env or default.
fn resolve_mlx_model() -> String {
    std::env::var("CONVERGIO_MLX_MODEL")
        .unwrap_or_else(|_| "mlx-community/Qwen2.5-Coder-7B-Instruct-4bit".into())
}

/// Update a night agent def to use the smart spawner.
/// Sets model to "auto" which enables prompt-based classification.
pub fn enable_smart_routing(pool: &ConnPool, def_id: i64) {
    if let Ok(conn) = pool.get() {
        let result = conn.execute(
            "UPDATE night_agent_defs SET model = 'auto', \
             updated_at = datetime('now') WHERE id = ?1",
            params![def_id],
        );
        match result {
            Ok(1) => info!(def_id, "smart routing enabled"),
            Ok(_) => warn!(def_id, "agent def not found"),
            Err(e) => error!(def_id, error = %e, "failed to enable smart routing"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_simple_prompts() {
        assert_eq!(
            TaskComplexity::classify_prompt("Scan memory files for stale entries"),
            TaskComplexity::Simple
        );
        assert_eq!(
            TaskComplexity::classify_prompt("List changed files since last scan"),
            TaskComplexity::Simple
        );
        assert_eq!(
            TaskComplexity::classify_prompt("Summarize today's agent activity"),
            TaskComplexity::Simple
        );
    }

    #[test]
    fn classify_agent_prompts() {
        assert_eq!(
            TaskComplexity::classify_prompt("Refactor the spawner module"),
            TaskComplexity::Agent
        );
        assert_eq!(
            TaskComplexity::classify_prompt("Review code in the auth crate"),
            TaskComplexity::Agent
        );
        assert_eq!(
            TaskComplexity::classify_prompt("Implement new endpoint and create PR"),
            TaskComplexity::Agent
        );
    }

    #[test]
    fn model_str_determines_complexity() {
        assert_eq!(
            TaskComplexity::from_model_str("auto"),
            TaskComplexity::Simple
        );
        assert_eq!(
            TaskComplexity::from_model_str("mlx:qwen2.5"),
            TaskComplexity::Simple
        );
        assert_eq!(
            TaskComplexity::from_model_str("local:llama3"),
            TaskComplexity::Simple
        );
        assert_eq!(
            TaskComplexity::from_model_str("claude-haiku-4-5"),
            TaskComplexity::Agent
        );
        assert_eq!(
            TaskComplexity::from_model_str("claude-sonnet-4"),
            TaskComplexity::Agent
        );
    }

    #[test]
    fn resolve_mlx_model_has_default() {
        std::env::remove_var("CONVERGIO_MLX_MODEL");
        let m = resolve_mlx_model();
        assert!(m.contains("Qwen"), "default should be Qwen: {m}");
    }
}
