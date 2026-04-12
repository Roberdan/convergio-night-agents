//! Extension trait implementation for convergio-night-agents.

use std::sync::Arc;

use convergio_db::pool::ConnPool;
use convergio_types::extension::{
    AppContext, Extension, Health, McpToolDef, Metric, ScheduledTask,
};
use convergio_types::manifest::{Capability, Dependency, Manifest, ModuleKind};

use crate::routes::{night_agents_routes, NightAgentsState};

pub struct NightAgentsExtension {
    pool: ConnPool,
}

impl NightAgentsExtension {
    pub fn new(pool: ConnPool) -> Self {
        Self { pool }
    }

    fn state(&self) -> Arc<NightAgentsState> {
        Arc::new(NightAgentsState {
            pool: self.pool.clone(),
        })
    }
}

impl Extension for NightAgentsExtension {
    fn manifest(&self) -> Manifest {
        Manifest {
            id: "convergio-night-agents".to_string(),
            description: "Night agent orchestration — scheduled agents, \
                          knowledge sync, project tracking"
                .to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: ModuleKind::Platform,
            provides: vec![
                Capability {
                    name: "night-agent-orchestration".to_string(),
                    version: "1.0.0".to_string(),
                    description: "Scheduled night agent lifecycle".into(),
                },
                Capability {
                    name: "knowledge-sync".to_string(),
                    version: "1.0.0".to_string(),
                    description: "Auto-update knowledge base from repos".into(),
                },
                Capability {
                    name: "project-tracking".to_string(),
                    version: "1.0.0".to_string(),
                    description: "Track repos for change detection".into(),
                },
            ],
            requires: vec![Dependency {
                capability: "db-pool".to_string(),
                version_req: ">=1.0.0".to_string(),
                required: true,
            }],
            agent_tools: vec![],
            required_roles: vec!["nightagent".into(), "orchestrator".into(), "all".into()],
        }
    }

    fn routes(&self, _ctx: &AppContext) -> Option<axum::Router> {
        let router = night_agents_routes(self.state())
            .merge(crate::drift_detection::drift_routes(self.state()));
        Some(router)
    }

    fn migrations(&self) -> Vec<convergio_types::extension::Migration> {
        crate::schema::migrations()
    }

    fn health(&self) -> Health {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(e) => {
                return Health::Down {
                    reason: format!("pool: {e}"),
                }
            }
        };
        // Check tables accessible
        let ok = conn
            .query_row("SELECT COUNT(*) FROM night_agent_defs", [], |r| {
                r.get::<_, i64>(0)
            })
            .is_ok();
        if !ok {
            return Health::Degraded {
                reason: "night_agent_defs table inaccessible".into(),
            };
        }
        // Check for stuck runs
        let stuck: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM night_runs \
                 WHERE status = 'running' \
                 AND started_at < datetime('now', '-2 hours')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if stuck > 0 {
            return Health::Degraded {
                reason: format!("{stuck} runs stuck > 2h"),
            };
        }
        Health::Ok
    }

    fn metrics(&self) -> Vec<Metric> {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut out = Vec::new();
        let query = |sql: &str| -> f64 {
            conn.query_row(sql, [], |r| r.get::<_, f64>(0))
                .unwrap_or(0.0)
        };
        out.push(Metric {
            name: "night_agents.defs.total".into(),
            value: query("SELECT COUNT(*) FROM night_agent_defs"),
            labels: vec![],
        });
        out.push(Metric {
            name: "night_agents.defs.enabled".into(),
            value: query("SELECT COUNT(*) FROM night_agent_defs WHERE enabled = 1"),
            labels: vec![],
        });
        out.push(Metric {
            name: "night_agents.runs.active".into(),
            value: query("SELECT COUNT(*) FROM night_runs WHERE status = 'running'"),
            labels: vec![],
        });
        out.push(Metric {
            name: "night_agents.runs.completed_24h".into(),
            value: query(
                "SELECT COUNT(*) FROM night_runs \
                 WHERE status = 'completed' \
                 AND completed_at > datetime('now', '-1 day')",
            ),
            labels: vec![],
        });
        out.push(Metric {
            name: "night_agents.runs.failed_24h".into(),
            value: query(
                "SELECT COUNT(*) FROM night_runs \
                 WHERE status = 'failed' \
                 AND completed_at > datetime('now', '-1 day')",
            ),
            labels: vec![],
        });
        out.push(Metric {
            name: "night_agents.projects.tracked".into(),
            value: query("SELECT COUNT(*) FROM tracked_projects WHERE enabled = 1"),
            labels: vec![],
        });
        out
    }

    fn scheduled_tasks(&self) -> Vec<ScheduledTask> {
        vec![
            ScheduledTask {
                name: "night-dispatch",
                cron: "* * * * *",
            },
            ScheduledTask {
                name: "night-reaper",
                cron: "0 7 * * *",
            },
            ScheduledTask {
                name: "knowledge-sync",
                cron: "0 0 * * *",
            },
            ScheduledTask {
                name: "memory-lint",
                cron: "0 3 * * *",
            },
        ]
    }

    fn on_scheduled_task(&self, task_name: &str) {
        let pool = self.pool.clone();
        match task_name {
            "night-dispatch" => {
                tokio::spawn(async move {
                    crate::runner::dispatch_all(&pool).await;
                });
            }
            "night-reaper" => {
                tokio::spawn(async move {
                    crate::runner::reap_stale(&pool);
                });
            }
            "knowledge-sync" => {
                tokio::spawn(async move {
                    crate::knowledge::sync_all_projects(&pool);
                });
            }
            "memory-lint" => {
                tokio::spawn(async move {
                    crate::memory_lint::lint_all_projects(&pool);
                });
            }
            _ => {}
        }
    }

    fn mcp_tools(&self) -> Vec<McpToolDef> {
        crate::mcp_defs::night_agent_tools()
    }
}
