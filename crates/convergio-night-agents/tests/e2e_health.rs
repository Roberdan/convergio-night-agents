//! E2E tests for NightAgentsExtension health checks and ci-optimizer.

use convergio_db::pool::ConnPool;
use convergio_night_agents::ci_optimizer;
use convergio_night_agents::ext::NightAgentsExtension;
use convergio_night_agents::schema;
use convergio_types::extension::{Extension, Health};
use rusqlite::params;

/// Create an in-memory DB with all night-agent migrations applied.
fn setup_pool() -> ConnPool {
    let pool = convergio_db::pool::create_memory_pool().unwrap();
    let conn = pool.get().unwrap();
    for m in schema::migrations() {
        conn.execute_batch(m.up).unwrap();
    }
    drop(conn);
    pool
}

// ── HEALTH CHECKS ────────────────────────────────────────────

#[test]
fn health_ok_with_clean_state() {
    let pool = setup_pool();
    let ext = NightAgentsExtension::new(pool);
    assert!(matches!(ext.health(), Health::Ok));
}

#[test]
fn health_ok_with_agent_defs() {
    let pool = setup_pool();
    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO night_agent_defs (name, schedule, agent_prompt) \
         VALUES ('test', '0 * * * *', 'noop')",
        [],
    )
    .unwrap();
    drop(conn);
    let ext = NightAgentsExtension::new(pool);
    assert!(matches!(ext.health(), Health::Ok));
}

#[test]
fn health_degraded_with_stuck_runs() {
    let pool = setup_pool();
    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO night_agent_defs (name, schedule, agent_prompt) \
         VALUES ('stuck', '0 * * * *', 'noop')",
        [],
    )
    .unwrap();
    let agent_id = conn.last_insert_rowid();
    // Insert a run stuck > 2 hours
    conn.execute(
        "INSERT INTO night_runs (agent_def_id, status, started_at) \
         VALUES (?1, 'running', datetime('now', '-3 hours'))",
        params![agent_id],
    )
    .unwrap();
    drop(conn);

    let ext = NightAgentsExtension::new(pool);
    match ext.health() {
        Health::Degraded { reason } => {
            assert!(reason.contains("stuck"), "reason: {reason}");
        }
        other => panic!("expected Degraded, got {other:?}"),
    }
}

#[test]
fn health_ok_with_recent_running() {
    let pool = setup_pool();
    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO night_agent_defs (name, schedule, agent_prompt) \
         VALUES ('recent', '0 * * * *', 'noop')",
        [],
    )
    .unwrap();
    let agent_id = conn.last_insert_rowid();
    // Running for < 2 hours — not stuck
    conn.execute(
        "INSERT INTO night_runs (agent_def_id, status, started_at) \
         VALUES (?1, 'running', datetime('now', '-30 minutes'))",
        params![agent_id],
    )
    .unwrap();
    drop(conn);

    let ext = NightAgentsExtension::new(pool);
    assert!(matches!(ext.health(), Health::Ok));
}

#[test]
fn metrics_returns_expected_keys() {
    let pool = setup_pool();
    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO night_agent_defs (name, schedule, agent_prompt) \
         VALUES ('m1', '0 * * * *', 'noop')",
        [],
    )
    .unwrap();
    drop(conn);

    let ext = NightAgentsExtension::new(pool);
    let metrics = ext.metrics();
    let names: Vec<&str> = metrics.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"night_agents.defs.total"));
    assert!(names.contains(&"night_agents.defs.enabled"));
    assert!(names.contains(&"night_agents.runs.active"));
    assert!(names.contains(&"night_agents.runs.completed_24h"));
    assert!(names.contains(&"night_agents.runs.failed_24h"));
    assert!(names.contains(&"night_agents.projects.tracked"));
}

#[test]
fn manifest_has_expected_capabilities() {
    let pool = setup_pool();
    let ext = NightAgentsExtension::new(pool);
    let m = ext.manifest();
    assert_eq!(m.id, "convergio-night-agents");
    let cap_names: Vec<&str> = m.provides.iter().map(|c| c.name.as_str()).collect();
    assert!(cap_names.contains(&"night-agent-orchestration"));
    assert!(cap_names.contains(&"knowledge-sync"));
    assert!(cap_names.contains(&"project-tracking"));
}

// ── CI OPTIMIZER ─────────────────────────────────────────────

#[test]
fn ci_optimizer_no_workflows_dir() {
    let report = ci_optimizer::analyze_ci("/nonexistent/path", "test-proj");
    assert_eq!(report.project, "test-proj");
    assert_eq!(report.workflows_analyzed, 0);
    assert!(report.findings.is_empty());
    assert!(report.summary.contains("No .github/workflows"));
}

#[test]
fn ci_optimizer_report_to_json() {
    let report = ci_optimizer::CiOptReport {
        project: "demo".into(),
        workflows_analyzed: 2,
        findings: vec![ci_optimizer::CiFinding {
            file: "ci.yml".into(),
            category: "missing-cache".into(),
            severity: "warning".into(),
            message: "No cache step".into(),
            suggestion: "Add actions/cache".into(),
        }],
        summary: "Analyzed 2 workflows — 1 missing-cache".into(),
    };
    let json = ci_optimizer::report_to_json(&report);
    assert_eq!(json["project"], "demo");
    assert_eq!(json["workflows_analyzed"], 2);
    assert_eq!(json["findings_count"], 1);
    assert_eq!(json["findings"][0]["category"], "missing-cache");
}
