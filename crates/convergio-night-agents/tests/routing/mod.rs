//! Shared test helpers for MLX routing validation.

use convergio_inference::router::ModelRouter;
use convergio_inference::types::*;

pub fn mlx_model() -> ModelEndpoint {
    ModelEndpoint {
        name: "qwen2.5-coder-32b-mlx".into(),
        provider: ModelProvider::Mlx,
        url: String::new(),
        cost_per_1k_input: 0.0,
        cost_per_1k_output: 0.0,
        tier_range: (InferenceTier::T1Trivial, InferenceTier::T2Standard),
        healthy: true,
    }
}

pub fn cloud_haiku() -> ModelEndpoint {
    ModelEndpoint {
        name: "claude-haiku-4-5".into(),
        provider: ModelProvider::Cloud,
        url: "https://api.anthropic.com".into(),
        cost_per_1k_input: 0.25,
        cost_per_1k_output: 1.25,
        tier_range: (InferenceTier::T1Trivial, InferenceTier::T2Standard),
        healthy: true,
    }
}

pub fn cloud_sonnet() -> ModelEndpoint {
    ModelEndpoint {
        name: "claude-sonnet-4".into(),
        provider: ModelProvider::Cloud,
        url: "https://api.anthropic.com".into(),
        cost_per_1k_input: 3.0,
        cost_per_1k_output: 15.0,
        tier_range: (InferenceTier::T2Standard, InferenceTier::T3Complex),
        healthy: true,
    }
}

pub fn cloud_opus() -> ModelEndpoint {
    ModelEndpoint {
        name: "claude-opus-4".into(),
        provider: ModelProvider::Cloud,
        url: "https://api.anthropic.com".into(),
        cost_per_1k_input: 15.0,
        cost_per_1k_output: 75.0,
        tier_range: (InferenceTier::T3Complex, InferenceTier::T4Critical),
        healthy: true,
    }
}

pub fn night_router() -> ModelRouter {
    let mut r = ModelRouter::new();
    r.register_model(mlx_model());
    r.register_model(cloud_haiku());
    r.register_model(cloud_sonnet());
    r.register_model(cloud_opus());
    r
}

pub fn make_request(prompt: &str, hint: Option<InferenceTier>) -> InferenceRequest {
    InferenceRequest {
        prompt: prompt.into(),
        max_tokens: 1024,
        tier_hint: hint,
        agent_id: "night-agent".into(),
        org_id: Some("convergio".into()),
        plan_id: None,
        constraints: InferenceConstraints::default(),
    }
}
