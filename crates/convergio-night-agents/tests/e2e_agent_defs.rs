//! E2E tests for night agent definition CRUD lifecycle.

mod e2e_helpers;

use axum::http::StatusCode;
use e2e_helpers::*;
use tower::ServiceExt;

const AGENT_BODY: &str = r#"{
    "name": "nightly-lint",
    "org_id": "convergio",
    "description": "Lint memory files",
    "schedule": "0 3 * * *",
    "agent_prompt": "Run memory lint on all projects",
    "model": "claude-haiku-4-5",
    "max_runtime_secs": 1800
}"#;

// ── CREATE ───────────────────────────────────────────────────

#[tokio::test]
async fn create_agent_def() {
    let (app, _pool) = setup();
    let req = post_json("/api/night-agents", AGENT_BODY);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "created");
    assert!(json["id"].is_number());
}

#[tokio::test]
async fn create_with_defaults() {
    let (app, _pool) = setup();
    let body = r#"{
        "name": "simple-agent",
        "schedule": "0 * * * *",
        "agent_prompt": "do stuff"
    }"#;
    let req = post_json("/api/night-agents", body);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "created");
}

// ── READ (single + list) ────────────────────────────────────

#[tokio::test]
async fn get_agent_def_by_id() {
    let (app, pool) = setup();

    // Create
    let resp = app
        .oneshot(post_json("/api/night-agents", AGENT_BODY))
        .await
        .unwrap();
    let json = body_json(resp).await;
    let id = json["id"].as_i64().unwrap();

    // Get
    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{id}");
    let resp2 = app2.oneshot(get_req(&uri)).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let def = body_json(resp2).await;
    assert_eq!(def["id"], id);
    assert_eq!(def["name"], "nightly-lint");
    assert_eq!(def["org_id"], "convergio");
    assert_eq!(def["schedule"], "0 3 * * *");
    assert_eq!(def["model"], "claude-haiku-4-5");
    assert_eq!(def["enabled"], true);
    assert_eq!(def["max_runtime_secs"], 1800);
}

#[tokio::test]
async fn list_agent_defs_empty() {
    let (app, _pool) = setup();
    let resp = app.oneshot(get_req("/api/night-agents")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    let arr = json.as_array().unwrap();
    assert!(arr.is_empty());
}

#[tokio::test]
async fn list_agent_defs_returns_created() {
    let (app, pool) = setup();

    // Create two
    app.oneshot(post_json("/api/night-agents", AGENT_BODY))
        .await
        .unwrap();
    let app2 = rebuild(&pool);
    let body2 = r#"{
        "name": "backup-agent",
        "schedule": "0 2 * * *",
        "agent_prompt": "backup db"
    }"#;
    app2.oneshot(post_json("/api/night-agents", body2))
        .await
        .unwrap();

    // List
    let app3 = rebuild(&pool);
    let resp = app3.oneshot(get_req("/api/night-agents")).await.unwrap();
    let json = body_json(resp).await;
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);
}

// ── UPDATE ───────────────────────────────────────────────────

#[tokio::test]
async fn update_agent_def() {
    let (app, pool) = setup();

    // Create
    let resp = app
        .oneshot(post_json("/api/night-agents", AGENT_BODY))
        .await
        .unwrap();
    let id = body_json(resp).await["id"].as_i64().unwrap();

    // Update
    let updated = r#"{
        "name": "nightly-lint-v2",
        "org_id": "convergio",
        "description": "Lint v2",
        "schedule": "0 4 * * *",
        "agent_prompt": "Run lint v2",
        "model": "claude-sonnet-4-5",
        "max_runtime_secs": 900
    }"#;
    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{id}");
    let resp2 = app2.oneshot(put_json(&uri, updated)).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let json = body_json(resp2).await;
    assert_eq!(json["status"], "updated");

    // Verify
    let app3 = rebuild(&pool);
    let resp3 = app3.oneshot(get_req(&uri)).await.unwrap();
    let def = body_json(resp3).await;
    assert_eq!(def["name"], "nightly-lint-v2");
    assert_eq!(def["schedule"], "0 4 * * *");
    assert_eq!(def["model"], "claude-sonnet-4-5");
    assert_eq!(def["max_runtime_secs"], 900);
}

#[tokio::test]
async fn update_nonexistent_returns_error() {
    let (app, _pool) = setup();
    let body = r#"{
        "name": "x",
        "schedule": "0 0 * * *",
        "agent_prompt": "noop"
    }"#;
    let resp = app
        .oneshot(put_json("/api/night-agents/9999", body))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["error"], "not found");
}

// ── DELETE (soft-disable) ────────────────────────────────────

#[tokio::test]
async fn delete_disables_agent_def() {
    let (app, pool) = setup();

    // Create
    let resp = app
        .oneshot(post_json("/api/night-agents", AGENT_BODY))
        .await
        .unwrap();
    let id = body_json(resp).await["id"].as_i64().unwrap();

    // Delete (soft)
    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{id}");
    let resp2 = app2.oneshot(delete_req(&uri)).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let json = body_json(resp2).await;
    assert_eq!(json["status"], "disabled");

    // Verify disabled
    let app3 = rebuild(&pool);
    let resp3 = app3.oneshot(get_req(&uri)).await.unwrap();
    let def = body_json(resp3).await;
    assert_eq!(def["enabled"], false);
}

#[tokio::test]
async fn delete_nonexistent_returns_error() {
    let (app, _pool) = setup();
    let resp = app
        .oneshot(delete_req("/api/night-agents/9999"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["error"], "not found");
}
