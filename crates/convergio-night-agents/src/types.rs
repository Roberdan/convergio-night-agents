//! Data types for night agent definitions, runs, and tracked projects.

use serde::{Deserialize, Serialize};

/// Persistent definition of a night agent — what it does, when, how.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NightAgentDef {
    pub id: i64,
    pub name: String,
    pub org_id: Option<String>,
    pub description: Option<String>,
    pub schedule: String,
    pub agent_prompt: String,
    pub model: String,
    pub enabled: bool,
    pub max_runtime_secs: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// A single execution of a night agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NightRun {
    pub id: i64,
    pub agent_def_id: i64,
    pub status: RunStatus,
    pub node_name: Option<String>,
    pub pid: Option<i64>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub outcome: Option<String>,
    pub error_message: Option<String>,
    pub tokens_used: i64,
    pub cost_usd: f64,
    pub worktree_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl std::fmt::Display for RunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A project tracked for automatic knowledge sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedProject {
    pub id: i64,
    pub name: String,
    pub repo_path: String,
    pub remote_url: Option<String>,
    pub last_scan_at: Option<String>,
    pub last_scan_hash: Option<String>,
    pub scan_profile_json: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

/// Request body for creating a night agent definition.
#[derive(Debug, Deserialize)]
pub struct CreateAgentBody {
    pub name: String,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub schedule: String,
    pub agent_prompt: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_runtime")]
    pub max_runtime_secs: i64,
}

fn default_model() -> String {
    "auto".to_string()
}

fn default_max_runtime() -> i64 {
    3600
}

/// Request body for creating a tracked project.
#[derive(Debug, Deserialize)]
pub struct CreateProjectBody {
    pub name: String,
    pub repo_path: String,
    #[serde(default)]
    pub remote_url: Option<String>,
}

/// Pagination query params.
#[derive(Debug, Deserialize, Default)]
pub struct ListQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
