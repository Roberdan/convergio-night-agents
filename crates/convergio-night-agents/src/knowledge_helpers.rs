//! Helper functions for knowledge sync — git ops, scanning, upsert.

use std::path::Path;
use std::process::Command;

use rusqlite::params;

pub fn git_head(path: &Path) -> Option<String> {
    Command::new("git")
        .args(["-C", &path.to_string_lossy(), "rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

pub fn git_log_since(path: &Path, since_hash: &str) -> String {
    let range = format!("{since_hash}..HEAD");
    Command::new("git")
        .args([
            "-C",
            &path.to_string_lossy(),
            "log",
            "--oneline",
            "--no-decorate",
            &range,
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            crate::spawner::truncate_safe(&s, 2000)
        })
        .unwrap_or_default()
}

pub fn detect_key_changes(path: &Path, last_hash: Option<&str>) -> String {
    let Some(prev) = last_hash else {
        return String::new();
    };
    let range = format!("{prev}..HEAD");
    let output = Command::new("git")
        .args(["-C", &path.to_string_lossy(), "diff", "--name-only", &range])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let key_dirs = [
        "src/components/",
        "src/routes/",
        "src/lib/",
        "src/app/",
        "daemon/src/routes/",
        "daemon/src/models/",
        "packages/",
    ];
    let relevant: Vec<&str> = output
        .lines()
        .filter(|line| key_dirs.iter().any(|d| line.contains(d)))
        .collect();
    relevant.join("\n")
}

pub fn scan_profile(path: &Path) -> String {
    let mut parts = Vec::new();
    let langs = detect_languages(path);
    if !langs.is_empty() {
        parts.push(format!("Languages: {langs}"));
    }
    let frameworks = detect_frameworks(path);
    if !frameworks.is_empty() {
        parts.push(format!("Frameworks: {frameworks}"));
    }
    let key_files = [
        "package.json",
        "Cargo.toml",
        "tsconfig.json",
        "next.config.ts",
        "next.config.js",
    ];
    let found: Vec<&str> = key_files
        .iter()
        .filter(|f| path.join(f).exists())
        .copied()
        .collect();
    if !found.is_empty() {
        parts.push(format!("Manifests: {}", found.join(", ")));
    }
    parts.join("\n")
}

pub fn detect_languages(path: &Path) -> String {
    let exts: &[(&str, &str)] = &[
        ("rs", "Rust"),
        ("ts", "TypeScript"),
        ("tsx", "TypeScript"),
        ("js", "JavaScript"),
        ("py", "Python"),
        ("go", "Go"),
    ];
    let mut found: Vec<&str> = Vec::new();
    let dirs_to_check = [path.to_path_buf(), path.join("src")];
    for dir in &dirs_to_check {
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    for (e, lang) in exts {
                        if ext == *e && !found.contains(lang) {
                            found.push(lang);
                        }
                    }
                }
            }
        }
    }
    found.join(", ")
}

fn detect_frameworks(path: &Path) -> String {
    let mut found = Vec::new();
    if path.join("next.config.ts").exists() || path.join("next.config.js").exists() {
        found.push("Next.js");
    }
    if path.join("Cargo.toml").exists() {
        found.push("Rust");
    }
    if path.join("src-tauri").is_dir() {
        found.push("Tauri");
    }
    found.join(", ")
}

pub fn upsert_knowledge(
    conn: &rusqlite::Connection,
    project_name: &str,
    domain_suffix: &str,
    content: &str,
) {
    let domain = format!("project:{project_name}:{domain_suffix}");
    let title = format!("{project_name} — {domain_suffix}");
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_base WHERE domain = ?1",
            params![domain],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);
    if exists {
        let _ = conn.execute(
            "UPDATE knowledge_base SET content = ?1, \
             created_at = datetime('now') WHERE domain = ?2",
            params![content, domain],
        );
    } else {
        let _ = conn.execute(
            "INSERT INTO knowledge_base \
             (domain, title, content, created_at) \
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![domain, title, content],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_head_on_current_repo() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let head = git_head(path);
        assert!(head.is_some());
        assert!(!head.unwrap().is_empty());
    }

    #[test]
    fn detect_languages_finds_rust() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let langs = detect_languages(path);
        assert!(langs.contains("Rust"));
    }
}
