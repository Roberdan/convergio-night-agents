//! Shared test helpers for night-agents E2E tests.
#![allow(dead_code)]

use axum::body::Body;
use axum::http::Request;
use convergio_db::pool::ConnPool;
use convergio_night_agents::routes::{night_agents_routes, NightAgentsState};
use convergio_night_agents::schema;
use std::sync::Arc;

/// Create an in-memory DB with all night-agent migrations applied.
pub fn setup() -> (axum::Router, ConnPool) {
    let pool = convergio_db::pool::create_memory_pool().unwrap();
    let conn = pool.get().unwrap();
    for m in schema::migrations() {
        conn.execute_batch(m.up).unwrap();
    }
    drop(conn);
    let state = Arc::new(NightAgentsState { pool: pool.clone() });
    let app = night_agents_routes(state);
    (app, pool)
}

/// Rebuild a fresh Router from an existing pool (needed because
/// axum `oneshot` consumes the router).
pub fn rebuild(pool: &ConnPool) -> axum::Router {
    let state = Arc::new(NightAgentsState { pool: pool.clone() });
    night_agents_routes(state)
}

/// Extract JSON body from a response.
pub async fn body_json(resp: axum::http::Response<Body>) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Build a POST request with JSON body.
pub fn post_json(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

/// Build a PUT request with JSON body.
pub fn put_json(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

/// Build a GET request.
pub fn get_req(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

/// Build a DELETE request.
pub fn delete_req(uri: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}
