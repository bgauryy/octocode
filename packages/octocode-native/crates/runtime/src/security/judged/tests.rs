//! The judged-leaf mark: typed, beside the value, honored only at its
//! pointer and digest.
use super::*;
use crate::security::{ContentSecurity, Sanitized};
use serde_json::json;

/// A Rust struct whose field reads as a YAML secret once its lines are cut
/// from the syntax around them, and a real token.
const SOURCE: &str = "#[derive(Serialize)]\nstruct SavingWalk<'a> {\n    pending: &'a [Ref],\n    token: Option<&'a str>,\n    read: &'a [Value],\n}\n";
const FRAGMENT: &str =
    "    pending: &'a [Ref],\n    token: Option<&'a str>,\n    read: &'a [Value],\n";

fn token() -> String {
    format!("ghp_{}", "a".repeat(36))
}

fn judged(text: &str) -> JudgedLeaf {
    JudgedSource::new(SOURCE, Path::new("/w/src/walk.rs"))
        .leaf(text)
        .expect("a piece of the judged text")
}

fn seal(value: Value, marks: &JudgedLeaves) -> Value {
    ContentSecurity::new()
        .seal_judged(
            value,
            marks,
            false,
            || Ok::<_, std::convert::Infallible>(()),
        )
        .unwrap_or_else(|never| match never {})
        .into_value()
}

fn row(content: &str) -> Value {
    json!({"results": [{"data": {"path": "src/walk.rs", "content": content}}]})
}

const CONTENT: &str = "/results/0/data/content";

/// The fragment alone is over-redacted by the leaf net; marked, the gate
/// keeps the whole-source judgement.
#[test]
fn a_marked_leaf_keeps_the_whole_source_judgement() {
    let alone = seal(row(FRAGMENT), &JudgedLeaves::default());
    assert_ne!(
        alone["results"][0]["data"]["content"], FRAGMENT,
        "fixture no longer reproduces the fragment misjudgement"
    );
    let mut marks = JudgedLeaves::default();
    marks.mark(CONTENT, judged(FRAGMENT));
    let sealed = seal(row(FRAGMENT), &marks);
    assert_eq!(sealed["results"][0]["data"]["content"], FRAGMENT);
}

/// A leaf that changed after it was judged, or a mark at another pointer,
/// gets the full gate.
#[test]
fn a_changed_or_moved_leaf_is_gated_again() {
    let mut marks = JudgedLeaves::default();
    marks.mark(CONTENT, judged(FRAGMENT));
    let changed = format!("{FRAGMENT}// hint\n");
    let sealed = seal(row(&changed), &marks);
    assert_ne!(sealed["results"][0]["data"]["content"], changed.as_str());
    let mut elsewhere = JudgedLeaves::default();
    elsewhere.mark("/results/0/data/other", judged(FRAGMENT));
    let sealed = seal(row(FRAGMENT), &elsewhere);
    assert_ne!(sealed["results"][0]["data"]["content"], FRAGMENT);
}

/// Nothing in the value can mark a leaf: look-alike keys are plain data.
#[test]
fn look_alike_keys_in_the_value_mark_nothing() {
    let mut value = row(FRAGMENT);
    value["results"][0]["data"]["judged"] = json!(true);
    value["results"][0]["data"]["judgedLeaves"] = json!([[CONTENT, "digest"]]);
    value["results"][0]["judged"] = json!({"pointer": CONTENT});
    let sealed = seal(value, &JudgedLeaves::default());
    assert_ne!(sealed["results"][0]["data"]["content"], FRAGMENT);
}

/// A text that is no piece of the judged source is never marked, and a
/// credential in a marked leaf is still redacted (the whole pass redacted
/// it before the cut; a leaf that slipped one in fails the digest).
#[test]
fn only_pieces_of_the_judged_text_are_marked_and_secrets_stay_redacted() {
    let source = JudgedSource::new(SOURCE, Path::new("/w/src/walk.rs"));
    assert!(source.leaf("").is_none());
    assert!(source.leaf("token: Option<&'a str>,\n    write").is_none());
    let mut marks = JudgedLeaves::default();
    marks.mark(CONTENT, judged(FRAGMENT));
    let planted = format!("{FRAGMENT}const T: &str = \"{}\";\n", token());
    let sealed = seal(row(&planted), &marks);
    assert!(!sealed.to_string().contains(&token()));
}

/// Marks follow their value into an enclosing one.
#[test]
fn marks_rebase_under_a_prefix() {
    let mut marks = JudgedLeaves::default();
    marks.mark("/content", judged(FRAGMENT));
    let marks = marks.under("/results/0/data");
    let sealed = seal(row(FRAGMENT), &marks);
    assert_eq!(sealed["results"][0]["data"]["content"], FRAGMENT);
    let _ = Sanitized::sealed(row("plain"));
}
