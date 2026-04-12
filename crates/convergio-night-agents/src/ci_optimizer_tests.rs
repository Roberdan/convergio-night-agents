use super::*;

#[test]
fn detects_missing_cache() {
    let content = "jobs:\n  build:\n    steps:\n      \
                   - run: npm ci\n";
    let findings = check_missing_cache("ci.yml", content);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].category, "missing-cache");
}

#[test]
fn no_false_positive_with_cache() {
    let content = "jobs:\n  build:\n    steps:\n      \
                   - uses: actions/cache@v3\n      \
                   - run: npm ci\n";
    let findings = check_missing_cache("ci.yml", content);
    assert!(findings.is_empty());
}

#[test]
fn detects_missing_timeout() {
    let content = "jobs:\n  build:\n    runs-on: ubuntu\n";
    let findings = check_missing_timeout("ci.yml", content);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].category, "missing-timeout");
}
