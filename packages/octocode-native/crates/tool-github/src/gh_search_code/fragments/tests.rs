use super::*;

use octocode_runtime_base::security::scan::Passthrough;
use serde_json::json;

/// A fragment scores by its sanitized text against the terms, and a
/// fragment sanitizing leaves empty scores nothing.
#[test]
fn a_fragment_scores_its_text_and_an_empty_one_scores_nothing() {
    let terms = super::super::ranking::terms(&["needle".to_owned()]).expect("terms");
    let of = |fragment: &str| {
        let text_match: TextMatch =
            serde_json::from_value(json!({"fragment": fragment, "matches": []})).expect("match");
        score(&text_match, "README.md", &terms, &Passthrough).expect("scored")
    };
    let hit = of("let needle = 1;").expect("text");
    let miss = of("let other = 1;").expect("text");
    assert!(hit > miss, "{hit} {miss}");
    assert_eq!(of(""), None);
}
