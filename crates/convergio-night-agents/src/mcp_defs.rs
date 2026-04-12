//! MCP tool definitions for the night agents extension.

use convergio_types::extension::McpToolDef;
use serde_json::json;

pub fn night_agent_tools() -> Vec<McpToolDef> {
    vec![
        McpToolDef {
            name: "cvg_list_night_agents".into(),
            description: "List all night agent definitions with status.".into(),
            method: "GET".into(),
            path: "/api/night-agents".into(),
            input_schema: json!({"type": "object", "properties": {}}),
            min_ring: "sandboxed".into(),
            path_params: vec![],
        },
        McpToolDef {
            name: "cvg_trigger_night_agent".into(),
            description: "Trigger a night agent run by definition ID.".into(),
            method: "POST".into(),
            path: "/api/night-agents/:agent_id/trigger".into(),
            input_schema: json!({
                "type": "object",
                "properties": {"agent_id": {"type": "integer"}},
                "required": ["agent_id"]
            }),
            min_ring: "trusted".into(),
            path_params: vec!["agent_id".into()],
        },
    ]
}
