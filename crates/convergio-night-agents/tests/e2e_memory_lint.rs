//! E2E tests for memory lint API endpoints.

mod e2e_helpers;

use axum::http::StatusCode;
use e2e_helpers::*;
use rusqlite::params;
use tower::ServiceExt;

/// Insert a lint finding directly for deterministic tests.
fn insert_finding(
    pool: &convergio_db::pool::ConnPool,
    project: &str,
    category: &str,
    severity: &str,
    rule: &str,
) -> i64 {
    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO memory_lint_results \
         (project_name, file_path, line, category, severity, rule, \
          message, suggestion) \
         VALUES (?1, 'src/lib.rs', 42, ?2, ?3, ?4, 'test msg', 'fix it')",
        params![project, category, severity, rule],
    )
    .unwrap();
    conn.last_insert_rowid()
}

#[tokio::test]
async fn list_findings_empty() {
    let (app, _pool) = setup();
    let resp = app
        .oneshot(get_req("/api/night-agents/memory-lint"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["count"], 0);
    assert!(json["findings"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn list_findings_returns_inserted() {
    let (_app, pool) = setup();
    insert_finding(&pool, "convergio", "stale", "warning", "STALE-001");
    insert_finding(&pool, "convergio", "duplicate", "error", "DUP-001");

    let app2 = rebuild(&pool);
    let resp = app2
        .oneshot(get_req("/api/night-agents/memory-lint"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["count"], 2);
    let findings = json["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 2);
    // Errors sort first
    assert_eq!(findings[0]["severity"], "error");
}

#[tokio::test]
async fn list_findings_filter_by_project() {
    let (_app, pool) = setup();
    insert_finding(&pool, "proj-a", "stale", "warning", "STALE-001");
    insert_finding(&pool, "proj-b", "stale", "warning", "STALE-002");

    let app2 = rebuild(&pool);
    let resp = app2
        .oneshot(get_req("/api/night-agents/memory-lint?project=proj-a"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["count"], 1);
    let f = &json["findings"].as_array().unwrap()[0];
    assert_eq!(f["project_name"], "proj-a");
}

#[tokio::test]
async fn list_findings_filter_by_severity() {
    let (_app, pool) = setup();
    insert_finding(&pool, "proj", "stale", "warning", "S-001");
    insert_finding(&pool, "proj", "duplicate", "error", "D-001");
    insert_finding(&pool, "proj", "alignment", "info", "A-001");

    let app2 = rebuild(&pool);
    let resp = app2
        .oneshot(get_req("/api/night-agents/memory-lint?severity=error"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["count"], 1);
}

#[tokio::test]
async fn list_findings_filter_by_category() {
    let (_app, pool) = setup();
    insert_finding(&pool, "proj", "stale", "warning", "S-001");
    insert_finding(&pool, "proj", "duplicate", "error", "D-001");

    let app2 = rebuild(&pool);
    let resp = app2
        .oneshot(get_req("/api/night-agents/memory-lint?category=stale"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["count"], 1);
    assert_eq!(json["findings"].as_array().unwrap()[0]["category"], "stale");
}

#[tokio::test]
async fn list_findings_respects_limit() {
    let (_app, pool) = setup();
    for i in 0..5 {
        insert_finding(&pool, "proj", "stale", "warning", &format!("R-{i:03}"));
    }
    let app2 = rebuild(&pool);
    let resp = app2
        .oneshot(get_req("/api/night-agents/memory-lint?limit=2"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["count"], 2);
}

#[tokio::test]
async fn list_findings_excludes_dismissed() {
    let (_app, pool) = setup();
    let id = insert_finding(&pool, "proj", "stale", "warning", "S-001");
    insert_finding(&pool, "proj", "duplicate", "error", "D-001");
    // Dismiss the first
    let conn = pool.get().unwrap();
    conn.execute(
        "UPDATE memory_lint_results SET dismissed = 1 WHERE id = ?1",
        params![id],
    )
    .unwrap();
    drop(conn);

    let app2 = rebuild(&pool);
    let resp = app2
        .oneshot(get_req("/api/night-agents/memory-lint"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["count"], 1);
}

#[tokio::test]
async fn lint_summary_empty() {
    let (app, _pool) = setup();
    let resp = app
        .oneshot(get_req("/api/night-agents/memory-lint/summary"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert!(json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn lint_summary_aggregates_by_project() {
    let (_app, pool) = setup();
    insert_finding(&pool, "proj-a", "stale", "warning", "S-001");
    insert_finding(&pool, "proj-a", "duplicate", "error", "D-001");
    insert_finding(&pool, "proj-b", "contradiction", "info", "C-001");

    let app2 = rebuild(&pool);
    let resp = app2
        .oneshot(get_req("/api/night-agents/memory-lint/summary"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    // proj-a has 2 findings, so it appears first (ORDER BY total DESC)
    assert_eq!(arr[0]["project_name"], "proj-a");
    assert_eq!(arr[0]["total"], 2);
    assert_eq!(arr[0]["errors"], 1);
    assert_eq!(arr[0]["warnings"], 1);
    assert_eq!(arr[1]["project_name"], "proj-b");
    assert_eq!(arr[1]["total"], 1);
}

#[tokio::test]
async fn trigger_lint_returns_triggered() {
    let (app, _pool) = setup();
    let resp = app
        .oneshot(post_json("/api/night-agents/memory-lint/trigger", "{}"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "triggered");
}

#[tokio::test]
async fn trigger_project_lint_returns_triggered() {
    let (app, pool) = setup();
    // Need a project first
    let resp = app
        .oneshot(post_json(
            "/api/night-agents/projects",
            r#"{"name":"lint-proj","repo_path":"/dev/null"}"#,
        ))
        .await
        .unwrap();
    let id = body_json(resp).await["id"].as_i64().unwrap();

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/projects/{id}/memory-lint");
    let resp2 = app2.oneshot(post_json(&uri, "{}")).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let json = body_json(resp2).await;
    assert_eq!(json["status"], "triggered");
    assert_eq!(json["project_id"], id);
}

#[tokio::test]
async fn dismiss_finding_works() {
    let (_app, pool) = setup();
    let id = insert_finding(&pool, "proj", "stale", "warning", "S-001");

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/memory-lint/{id}/dismiss");
    let resp = app2.oneshot(post_json(&uri, "{}")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "dismissed");
}

#[tokio::test]
async fn dismiss_nonexistent_finding_fails() {
    let (app, _pool) = setup();
    let resp = app
        .oneshot(post_json(
            "/api/night-agents/memory-lint/9999/dismiss",
            "{}",
        ))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["error"], "not found");
}
