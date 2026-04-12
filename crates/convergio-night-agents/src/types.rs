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

/// Maximum allowed lengths for user-supplied strings.
const MAX_NAME_LEN: usize = 128;
const MAX_PROMPT_LEN: usize = 32_000;
const MAX_PATH_LEN: usize = 1024;
const MAX_RUNTIME_SECS: i64 = 86_400; // 24h

impl CreateAgentBody {
    /// Validate user input. Returns `Err(reason)` on violation.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > MAX_NAME_LEN {
            return Err(format!("name must be 1–{MAX_NAME_LEN} chars"));
        }
        if !self
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ')
        {
            return Err("name may only contain alphanumeric, dash, underscore, space".into());
        }
        validate_cron(&self.schedule)?;
        if self.agent_prompt.is_empty() || self.agent_prompt.len() > MAX_PROMPT_LEN {
            return Err(format!("agent_prompt must be 1–{MAX_PROMPT_LEN} chars"));
        }
        validate_model(&self.model)?;
        if self.max_runtime_secs <= 0 || self.max_runtime_secs > MAX_RUNTIME_SECS {
            return Err(format!("max_runtime_secs must be 1–{MAX_RUNTIME_SECS}"));
        }
        Ok(())
    }
}

/// Validate a cron expression (5 space-separated fields, safe chars).
pub fn validate_cron(cron: &str) -> Result<(), String> {
    let parts: Vec<&str> = cron.split_whitespace().collect();
    if parts.len() != 5 {
        return Err("schedule must have exactly 5 cron fields".into());
    }
    for part in &parts {
        if !part
            .chars()
            .all(|c| c.is_ascii_digit() || c == '*' || c == '/' || c == '-' || c == ',')
        {
            return Err(format!("invalid cron field: {part}"));
        }
    }
    Ok(())
}

/// Validate model string against the allowlist.
pub fn validate_model(model: &str) -> Result<(), String> {
    let allowed_static = [
        "auto",
        "claude-haiku-4-5",
        "claude-sonnet-4",
        "claude-sonnet-4-5",
        "claude-opus-4",
    ];
    if allowed_static.contains(&model) || model.starts_with("mlx:") || model.starts_with("local:") {
        // Prefixed models: only allow safe chars after the prefix
        if let Some(suffix) = model
            .strip_prefix("mlx:")
            .or_else(|| model.strip_prefix("local:"))
        {
            if !suffix
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.')
            {
                return Err(format!("invalid model suffix: {suffix}"));
            }
        }
        Ok(())
    } else {
        Err(format!("invalid model: {model}"))
    }
}

/// Request body for creating a tracked project.
#[derive(Debug, Deserialize)]
pub struct CreateProjectBody {
    pub name: String,
    pub repo_path: String,
    #[serde(default)]
    pub remote_url: Option<String>,
}

impl CreateProjectBody {
    /// Validate user input. Returns `Err(reason)` on violation.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > MAX_NAME_LEN {
            return Err(format!("name must be 1–{MAX_NAME_LEN} chars"));
        }
        if self.repo_path.is_empty() || self.repo_path.len() > MAX_PATH_LEN {
            return Err(format!("repo_path must be 1–{MAX_PATH_LEN} chars"));
        }
        // Path traversal: reject relative segments and null bytes
        if self.repo_path.contains("..") || self.repo_path.contains('\0') {
            return Err("repo_path must not contain '..' or null bytes".into());
        }
        // Must be absolute
        if !self.repo_path.starts_with('/') {
            return Err("repo_path must be an absolute path".into());
        }
        Ok(())
    }
}

/// Pagination query params.
#[derive(Debug, Deserialize, Default)]
pub struct ListQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
