//! E2E tests for tracked projects: CRUD lifecycle and scan trigger.

mod e2e_helpers;

use axum::http::StatusCode;
use e2e_helpers::*;
use tower::ServiceExt;

const PROJECT_BODY: &str = r#"{
    "name": "convergio",
    "repo_path": "/Users/test/GitHub/convergio",
    "remote_url": "https://github.com/Roberdan/convergio"
}"#;

// ── CREATE PROJECT ───────────────────────────────────────────

#[tokio::test]
async fn create_project() {
    let (app, _pool) = setup();
    let req = post_json("/api/night-agents/projects", PROJECT_BODY);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "created");
    assert!(json["id"].is_number());
}

#[tokio::test]
async fn create_project_without_remote() {
    let (app, _pool) = setup();
    let body = r#"{
        "name": "local-proj",
        "repo_path": "/home/dev/local-proj"
    }"#;
    let resp = app
        .oneshot(post_json("/api/night-agents/projects", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "created");
}

#[tokio::test]
async fn create_duplicate_name_fails() {
    let (app, pool) = setup();
    app.oneshot(post_json("/api/night-agents/projects", PROJECT_BODY))
        .await
        .unwrap();

    let app2 = rebuild(&pool);
    let resp = app2
        .oneshot(post_json("/api/night-agents/projects", PROJECT_BODY))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert!(json["error"].is_string());
}

// ── LIST PROJECTS ────────────────────────────────────────────

#[tokio::test]
async fn list_projects_empty() {
    let (app, _pool) = setup();
    let resp = app
        .oneshot(get_req("/api/night-agents/projects"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert!(json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn list_projects_returns_created() {
    let (app, pool) = setup();
    app.oneshot(post_json("/api/night-agents/projects", PROJECT_BODY))
        .await
        .unwrap();

    let body2 = r#"{
        "name": "other-proj",
        "repo_path": "/home/dev/other"
    }"#;
    let app2 = rebuild(&pool);
    app2.oneshot(post_json("/api/night-agents/projects", body2))
        .await
        .unwrap();

    let app3 = rebuild(&pool);
    let resp = app3
        .oneshot(get_req("/api/night-agents/projects"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);

    // Ordered by name
    assert_eq!(arr[0]["name"], "convergio");
    assert_eq!(arr[1]["name"], "other-proj");
}

#[tokio::test]
async fn list_projects_contains_expected_fields() {
    let (app, pool) = setup();
    app.oneshot(post_json("/api/night-agents/projects", PROJECT_BODY))
        .await
        .unwrap();

    let app2 = rebuild(&pool);
    let resp = app2
        .oneshot(get_req("/api/night-agents/projects"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    let proj = &json.as_array().unwrap()[0];
    assert!(proj["id"].is_number());
    assert_eq!(proj["name"], "convergio");
    assert_eq!(proj["repo_path"], "/Users/test/GitHub/convergio");
    assert_eq!(proj["remote_url"], "https://github.com/Roberdan/convergio");
    assert_eq!(proj["enabled"], true);
    assert!(proj["created_at"].is_string());
}

// ── DELETE PROJECT (soft-disable) ────────────────────────────

#[tokio::test]
async fn delete_project_disables() {
    let (app, pool) = setup();
    let resp = app
        .oneshot(post_json("/api/night-agents/projects", PROJECT_BODY))
        .await
        .unwrap();
    let id = body_json(resp).await["id"].as_i64().unwrap();

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/projects/{id}");
    let resp2 = app2.oneshot(delete_req(&uri)).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let json = body_json(resp2).await;
    assert_eq!(json["status"], "disabled");
}

#[tokio::test]
async fn delete_nonexistent_project_fails() {
    let (app, _pool) = setup();
    let resp = app
        .oneshot(delete_req("/api/night-agents/projects/9999"))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["error"], "not found");
}

// ── SCAN PROJECT ─────────────────────────────────────────────

#[tokio::test]
async fn scan_project_returns_triggered() {
    let (app, pool) = setup();
    let resp = app
        .oneshot(post_json("/api/night-agents/projects", PROJECT_BODY))
        .await
        .unwrap();
    let id = body_json(resp).await["id"].as_i64().unwrap();

    let app2 = rebuild(&pool);
    let uri = format!("/api/night-agents/projects/{id}/scan");
    let resp2 = app2.oneshot(post_json(&uri, "{}")).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let json = body_json(resp2).await;
    assert_eq!(json["status"], "scan_triggered");
    assert_eq!(json["project_id"], id);
}
