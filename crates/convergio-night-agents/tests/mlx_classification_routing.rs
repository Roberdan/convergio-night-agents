// TODO: re-enable when convergio-inference repo is published on GitHub
#![cfg(feature = "inference")]
//! Classification + routing tests: night agent prompts get correct
//! tier assignments and the router selects the right model.

mod routing;

use convergio_inference::classifier;
use convergio_inference::types::*;
use routing::*;

// ================================================================
// 1. CLASSIFICATION — night agent prompts get correct tiers
// ================================================================

#[test]
fn memory_lint_classifies_as_trivial() {
    let req = make_request("Scan memory files for stale entries and duplicates", None);
    assert_eq!(classifier::classify(&req), InferenceTier::T1Trivial);
}

#[test]
fn knowledge_sync_classifies_as_trivial() {
    let req = make_request("List changed files since last scan hash", None);
    assert_eq!(classifier::classify(&req), InferenceTier::T1Trivial);
}

#[test]
fn stale_cleanup_classifies_as_trivial() {
    let req = make_request("Remove completed runs older than 30 days", None);
    assert_eq!(classifier::classify(&req), InferenceTier::T1Trivial);
}

#[test]
fn code_review_classifies_higher() {
    let req = make_request(
        "Review the security of the authentication module, check for \
         injection vulnerabilities, analyze the token validation logic, \
         and verify RBAC enforcement across all API endpoints. \
         This is a critical architecture review.",
        None,
    );
    let tier = classifier::classify(&req);
    assert!(tier >= InferenceTier::T3Complex, "got {tier:?}");
}

#[test]
fn refactoring_classifies_as_complex() {
    let req = make_request(
        "Refactor the entire agent spawner to support multiple backends. \
         This involves redesigning the interface, updating all callers, \
         and ensuring backward compatibility with existing agent defs. \
         The architecture needs careful consideration.",
        None,
    );
    let tier = classifier::classify(&req);
    assert!(tier >= InferenceTier::T3Complex, "got {tier:?}");
}

// ================================================================
// 2. ROUTING — MLX selected for simple, cloud for complex
// ================================================================

#[test]
fn mlx_selected_for_memory_lint() {
    let router = night_router();
    let req = make_request(
        "Scan memory files for stale entries",
        Some(InferenceTier::T1Trivial),
    );
    let (resp, decision) = router.route(&req, false).unwrap();
    assert_eq!(resp.model_used, "qwen2.5-coder-32b-mlx");
    assert_eq!(decision.effective_tier, "t1");
    assert!(decision.fallback_chain.contains(&"claude-haiku-4-5".into()));
}

#[test]
fn mlx_selected_for_knowledge_sync() {
    let router = night_router();
    let req = make_request(
        "Detect language changes in tracked repos",
        Some(InferenceTier::T2Standard),
    );
    let (resp, _) = router.route(&req, false).unwrap();
    assert_eq!(resp.model_used, "qwen2.5-coder-32b-mlx");
}

#[test]
fn cloud_selected_for_code_review() {
    let router = night_router();
    let req = make_request(
        "Deep security architecture review",
        Some(InferenceTier::T3Complex),
    );
    let (resp, decision) = router.route(&req, false).unwrap();
    assert_eq!(resp.model_used, "claude-sonnet-4");
    assert_eq!(decision.effective_tier, "t3");
}

#[test]
fn opus_selected_for_critical_tasks() {
    let router = night_router();
    let req = make_request(
        "Full system architecture redesign",
        Some(InferenceTier::T4Critical),
    );
    let (resp, _) = router.route(&req, false).unwrap();
    assert_eq!(resp.model_used, "claude-opus-4");
}

#[test]
fn mlx_tasks_cost_zero() {
    let router = night_router();
    let req = make_request("simple lint", Some(InferenceTier::T1Trivial));
    let (resp, _) = router.route(&req, false).unwrap();
    assert_eq!(resp.model_used, "qwen2.5-coder-32b-mlx");
    assert_eq!(resp.cost, 0.0, "local MLX inference should be free");
}
