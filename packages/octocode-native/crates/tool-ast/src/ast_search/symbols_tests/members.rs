//! What an outline says of members: their visibility follows their owning
//! type, and a `limit` is counted in top-level declarations.

use super::*;
use octocode_test_support::json_fixture;

const SOURCE: &str = "pub(super) struct Hidden {\n    pub field: u8,\n}\n\npub struct Shown {\n    pub field: u8,\n}\n";

fn outline(row: serde_json::Value) -> serde_json::Value {
    let root = Fixture::new();
    let file = root.0.join("members.rs");
    std::fs::write(&file, SOURCE).expect("source");
    let mut query = json!({"operation":"symbols","path":file.to_string_lossy()});
    query
        .as_object_mut()
        .expect("object")
        .extend(row.as_object().expect("object").clone());
    run_lead(&root.0, query)
}

fn members<'a>(out: &'a serde_json::Value, owner: &str) -> Vec<&'a serde_json::Value> {
    out["symbols"]
        .as_array()
        .expect("symbols")
        .iter()
        .filter(|symbol| symbol["symbolName"] == owner)
        .flat_map(|symbol| symbol["members"].as_array().into_iter().flatten())
        .collect()
}

/// A `pub` field of a type that is not exported is not tagged exported;
/// one of an exported type is.
#[test]
fn a_member_of_an_unexported_type_is_not_tagged_exported() {
    let out = outline(json!({}));
    let hidden = members(&out, "Hidden");
    let shown = members(&out, "Shown");
    assert_eq!((hidden.len(), shown.len()), (1, 1), "{out}");
    assert!(hidden[0].get("exported").is_none(), "{out}");
    assert_eq!(shown[0]["exported"], true, "{out}");
}

/// A limit counts top-level declarations, so rows past it are members; the
/// page says so.
#[test]
fn a_limit_states_that_it_counts_top_level_declarations() {
    let out = outline(json_fixture(r#"{"limit": 1}"#));
    let top_level = out["symbols"].as_array().expect("symbols").len();
    let nested = members(&out, "Hidden").len() + members(&out, "Shown").len();
    assert!(nested > 0, "{out}");
    assert_eq!(top_level, 1, "{out}");
    let hints = out["warnings"].to_string();
    assert!(
        hints.contains("limit counts top-level declarations"),
        "{out}"
    );
    let unlimited = outline(json!({}));
    assert!(!unlimited["warnings"].to_string().contains("limit counts"));
}
