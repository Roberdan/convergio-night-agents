// TODO: re-enable when convergio-inference repo is published on GitHub
#![cfg(feature = "inference")]
//! Fallback, budget, and full nightly cycle simulation tests.

mod routing;

use convergio_inference::types::*;
use routing::*;

// ================================================================
// 1. FALLBACK — MLX down → gracefully escalate to cloud
// ================================================================

#[test]
fn mlx_down_falls_back_to_haiku() {
    let mut router = night_router();
    router.set_health("qwen2.5-coder-32b-mlx", false);

    let req = make_request("lint memory files", Some(InferenceTier::T1Trivial));
    let (resp, _) = router.route(&req, false).unwrap();
    assert_eq!(resp.model_used, "claude-haiku-4-5");
}

#[test]
fn all_simple_models_down_errors_gracefully() {
    let mut router = night_router();
    router.set_health("qwen2.5-coder-32b-mlx", false);
    router.set_health("claude-haiku-4-5", false);

    let req = make_request("lint memory files", Some(InferenceTier::T1Trivial));
    assert!(router.route(&req, false).is_err());
}

// ================================================================
// 2. BUDGET PRESSURE — downgrade complex to cheaper models
// ================================================================

#[test]
fn budget_pressure_downgrades_complex_to_standard() {
    let router = night_router();
    let req = make_request("code review task", Some(InferenceTier::T3Complex));

    let (resp_normal, _) = router.route(&req, false).unwrap();
    assert_eq!(resp_normal.model_used, "claude-sonnet-4");

    // Budget pressure: T3 → T2 → MLX (cheapest)
    let (resp_budget, decision) = router.route(&req, true).unwrap();
    assert_eq!(resp_budget.model_used, "qwen2.5-coder-32b-mlx");
    assert_eq!(decision.effective_tier, "t2");
}

// ================================================================
// 3. FULL NIGHTLY CYCLE — end-to-end simulation
// ================================================================

#[test]
fn nightly_cycle_routes_correctly() {
    let router = night_router();

    let tasks: Vec<(&str, &str, InferenceTier)> = vec![
        (
            "memory-lint",
            "Scan and lint memory files",
            InferenceTier::T1Trivial,
        ),
        (
            "knowledge-sync",
            "Detect repo changes",
            InferenceTier::T1Trivial,
        ),
        (
            "stale-reaper",
            "Clean up old runs",
            InferenceTier::T1Trivial,
        ),
        (
            "code-review",
            "Review PR security",
            InferenceTier::T3Complex,
        ),
        (
            "arch-audit",
            "Architecture assessment",
            InferenceTier::T4Critical,
        ),
    ];

    let mut local_count = 0;
    let mut cloud_count = 0;

    for (name, prompt, tier) in &tasks {
        let req = make_request(prompt, Some(tier.clone()));
        let (resp, _) = router.route(&req, false).unwrap();
        if resp.model_used.contains("mlx") {
            local_count += 1;
        } else {
            cloud_count += 1;
        }
        eprintln!(
            "  {name}: {model} (tier={t})",
            model = resp.model_used,
            t = tier.label()
        );
    }

    assert_eq!(local_count, 3, "3 simple tasks should use MLX");
    assert_eq!(cloud_count, 2, "2 complex tasks should use cloud");
}

#[test]
fn mixed_health_nightly_cycle() {
    let mut router = night_router();
    router.set_health("qwen2.5-coder-32b-mlx", false);

    let simple_tasks = [
        "Scan memory files for stale entries",
        "Detect language changes in repos",
        "Clean up old night runs",
    ];

    for prompt in &simple_tasks {
        let req = make_request(prompt, Some(InferenceTier::T1Trivial));
        let (resp, _) = router.route(&req, false).unwrap();
        assert_eq!(resp.model_used, "claude-haiku-4-5");
    }

    // Complex task unaffected by MLX outage
    let req = make_request(
        "Review security architecture",
        Some(InferenceTier::T3Complex),
    );
    let (resp, _) = router.route(&req, false).unwrap();
    assert_eq!(resp.model_used, "claude-sonnet-4");
}
