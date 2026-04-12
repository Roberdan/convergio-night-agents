//! E2E tests for routing stats, set_routing, and migrate_all endpoints.

mod e2e_helpers;

use axum::http::StatusCode;
use e2e_helpers::*;
use tower::ServiceExt;

const AGENT_BODY: &str = r#"{
    "name": "route-test-agent",
    "schedule": "0 3 * * *",
    "agent_prompt": "do stuff"
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

// ── ROUTING STATS ────────────────────────────────────────────

#[tokio::test]
async fn routing_stats_empty() {
    let (app, _pool) = setup();
    let resp = app
        .oneshot(get_req("/api/night-agents/routing/stats"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["definitions"]["total"], 0);
    assert_eq!(json["definitions"]["auto"], 0);
    assert_eq!(json["definitions"]["mlx"], 0);
    assert_eq!(json["definitions"]["cloud"], 0);
    assert!(json["runs_last_7_days"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn routing_stats_counts_defs() {
    let (_app, pool) = setup();
    // Default model is "auto"
    create_agent(&pool).await;

    let app2 = rebuild(&pool);
    let resp = app2
        .oneshot(get_req("/api/night-agents/routing/stats"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["definitions"]["total"], 1);
    assert_eq!(json["definitions"]["auto"], 1);
}

// ── SET ROUTING ──────────────────────────────────────────────

#[tokio::test]
async fn set_routing_to_auto() {
    let (_app, pool) = setup();
    let id = create_agent(&pool).await;

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{id}/routing");
    let resp = app2
        .oneshot(post_json(&uri, r#"{"model":"auto"}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "updated");
    assert_eq!(json["model"], "auto");

    // Verify via GET
    let app3 = rebuild(&pool);
    let get_uri = format!("/api/night-agents/{id}");
    let resp2 = app3.oneshot(get_req(&get_uri)).await.unwrap();
    let def = body_json(resp2).await;
    assert_eq!(def["model"], "auto");
}

#[tokio::test]
async fn set_routing_to_mlx_model() {
    let (_app, pool) = setup();
    let id = create_agent(&pool).await;

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{id}/routing");
    let resp = app2
        .oneshot(post_json(&uri, r#"{"model":"mlx:qwen2.5-coder-32b"}"#))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["status"], "updated");
    assert_eq!(json["model"], "mlx:qwen2.5-coder-32b");
}

#[tokio::test]
async fn set_routing_to_local_model() {
    let (_app, pool) = setup();
    let id = create_agent(&pool).await;

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{id}/routing");
    let resp = app2
        .oneshot(post_json(&uri, r#"{"model":"local:llama3"}"#))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["status"], "updated");
    assert_eq!(json["model"], "local:llama3");
}

#[tokio::test]
async fn set_routing_invalid_model_fails() {
    let (_app, pool) = setup();
    let id = create_agent(&pool).await;

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/{id}/routing");
    let resp = app2
        .oneshot(post_json(&uri, r#"{"model":"gpt-4-turbo"}"#))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert!(json["error"].as_str().unwrap().contains("invalid model"));
}

#[tokio::test]
async fn set_routing_nonexistent_agent_fails() {
    let (app, _pool) = setup();
    let resp = app
        .oneshot(post_json(
            "/api/night-agents/9999/routing",
            r#"{"model":"auto"}"#,
        ))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["error"], "not found");
}

#[tokio::test]
async fn set_routing_cloud_models() {
    let (_app, pool) = setup();
    let id = create_agent(&pool).await;
    let models = ["claude-haiku-4-5", "claude-sonnet-4", "claude-opus-4"];
    for model in &models {
        let app2 = rebuild(&pool);
        let uri = format!("/api/night-agents/{id}/routing");
        let body = format!(r#"{{"model":"{model}"}}"#);
        let resp = app2.oneshot(post_json(&uri, &body)).await.unwrap();
        let json = body_json(resp).await;
        assert_eq!(json["status"], "updated");
        assert_eq!(json["model"], *model);
    }
}

// ── MIGRATE ALL TO AUTO ──────────────────────────────────────

#[tokio::test]
async fn migrate_all_to_auto() {
    let (_app, pool) = setup();
    // Create agents with non-auto models
    let id1 = create_agent(&pool).await;
    let body2 = r#"{
        "name": "agent-2",
        "schedule": "0 4 * * *",
        "agent_prompt": "stuff",
        "model": "claude-sonnet-4"
    }"#;
    let app2 = rebuild(&pool);
    app2.oneshot(post_json("/api/night-agents", body2))
        .await
        .unwrap();

    // Migrate
    let app3 = rebuild(&pool);
    let resp = app3
        .oneshot(post_json("/api/night-agents/routing/migrate-all", "{}"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "migrated");
    assert!(json["updated"].as_i64().unwrap() >= 1);

    // Verify all defs are now "auto"
    let app4 = rebuild(&pool);
    let resp2 = app4
        .oneshot(get_req("/api/night-agents/routing/stats"))
        .await
        .unwrap();
    let stats = body_json(resp2).await;
    let total = stats["definitions"]["total"].as_i64().unwrap();
    let auto = stats["definitions"]["auto"].as_i64().unwrap();
    assert_eq!(total, auto, "all defs should be auto");
    let _ = id1;
}

#[tokio::test]
async fn migrate_all_idempotent() {
    let (_app, pool) = setup();
    // All defs already auto
    let app2 = rebuild(&pool);
    let resp = app2
        .oneshot(post_json("/api/night-agents/routing/migrate-all", "{}"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["status"], "migrated");
    assert_eq!(json["updated"], 0);
}
