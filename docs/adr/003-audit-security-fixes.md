# ADR-003: Security Audit and Hardening

**Status:** Accepted
**Date:** 2025-07-22
**Author:** Security Audit (Copilot)

## Context

A comprehensive security audit of convergio-night-agents (5,046 LOC)
identified several vulnerabilities related to command injection, input
validation, path traversal, race conditions, and unsafe string handling.

Night agents are particularly sensitive because they **execute external
processes** (Claude CLI, MLX Python subprocess) with user-supplied prompts
and model names.

## Findings and Fixes

### CRITICAL — Command Injection in MLX Subprocess

**Before:** `model_name` and `prompt` were interpolated directly into a
Python script string via `format!()`, allowing arbitrary code execution.

**Fix:** Replaced string interpolation with environment variables
(`_MLX_MODEL`, `_MLX_PROMPT`). The Python script reads values via
`json.loads(os.environ[...])`. Added `model_name` character allowlist
validation.

### CRITICAL — Race Condition in Dispatch (TOCTOU)

**Before:** `dispatch_all` checked `SELECT COUNT(*)` for active runs,
then separately inserted a new run — allowing duplicate dispatches under
concurrent scheduling.

**Fix:** Replaced with atomic `INSERT ... WHERE NOT EXISTS (SELECT ...)`,
eliminating the check-then-act race.

### HIGH — Missing Input Validation on All Endpoints

**Before:** `CreateAgentBody` and `CreateProjectBody` accepted arbitrary
strings without length limits, character validation, or format checks.

**Fix:** Added `validate()` methods with:
- Name: 1–128 chars, alphanumeric/dash/underscore/space only
- Schedule: 5 cron fields, safe characters only
- Model: allowlist + prefix validation for `mlx:`/`local:`
- Prompt: 1–32,000 chars
- `max_runtime_secs`: 1–86,400 (24h cap)
- `repo_path`: absolute path required, no `..` or null bytes

### HIGH — Path Traversal in Tracked Projects

**Before:** `repo_path` accepted any string including `../../etc/passwd`.

**Fix:** `CreateProjectBody::validate()` rejects relative paths, `..`
segments, and null bytes. Requires absolute paths only.

### MEDIUM — Unsafe String Truncation

**Before:** `&outcome[..2000]` could panic on multi-byte UTF-8 boundaries.

**Fix:** Introduced `truncate_safe()` helper that finds the nearest valid
char boundary before truncating. Applied in spawner, smart_spawner, and
knowledge_helpers.

### LOW — Duplicate Model Validation Logic

**Before:** `routes_routing.rs` had its own inline model allowlist
separate from the one in types.

**Fix:** Centralized to `types::validate_model()`, used by both
`CreateAgentBody::validate()` and `set_routing` handler.

## Decision

All fixes applied. Validation is enforced at the API boundary before
any database or filesystem operations.

## Consequences

- Breaking change: API now rejects previously-accepted invalid inputs
- Model allowlist must be updated when new models are supported
- Future: consider auth middleware (currently relies on daemon's
  ring-based security model)
