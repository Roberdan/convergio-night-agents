---
version: "1.0"
last_updated: "2026-04-07"
author: "convergio-team"
tags: ["adr"]
---

# ADR-025: Night Agent Smart Routing — MLX Local Default

**Status**: Accepted
**Date**: 2026-04-06

## Context

Night agents run scheduled tasks (memory lint, knowledge sync, stale cleanup, code review). Previously, all night agents used Claude CLI with `claude-haiku-4-5` as the hardcoded default model — regardless of task complexity.

The daemon already has a mature inference infrastructure:
- **ModelRouter** with tier-based selection (T1-trivial → T4-critical)
- **Classifier** for semantic prompt analysis (keyword + length heuristics)
- **MLX backend** for local Apple Silicon inference ($0 cost, ~37 tok/s on M1 Pro)
- **Budget tracking** with automatic tier downgrade under pressure

Night agents were bypassing all of this by directly shelling out to `claude -p`.

## Decision

1. **Default model changed to `"auto"`** — all new night agent definitions use smart routing by default.

2. **Dual-path spawner** classifies tasks at dispatch time:
   - **Simple tasks** (T1/T2): memory lint, knowledge sync, cleanup → MLX local inference via `inference_bridge.rs`. No Claude CLI, no API cost.
   - **Agent tasks** (T3/T4): code review, refactoring, architecture → Claude CLI with full tool access (file edit, terminal, git).

3. **Classification sources** (in priority order):
   - Model field prefix: `mlx:*` or `local:*` → force local; `claude-*` → force CLI
   - `model = "auto"` → prompt-based classification via keyword analysis
   - Agent keywords ("refactor", "implement", "review code") → Agent path
   - Everything else → Simple/inference path

4. **Inference bridge** tries two paths:
   - Daemon inference API at `localhost:8420/api/inference/generate` (reuses the running router)
   - Direct MLX subprocess fallback (if daemon API unreachable)
   - If both fail → graceful fallback to Claude CLI with haiku

5. **API endpoints** for monitoring and management:
   - `GET /api/night-agents/routing/stats` — model usage breakdown
   - `POST /api/night-agents/:id/routing` — set routing mode per agent
   - `POST /api/night-agents/routing/migrate-all` — batch migrate to auto

6. **CLI commands** (`cvg night`):
   - `cvg night routing` — show routing stats
   - `cvg night set-model <id> <model>` — set per-agent model
   - `cvg night migrate-auto` — migrate all to smart routing
   - `cvg night lint` / `cvg night lint-run` / `cvg night lint-summary`

## Consequences

- **Cost reduction**: Simple nightly tasks (3 out of 5 typical) run at $0 via MLX instead of paying Haiku API costs.
- **Latency**: MLX inference (~37 tok/s local) may be slower than Haiku API for some prompts, but eliminates network dependency.
- **Backward compatible**: Existing agent defs with explicit `claude-*` model continue to use Claude CLI unchanged.
- **Graceful degradation**: If MLX is unavailable (not installed, crash), falls back to Haiku automatically.
- **Validated**: 15 integration tests prove classification, routing, fallback, and budget pressure behavior. All pass.

## Validation

Test suite covers:
- Classifier assigns correct tiers to realistic night-agent prompts
- Router selects MLX ($0) over cloud for T1/T2, escalates for T3/T4
- MLX failure → automatic haiku fallback
- Budget pressure → complex tasks downgraded to cheaper models
- Full nightly cycle simulation (5 task types: 3 local + 2 cloud)
