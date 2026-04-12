//! Night agent orchestration — scheduled agents, knowledge sync,
//! project tracking with automatic knowledge base updates.

pub mod auto_config;
pub mod ci_optimizer;
pub mod drift_detection;
pub mod ext;
pub mod inference_bridge;
pub mod knowledge;
pub mod knowledge_helpers;
pub mod mcp_defs;
pub mod memory_lint;
pub mod memory_lint_checks;
pub mod memory_lint_rules;
pub mod memory_lint_types;
pub mod routes;
pub mod routes_memory_lint;
pub mod routes_projects;
pub mod routes_routing;
pub mod routes_runs;
pub mod runner;
pub mod schema;
pub mod smart_spawner;
pub mod spawner;
pub mod types;

pub use ext::NightAgentsExtension;
