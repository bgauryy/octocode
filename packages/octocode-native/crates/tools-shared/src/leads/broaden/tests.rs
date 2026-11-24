use super::*;
use octocode_test_support::json_fixture;

#[test]
fn broadening_drops_one_filter_group_or_keyword_and_keeps_public_scope() {
    let row = json_fixture(
        r#"{
            "operation": "search",
            "owner": "octo",
            "keywords": ["cache", "redis"],
            "filters": {"language": "Rust"},
            "sort": {"by": "stars", "order": "desc"},
            "page": 3
        }"#,
    );
    let lead = broaden(
        ToolId::GhSearchRepo,
        &row,
        &Broaden {
            strip: &[],
            groups: &[&["filters"]],
            split_lone_keyword: false,
        },
        5,
    )
    .expect("broader searches");
    assert_eq!(lead["kind"], "related");
    octocode_runtime_base::contracts::validate("ghSearchRepo", lead["input"].clone())
        .expect("authored input");
    let variants = lead["input"]["queries"].as_array().expect("queries");
    assert_eq!(variants.len(), 3);
    for variant in variants {
        assert_eq!(variant["owner"], row["owner"]);
        assert_eq!(variant["sort"], row["sort"]);
        assert!(variant.get("page").is_none());
        let filter_removed = variant.get("filters").is_none();
        let keyword_removed = variant["keywords"].as_array().expect("keywords").len() == 1;
        assert_ne!(
            filter_removed, keyword_removed,
            "one scope change per variant: {variant}"
        );
    }
}

#[test]
fn broadening_never_emits_a_request_with_no_search_target() {
    let row = json_fixture(
        r#"{
            "operation": "search",
            "ecosystem": "npm",
            "keywords": ["cache"]
        }"#,
    );
    assert!(
        broaden(
            ToolId::ArtifactSearch,
            &row,
            &Broaden {
                strip: &[],
                groups: &[],
                split_lone_keyword: false,
            },
            5
        )
        .is_none()
    );
}

/// A broader lead another related call already runs is no second lead: the
/// row's `related` has one writer, which states each call once.
#[test]
fn a_broader_lead_a_related_call_already_holds_is_not_added_again() {
    let row = |keyword: &str| json!({"operation":"search","keywords":[keyword]});
    let wide = octocode_runtime_base::response::result::Continuation::related_input(
        ToolId::GhSearchRepo,
        json!({"queries": [row("cache"), row("redis")]}),
    )
    .build();
    let narrow = octocode_runtime_base::response::result::Continuation::related_input(
        ToolId::GhSearchRepo,
        json!({"queries": [row("redis")]}),
    )
    .build();
    let mut value = json!({"related": [wide.clone()]});
    set_broadened(&mut value, &json!({}), narrow.clone());
    set_broadened(&mut value, &json!({}), wide.clone());
    assert_eq!(value["related"], json!([wide]), "{value}");
}

/// The hints an empty `row` gets beside `lead`.
fn broadened_hints(row: &Value, lead: Value) -> Vec<String> {
    let mut value = json!({"status": "empty"});
    set_broadened(&mut value, row, lead);
    value["hints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// Each broader request drops exactly one narrowing, so the hint names
/// every dropped filter and keyword: a keyword-only search names its terms,
/// not "filters".
#[test]
fn the_broadened_hint_names_what_each_request_drops() {
    let row = json_fixture(
        r#"{
            "operation": "search",
            "keywords": ["cache", "redis"],
            "filters": {"language": "Rust", "stars": {"min": 10}}
        }"#,
    );
    let drop = Broaden {
        strip: &[],
        groups: &[&["filters"]],
        split_lone_keyword: false,
    };
    let lead = broaden(ToolId::GhSearchRepo, &row, &drop, 5).expect("broader searches");
    let hints = broadened_hints(&row, lead);
    assert_eq!(hints.len(), 1, "{hints:?}");
    for dropped in ["filters.language", "filters.stars", "`cache`", "`redis`"] {
        assert!(hints[0].contains(dropped), "{dropped}: {hints:?}");
    }
    assert!(hints[0].chars().count() <= HINT_CHARS, "{hints:?}");
    let row = json_fixture(r#"{"operation": "search", "keywords": ["cache", "redis"]}"#);
    let lead = broaden(ToolId::GhSearchRepo, &row, &drop, 5).expect("broader searches");
    let hints = broadened_hints(&row, lead);
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert!(!hints[0].contains("filter"), "{hints:?}");
}

/// A field every request leaves out (one the tool never copies) narrows
/// none of them apart: the hint names only what each one drops.
#[test]
fn a_field_every_broader_request_leaves_out_is_not_named() {
    let row = json_fixture(r#"{"owner": "o", "keywords": ["cache", "redis"], "ref": "dev"}"#);
    let drop = Broaden {
        strip: &["ref"],
        groups: &[],
        split_lone_keyword: false,
    };
    let lead = broaden(ToolId::GhSearchCode, &row, &drop, 2).expect("broader searches");
    let hints = broadened_hints(&row, lead);
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert!(
        hints[0].contains("`cache`") && hints[0].contains("`redis`"),
        "{hints:?}"
    );
    assert!(!hints[0].contains("ref"), "{hints:?}");
}

/// The generic hint's premise is that every related request only drops a
/// narrowing: beside a request that rewrites the search (globs rebased under
/// `path`) or drops nothing, and beside a more specific hint already on the
/// row, it says nothing; the related call is still attached.
#[test]
fn no_broadened_hint_beside_a_rewrite_or_a_more_specific_hint() {
    let row = json_fixture(
        r#"{"operation": "tree", "owner": "o", "repo": "r", "path": "src", "include": ["src/*.rs"]}"#,
    );
    let mut rebased = row.clone();
    rebased["include"] = json!(["*.rs"]);
    let mut dropped = row.clone();
    if let Some(fields) = dropped.as_object_mut() {
        fields.remove("include");
    }
    let lead = Continuation::related_input(
        ToolId::GhStructure,
        json!({"queries": [rebased, dropped.clone()]}),
    )
    .build();
    let mut value = json!({"status": "empty"});
    set_broadened(&mut value, &row, lead);
    assert!(value.get("hints").is_none(), "{value}");
    assert!(value.get("related").is_some(), "{value}");
    let same = Continuation::related(ToolId::GhStructure, row.clone()).build();
    assert!(broadened_hints(&row, same).is_empty());
    let mut value = json!({"status": "empty", "hints": ["A more specific hint."]});
    let lead = Continuation::related(ToolId::GhStructure, dropped).build();
    set_broadened(&mut value, &row, lead.clone());
    assert_eq!(value["hints"], json!(["A more specific hint."]), "{value}");
    let hints = broadened_hints(&row, lead);
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert!(hints[0].contains("include"), "{hints:?}");
}

/// A lone phrase with nothing else to drop is told to change its words; the
/// hint is short and names no ANDing, which one phrase cannot have.
#[test]
fn a_lone_phrase_hint_is_short_and_names_no_anding() {
    let mut value = json!({"status": "empty"});
    set_no_broader(&mut value);
    let hint = value["hints"][0].as_str().expect("hint");
    assert!(hint.chars().count() <= HINT_CHARS, "{hint}");
    assert!(!hint.contains("AND"), "{hint}");
}

/// The policy bound on one hint sentence.
const HINT_CHARS: usize = 150;
