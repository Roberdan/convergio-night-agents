//! Auto-config — assigns night agents based on project type.
//!
//! Called during onboarding to determine which night agents
//! should be enabled for a given project.

/// Return recommended night agent names for the given project type.
///
/// Project types are derived from `project_scanner::RepoType` display names
/// or language/framework identifiers (e.g. "rust", "nextjs", "python").
pub fn auto_assign_agents(project_type: &str) -> Vec<String> {
    let lower = project_type.to_lowercase();
    let mut agents: Vec<String> = Vec::new();

    // Universal agents — always included
    agents.push("dependency-auditor".to_string());
    agents.push("ci-optimizer".to_string());

    // Rust projects
    if lower.contains("rust") || lower.contains("cargo") {
        agents.push("security-scanner".to_string());
        return agents;
    }

    // Node / Next.js / React / Vue / TypeScript
    if lower.contains("node")
        || lower.contains("next")
        || lower.contains("react")
        || lower.contains("vue")
        || lower.contains("typescript")
        || lower.contains("javascript")
    {
        agents.push("lighthouse-auditor".to_string());
        return agents;
    }

    // Python
    if lower.contains("python") || lower.contains("django") || lower.contains("fastapi") {
        agents.push("security-scanner".to_string());
        return agents;
    }

    // Fallback: just the universal agents
    agents
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_project_gets_security_scanner() {
        let agents = auto_assign_agents("rust");
        assert!(agents.contains(&"dependency-auditor".to_string()));
        assert!(agents.contains(&"ci-optimizer".to_string()));
        assert!(agents.contains(&"security-scanner".to_string()));
        assert!(!agents.contains(&"lighthouse-auditor".to_string()));
    }

    #[test]
    fn nextjs_project_gets_lighthouse() {
        let agents = auto_assign_agents("nextjs");
        assert!(agents.contains(&"dependency-auditor".to_string()));
        assert!(agents.contains(&"lighthouse-auditor".to_string()));
        assert!(!agents.contains(&"security-scanner".to_string()));
    }

    #[test]
    fn unknown_project_gets_universal_only() {
        let agents = auto_assign_agents("unknown");
        assert_eq!(agents.len(), 2);
        assert!(agents.contains(&"dependency-auditor".to_string()));
        assert!(agents.contains(&"ci-optimizer".to_string()));
    }

    #[test]
    fn python_project_gets_security_scanner() {
        let agents = auto_assign_agents("python");
        assert!(agents.contains(&"security-scanner".to_string()));
    }
}
