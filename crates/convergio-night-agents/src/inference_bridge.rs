//! Inference bridge — thin adapter to call MLX or local models
//! directly from night-agents without full inference crate dependency.
//!
//! Uses the daemon's inference API at localhost:8420 when available,
//! falls back to direct MLX subprocess call.

use tracing::{info, warn};

/// Call a local model for simple text processing.
/// Tries the daemon inference API first, then direct MLX subprocess.
pub async fn call_local(model_name: &str, prompt: &str) -> Result<String, String> {
    // Try daemon inference API first (already running on same host)
    match call_daemon_api(prompt).await {
        Ok(content) => {
            info!("inference-bridge: daemon API responded");
            return Ok(content);
        }
        Err(e) => {
            warn!("inference-bridge: daemon API unavailable ({e}), trying direct MLX");
        }
    }

    // Direct MLX subprocess fallback
    call_mlx_direct(model_name, prompt).await
}

/// Call the daemon's own inference API at localhost.
async fn call_daemon_api(prompt: &str) -> Result<String, String> {
    let port = std::env::var("CONVERGIO_PORT").unwrap_or_else(|_| "8420".into());
    let url = format!("http://127.0.0.1:{port}/api/inference/generate");

    let body = serde_json::json!({
        "prompt": prompt,
        "max_tokens": 1024,
        "agent_id": "night-agent-inference",
        "tier_hint": "t1",
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        return Err(format!("daemon API returned {status}"));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| format!("parse: {e}"))?;

    json.get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "no content in response".into())
}

/// Direct MLX subprocess call (same logic as convergio-inference backend_mlx).
async fn call_mlx_direct(model_name: &str, prompt: &str) -> Result<String, String> {
    // Validate model_name to prevent code injection into Python script
    if !model_name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.')
    {
        return Err(format!("invalid model name: {model_name}"));
    }

    let python = resolve_python();
    let prompt_json = serde_json::to_string(prompt).unwrap_or_default();
    let model_json = serde_json::to_string(model_name).unwrap_or_default();

    // Pass model_name and prompt as JSON env vars instead of
    // interpolating them into the script to prevent injection.
    let script = r#"
import json, os
from mlx_lm import load, generate

model_name = json.loads(os.environ["_MLX_MODEL"])
raw = json.loads(os.environ["_MLX_PROMPT"])
model, tokenizer = load(model_name)
messages = [{"role": "user", "content": raw}]
prompt = tokenizer.apply_chat_template(
    messages, add_generation_prompt=True, tokenize=False
)
response = generate(model, tokenizer, prompt=prompt, max_tokens=1024)
for tag in ["<|im_start|>", "<|im_end|>", "<|endoftext|>"]:
    response = response.replace(tag, "")
response = response.strip()
print(json.dumps({"content": response}))
"#;

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&python)
            .args(["-c", script])
            .env("_MLX_MODEL", model_json)
            .env("_MLX_PROMPT", prompt_json)
            .output()
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))?
    .map_err(|e| format!("mlx subprocess: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("mlx-lm failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).map_err(|e| format!("parse: {e}"))?;

    parsed
        .get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "no content in MLX output".into())
}

fn resolve_python() -> String {
    if let Ok(p) = std::env::var("CONVERGIO_PYTHON") {
        return p;
    }
    if let Ok(home) = std::env::var("HOME") {
        let venv = format!("{home}/.convergio/mlx-env/bin/python3");
        if std::path::Path::new(&venv).exists() {
            return venv;
        }
    }
    "python3".to_string()
}
