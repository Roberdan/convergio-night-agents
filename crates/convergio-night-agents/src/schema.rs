//! DB migrations for night agent tables.

use convergio_types::extension::Migration;

pub fn migrations() -> Vec<Migration> {
    vec![Migration {
        version: 1,
        description: "night agent tables",
        up: "\
CREATE TABLE IF NOT EXISTS night_agent_defs (\
    id INTEGER PRIMARY KEY,\
    name TEXT NOT NULL UNIQUE,\
    org_id TEXT,\
    description TEXT,\
    schedule TEXT NOT NULL,\
    agent_prompt TEXT NOT NULL,\
    model TEXT DEFAULT 'claude-haiku-4-5',\
    enabled INTEGER DEFAULT 1,\
    max_runtime_secs INTEGER DEFAULT 3600,\
    created_at TEXT DEFAULT (datetime('now')),\
    updated_at TEXT DEFAULT (datetime('now'))\
);\
CREATE INDEX IF NOT EXISTS idx_nad_enabled \
    ON night_agent_defs(enabled);\
CREATE TABLE IF NOT EXISTS night_runs (\
    id INTEGER PRIMARY KEY,\
    agent_def_id INTEGER NOT NULL \
        REFERENCES night_agent_defs(id),\
    status TEXT DEFAULT 'pending',\
    node_name TEXT,\
    pid INTEGER,\
    started_at TEXT,\
    completed_at TEXT,\
    outcome TEXT,\
    error_message TEXT,\
    tokens_used INTEGER DEFAULT 0,\
    cost_usd REAL DEFAULT 0.0,\
    worktree_path TEXT\
);\
CREATE INDEX IF NOT EXISTS idx_nr_agent \
    ON night_runs(agent_def_id);\
CREATE INDEX IF NOT EXISTS idx_nr_status \
    ON night_runs(status);\
CREATE TABLE IF NOT EXISTS tracked_projects (\
    id INTEGER PRIMARY KEY,\
    name TEXT NOT NULL UNIQUE,\
    repo_path TEXT NOT NULL,\
    remote_url TEXT,\
    last_scan_at TEXT,\
    last_scan_hash TEXT,\
    scan_profile_json TEXT,\
    enabled INTEGER DEFAULT 1,\
    created_at TEXT DEFAULT (datetime('now'))\
);\
CREATE INDEX IF NOT EXISTS idx_tp_enabled \
    ON tracked_projects(enabled);\
CREATE TABLE IF NOT EXISTS memory_lint_results (\
    id INTEGER PRIMARY KEY,\
    project_name TEXT NOT NULL,\
    file_path TEXT NOT NULL,\
    line INTEGER,\
    category TEXT NOT NULL,\
    severity TEXT NOT NULL,\
    rule TEXT NOT NULL,\
    message TEXT NOT NULL,\
    suggestion TEXT,\
    dismissed INTEGER DEFAULT 0,\
    run_at TEXT DEFAULT (datetime('now'))\
);\
CREATE INDEX IF NOT EXISTS idx_mlr_project \
    ON memory_lint_results(project_name);\
CREATE INDEX IF NOT EXISTS idx_mlr_severity \
    ON memory_lint_results(severity);\
CREATE INDEX IF NOT EXISTS idx_mlr_dismissed \
    ON memory_lint_results(dismissed);",
    }]
}
