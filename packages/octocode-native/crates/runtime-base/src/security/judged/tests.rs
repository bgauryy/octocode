//! The judged-leaf mark: typed, beside the value, honored only at its
//! pointer and digest.
use super::*;
use crate::security::{ContentSecurity, Sanitized};
use serde_json::json;

/// A source whose identifier holds a token's shape inside a longer word, so
/// no pattern matches it in place, and a fragment cut where that shape starts,
/// which a pattern matches alone.
const SOURCE: &str = "#[derive(Serialize)]\nstruct Walk {\n    id: xsk-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789ABCD,\n    read: Vec<Value>,\n}\n";
const FRAGMENT: &str = "sk-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789ABCD,\n    read: Vec<Value>,\n";

fn token() -> String {
    format!("ghp_{}", "a".repeat(36))
}

fn judged(text: &str) -> JudgedLeaf {
    JudgedSource::new(SOURCE, Path::new("/w/src/walk.rs"))
        .leaf(text, |_| false)
        .expect("a piece of the judged text")
}

fn seal(value: Value, marks: &JudgedLeaves) -> Value {
    ContentSecurity::new()
        .seal_judged(
            value,
            marks,
            &crate::security::Admitted::default(),
            crate::security::Pii::default(),
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
    assert!(source.leaf("", |_| false).is_none());
    assert!(
        source
            .leaf("sk-AbCdEfGhIjKl,\n    write", |_| false)
            .is_none()
    );
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

/// A token on a line of its own, its name on an earlier line, a comment
/// between.
const KEYED: &str = "const apiKey =\n  // the key\n  \"ghp_Bq1Zx9Lm2Vp4Kd8Rt7Yb6Nc3Aa5Ss7Dd9Ff1Gg3\";\nconst label = \"plain\";\nexport const options = {\n  token: <string>raw,\n};\n";
const VALUE: &str = "ghp_Bq1Zx9Lm2Vp4Kd8Rt7Yb6Nc3Aa5Ss7Dd9Ff1Gg3";

fn keyed() -> JudgedText {
    JudgedText::judge(KEYED, Path::new("/w/src/s.ts"), &ContentSecurity::new())
}

/// Every cut of a judged source shows the whole-source judgment at its
/// place: the value line, the node inside its quotes, a span inside the
/// value; text the pass left alone is the source's own bytes.
#[test]
fn a_cut_shows_the_whole_source_judgment_at_its_place() {
    let judged = keyed();
    assert!(judged.is_redacted());
    let read = judged.lines(3, 1).expect("line 3").into_owned();
    assert!(
        !read.contains(VALUE) && read.contains("[REDACTED"),
        "{read}"
    );
    let line = judged.line_span(3, 1).expect("line 3");
    assert_eq!(judged.cut(line.clone()), read);
    let at = KEYED.find(VALUE).expect("value");
    let node = judged.cut(at - 1..at + VALUE.len() + 1);
    assert!(read.contains(node.as_ref()), "{node} of {read}");
    assert!(!judged.cut(at + 4..at + 9).contains(&VALUE[4..9]));
    let label = judged.line_span(4, 1).expect("line 4");
    assert!(!judged.exposes(&label));
    assert_eq!(judged.cut(label), "const label = \"plain\";");
    // Lines the pass left alone are the read's lines, placeholders and all.
    assert_eq!(judged.lines(1, 4).expect("lines"), {
        let mut lines = KEYED.lines().take(4).collect::<Vec<_>>();
        lines[2] = &read;
        lines.join("\n")
    });
}

/// A fragment is placed by its line and column, in whatever unit the tool
/// counts.
#[test]
fn a_fragment_is_placed_and_never_judged_alone() {
    let judged = keyed();
    let quoted = format!("\"{VALUE}\"");
    for column in [Some(2), None] {
        let (shown, placed) = judged.fragment(3, column, &quoted);
        assert!(placed);
        assert!(!shown.contains(VALUE), "{shown}");
    }
    let code = "  token: <string>raw,";
    assert_eq!(judged.fragment(6, Some(0), code), (code.into(), true));
    // Text that is not the source's at its place is judged by its lines.
    assert_eq!(judged.fragment(4, None, "edited").0, "edited");
    assert!(!judged.fragment(3, None, "edited").0.contains("edited"));
    assert!(judged.fragment(99, None, "x").0.contains("REDACTED"));
}

/// A fact with no place (a stored name) shows no value the pass replaced.
#[test]
fn a_placeless_fact_is_scrubbed_of_the_redacted_values() {
    let judged = keyed();
    let fact = format!("\"{VALUE}\".to_owned");
    let scrubbed = judged.scrubbed(&fact).expect("scrubbed");
    assert!(
        !scrubbed.contains(VALUE) && scrubbed.ends_with("\".to_owned"),
        "{scrubbed}"
    );
    assert_eq!(judged.scrubbed("getGitHubToken"), None);
}

/// A request judges a source once, and the gate honors the texts it showed.
#[test]
fn a_request_judges_a_source_once_and_its_shown_texts_pass_the_gate() {
    let (security, files) = ContentSecurity::new().for_request();
    let path = Path::new("/w/src/walk.rs");
    text::JUDGED_BYTES.with(|judged| judged.set(0));
    let scan = |_: &[u8; 32]| JudgedText::judge(SOURCE, path, &security);
    let first = security.judge_source(SOURCE, path, scan);
    for _ in 0..2_000 {
        let _ = security.judge_source(SOURCE, path, scan);
    }
    assert_eq!(text::JUDGED_BYTES.with(std::cell::Cell::get), SOURCE.len());
    security.show(first.path(), FRAGMENT);
    let marks = files.leaves().under("/results/0/data");
    assert_eq!(
        seal(row(FRAGMENT), &marks)["results"][0]["data"]["content"],
        FRAGMENT
    );
    // Another text of the row is no shown text: the full gate judges it.
    let other = format!("{FRAGMENT}// more\n");
    assert_ne!(
        seal(row(&other), &marks)["results"][0]["data"]["content"],
        other.as_str()
    );
}
