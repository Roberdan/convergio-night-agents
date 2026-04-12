//! E2E tests for night agent runs: trigger, list, active, cancel.

mod e2e_helpers;

use axum::http::StatusCode;
use e2e_helpers::*;
use rusqlite::params;
use tower::ServiceExt;

const AGENT_BODY: &str = r#"{
    "name": "run-test-agent",
    "schedule": "0 3 * * *",
    "agent_prompt": "do work"
}"#;

/// Create an agent def and return its id.
async fn create_agent(pool: &convergio_db::pool::ConnPool) -> i64 {
    let app = rebuild(pool);
    let resp = app
        .oneshot(post_json("/api/night-agents", AGENT_BODY))
        .await
        .unwrap();
    body_json(resp).await["id"].as_i64().unwrap()
}

/// Insert a run row directly for deterministic tests.
fn insert_run(pool: &convergio_db::pool::ConnPool, agent_id: i64, status: &str) -> i64 {
    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO night_runs (agent_def_id, status) VALUES (?1, ?2)",
        params![agent_id, status],
    )
    .unwrap();
    conn.last_insert_rowid()
}

// ── TRIGGER ──────────────────────────────────────────────────

#[tokio::test]
async fn trigger_returns_triggered() {
    let (app, pool) = setup();
    let agent_id = create_agent(&pool).await;

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{agent_id}/trigger");
    let resp = app2.oneshot(post_json(&uri, "{}")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "triggered");
    assert_eq!(json["agent_def_id"], agent_id);
    // Don't assert_eq! on run state: `dispatch_single` runs async
    // in background — we only verify the HTTP response is correct.
    let _ = app;
}

// ── LIST RUNS ────────────────────────────────────────────────

#[tokio::test]
async fn list_runs_empty() {
    let (_app, pool) = setup();
    let agent_id = create_agent(&pool).await;

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{agent_id}/runs");
    let resp = app2.oneshot(get_req(&uri)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert!(json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn list_runs_returns_inserted() {
    let (_app, pool) = setup();
    let agent_id = create_agent(&pool).await;

    insert_run(&pool, agent_id, "completed");
    insert_run(&pool, agent_id, "failed");

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{agent_id}/runs");
    let resp = app2.oneshot(get_req(&uri)).await.unwrap();
    let json = body_json(resp).await;
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);
}

#[tokio::test]
async fn list_runs_respects_limit() {
    let (_app, pool) = setup();
    let agent_id = create_agent(&pool).await;

    for _ in 0..5 {
        insert_run(&pool, agent_id, "completed");
    }

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{agent_id}/runs?limit=2");
    let resp = app2.oneshot(get_req(&uri)).await.unwrap();
    let json = body_json(resp).await;
    assert_eq!(json.as_array().unwrap().len(), 2);
}

// ── ACTIVE RUNS ──────────────────────────────────────────────

#[tokio::test]
async fn active_runs_empty() {
    let (app, _pool) = setup();
    let resp = app
        .oneshot(get_req("/api/night-agents/runs/active"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert!(json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn active_runs_only_pending_and_running() {
    let (_app, pool) = setup();
    let agent_id = create_agent(&pool).await;

    insert_run(&pool, agent_id, "pending");
    insert_run(&pool, agent_id, "running");
    insert_run(&pool, agent_id, "completed");

    let app2 = rebuild(&pool);
    let resp = app2
        .oneshot(get_req("/api/night-agents/runs/active"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    let statuses: Vec<&str> = arr.iter().map(|r| r["status"].as_str().unwrap()).collect();
    assert!(statuses.contains(&"pending"));
    assert!(statuses.contains(&"running"));
}

// ── CANCEL RUN ───────────────────────────────────────────────

#[tokio::test]
async fn cancel_pending_run() {
    let (_app, pool) = setup();
    let agent_id = create_agent(&pool).await;
    let run_id = insert_run(&pool, agent_id, "pending");

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{agent_id}/runs/{run_id}/cancel");
    let resp = app2.oneshot(post_json(&uri, "{}")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "cancelled");
}

#[tokio::test]
async fn cancel_running_run() {
    let (_app, pool) = setup();
    let agent_id = create_agent(&pool).await;
    let run_id = insert_run(&pool, agent_id, "running");

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{agent_id}/runs/{run_id}/cancel");
    let resp = app2.oneshot(post_json(&uri, "{}")).await.unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["status"], "cancelled");
}

#[tokio::test]
async fn cancel_completed_run_fails() {
    let (_app, pool) = setup();
    let agent_id = create_agent(&pool).await;
    let run_id = insert_run(&pool, agent_id, "completed");

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{agent_id}/runs/{run_id}/cancel");
    let resp = app2.oneshot(post_json(&uri, "{}")).await.unwrap();
    let json = body_json(resp).await;
    assert!(json["error"].is_string());
}

#[tokio::test]
async fn cancel_nonexistent_run_fails() {
    let (_app, pool) = setup();
    let agent_id = create_agent(&pool).await;

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{agent_id}/runs/9999/cancel");
    let resp = app2.oneshot(post_json(&uri, "{}")).await.unwrap();
    let json = body_json(resp).await;
    assert!(json["error"].is_string());
}
