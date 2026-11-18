use super::*;

use octocode_runtime_base::security::scan::Passthrough;
use octocode_test_support::json_fixture;

#[test]
fn a_projected_snippet_keeps_its_first_highlight_only() {
    let fragment = "# Title\n\n// keep  spacing\nuse   needle;\nlet needle = 1;";
    let at = |needle_line: &str| {
        let start = fragment.find(needle_line).unwrap();
        json!({"indices": [start, start + 6]})
    };
    let text_match: TextMatch = serde_json::from_value(json!({
            "fragment": fragment,
            "matches": [at("needle;"), at("needle =")]}))
    .unwrap();
    let (value, _) = project(&text_match, "README.md", &[], &Passthrough)
        .unwrap()
        .unwrap();
    assert_eq!(value, json_fixture(r#"{"highlight": "needle"}"#));
}

/// No projected value holds fragment text beyond the match GitHub
/// highlighted: a fake token the fragment's edge cuts (its prefix gone, so
/// no scanner can name it), whole at either edge, or split from its name
/// by a line break, never shows any 8-byte piece of it (B45).
#[test]
fn a_projected_fragment_keeps_only_its_highlight() {
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let token = "ghp_aB3dE5gH7jK9mN1pQ3sT5vX7zA9cE1gI3kM5";
    let pieces = |text: &str| {
        (4..token.len() - 8)
            .filter_map(|at| token.get(at..at + 8))
            .filter(|piece| text.contains(piece))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    for cut in [0, 4, 9, 20] {
        let (after, before) = (
            token.get(cut..).expect("ascii"),
            token.get(..token.len() - cut).expect("ascii"),
        );
        let head = format!("{after}\nlet needle = 1;\n");
        let tail = format!("let needle = 1;\nconst T = \"{before}");
        let split = format!("let needle = 1;\nconst TOKEN =\n  \"{after}\";");
        for fragment in [head, tail, split] {
            let start = fragment.find("needle").expect("needle");
            let text_match: TextMatch = serde_json::from_value(json!({
                    "fragment": fragment,
                    "matches": [{"indices": [start, start + 6]}]}))
            .expect("match");
            let (value, _) = project(&text_match, "src/main.rs", &[], &security)
                .expect("projected")
                .expect("shown");
            let shown = value.to_string();
            assert!(pieces(&shown).is_empty(), "cut {cut}: {shown}");
            assert_eq!(value["highlight"], "needle", "cut {cut}: {shown}");
        }
    }
}
