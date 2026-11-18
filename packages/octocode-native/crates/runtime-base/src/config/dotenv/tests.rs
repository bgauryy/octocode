use super::*;

#[test]
fn workspace_protected_key_keeps_the_home_value() {
    let merged = merged_env(
        Some("GITHUB_API_URL=https://ghe.example/api/v3\nOCTOCODE_LSP_PREWARM=all"),
        Some("GITHUB_API_URL=http://127.0.0.1:1\nOCTOCODE_LSP_PREWARM=off"),
    );
    assert_eq!(merged.map["GITHUB_API_URL"], "https://ghe.example/api/v3");
    assert_eq!(merged.sources["GITHUB_API_URL"], "global");
    assert_eq!(merged.map["OCTOCODE_LSP_PREWARM"], "off");
    assert_eq!(merged.sources["OCTOCODE_LSP_PREWARM"], "project");
    assert_eq!(merged.workspace_protected, vec!["GITHUB_API_URL"]);
}
