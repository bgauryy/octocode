use super::*;

fn no_split<'a>(strip: &'a [&'a str], groups: &'a [&'a [&'a str]]) -> Broaden<'a> {
    Broaden {
        strip,
        groups,
        split_lone_keyword: false,
    }
}

/// The one broadening rule every search shares: paging keys stay out; one
/// variant per set filter group (its keys dropped together), then one per
/// dropped keyword (a lone keyword drops whole); a variant the contract
/// rejects is skipped, and `limit` caps the batch.
#[test]
fn broaden_drops_each_filter_group_then_each_keyword() {
    let groups: &[&[&str]] = &[&["qualifiers"], &["since", "until"]];
    let drop = no_split(&["fields"], groups);
    let row = json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["a","b"],
        "since":"2026-01-01","until":"2026-02-01","page":3,"pageSize":7,"fields":["number"]});
    let lead = broaden(ToolId::GhSearchHistory, &row, &drop, 10).expect("variants");
    assert_eq!(lead["confidence"], "low", "{lead}");
    let base = json!({"operation":"pullRequest","owner":"o","repo":"r"});
    let with = |extra: Value| {
        let mut variant = base.clone();
        variant
            .as_object_mut()
            .expect("row")
            .extend(extra.as_object().expect("extra").clone());
        variant
    };
    assert_eq!(
        lead["query"]["queries"],
        json!([
            with(json!({"keywords":["a","b"]})),
            with(json!({"keywords":["a"],"since":"2026-01-01","until":"2026-02-01"})),
            with(json!({"keywords":["b"],"since":"2026-01-01","until":"2026-02-01"})),
        ]),
        "{lead}"
    );
    // A lone keyword drops whole; the limit caps the batch.
    let lone = json!({"owner":"o","repo":"r","keywords":["a"],"language":"rust"});
    let filters: &[&[&str]] = &[&["language"]];
    let drop = no_split(&[], filters);
    let lead = broaden(ToolId::GhSearchRepo, &lone, &drop, 5).expect("variants");
    assert_eq!(
        lead["query"]["queries"],
        json!([{"owner":"o","repo":"r","keywords":["a"]},{"owner":"o","repo":"r","language":"rust"}])
    );
    let lead = broaden(ToolId::GhSearchRepo, &lone, &drop, 1).expect("variants");
    assert_eq!(lead["query"]["queries"].as_array().map(Vec::len), Some(1));
    // Nothing left to search by: the contract rejects the variant.
    let bare = json!({"keywords":["a"]});
    assert!(broaden(ToolId::GhSearchRepo, &bare, &drop, 5).is_none());
}

/// A variant is a fresh search: every walk-position field and the page size
/// stay out, whatever the tool names them.
#[test]
fn a_variant_never_copies_where_the_walk_stands() {
    let row = json!({"keywords":["a","b"],"page":4,"pageSize":9,"selection":"plan","owner":"o"});
    let lead = broaden(ToolId::GhSearchCode, &row, &no_split(&[], &[]), 5).expect("variants");
    for variant in lead["query"]["queries"].as_array().expect("rows") {
        for key in ["page", "pageSize", "selection"] {
            assert!(variant.get(key).is_none(), "{key}: {variant}");
        }
    }
}

/// A registry reads one keyword as several terms: with
/// `split_lone_keyword`, a lone multi-word keyword drops one word at a time
/// (the last first), and the note names the dropped word, not the keyword.
#[test]
fn a_split_lone_keyword_drops_one_word_at_a_time() {
    // `cursor` and `page` are the walk position (cursor fields): no
    // variant copies them, and the note does not name them.
    let drop = Broaden {
        strip: &[],
        groups: &[],
        split_lone_keyword: true,
    };
    let row = json!({"ecosystem":"crates","keywords":["zzz async runtime"],"cursor":"c","page":2});
    let lead = broaden(ToolId::ArtifactSearch, &row, &drop, 5).expect("variants");
    assert_eq!(
        lead["query"]["queries"],
        json!([
            {"ecosystem":"crates","keywords":["zzz async"]},
            {"ecosystem":"crates","keywords":["zzz runtime"]},
            {"ecosystem":"crates","keywords":["async runtime"]},
        ]),
        "{lead}"
    );
    let mut value = json!({});
    set_broadened(&mut value, &row, lead);
    assert_eq!(
        value["hints"],
        json!([
            "broadenSearch rows: queries[0] drops word \"runtime\"; queries[1] drops word \"async\"; queries[2] drops word \"zzz\""
        ]),
        "{value}"
    );
    // Several keywords still drop one keyword at a time.
    let many = json!({"ecosystem":"crates","keywords":["zzz","async runtime"]});
    let lead = broaden(ToolId::ArtifactSearch, &many, &drop, 5).expect("variants");
    assert_eq!(
        lead["query"]["queries"],
        json!([{"ecosystem":"crates","keywords":["zzz"]},{"ecosystem":"crates","keywords":["async runtime"]}]),
        "{lead}"
    );
    // A one-word lone keyword leaves nothing to search by: no lead.
    let single = json!({"ecosystem":"crates","keywords":["zzz"]});
    assert!(broaden(ToolId::ArtifactSearch, &single, &drop, 5).is_none());
}

/// A broadened search never drops a filter silently: beside the lead, one
/// hint names what each row drops (a filter group, a keyword, a key the
/// tool never copies) or sets, relative to the search it broadens. Paging,
/// output and brief keys are not search filters and go unnamed.
#[test]
fn a_broadened_search_names_what_each_row_drops() {
    let row = json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["a","b"],
        "since":"2026-01-01","until":"2026-02-01","qualifiers":"author:x","page":2,
        "extras":["details"],"mainGoal":"g"});
    let drop = no_split(&[], &[&["qualifiers"], &["since", "until"]]);
    let lead = broaden(ToolId::GhSearchHistory, &row, &drop, 5).expect("variants");
    let mut value = json!({"pullRequests":[],"hints":["tip"]});
    set_broadened(&mut value, &row, lead.clone());
    assert_eq!(value["next"]["broadenSearch"], lead, "{value}");
    assert_eq!(
        value["hints"],
        json!([
            "tip",
            "broadenSearch rows: queries[0] drops qualifiers; queries[1] drops since, until; queries[2] drops keyword \"b\"",
            "broadenSearch rows: queries[3] drops keyword \"a\""
        ]),
        "{value}"
    );
    // A row that rewrites a value says what it sets; a key the tool never
    // copies (`ref`) is named as dropped.
    let row = json!({"owner":"o","repo":"r","path":"crates","ref":"dev","include":["crates/*.rs"]});
    let lead = json!({"tool":"ghStructure","query":{"queries":[
        {"owner":"o","repo":"r","path":"crates","include":["*.rs"]}]},"confidence":"low"});
    let mut value = json!({});
    set_broadened(&mut value, &row, lead);
    assert_eq!(
        value["hints"],
        json!(["broadenSearch rows: queries[0] drops ref, sets include [\"*.rs\"]"]),
        "{value}"
    );
}

/// The note names every row however many there are, and each note is one
/// hint within the guidance budget: rows pack in order, a row that overflows
/// starts the next hint, and a row whose own values are too long to state is
/// named by position, never cut.
#[test]
fn broadened_notes_stay_within_the_hint_budget_and_name_every_row() {
    let budget = crate::response::rows::MAX_GUIDANCE_CHARS;
    let drop = no_split(&[], &[&["qualifiers"], &["since", "until"]]);
    let long = "x".repeat(150);
    let row = json!({"operation":"pullRequest","owner":"o","repo":"r",
        "keywords":["alpha","beta","gamma",long,"delta"],
        "since":"2026-01-01","until":"2026-02-01","qualifiers":"author:x"});
    let lead = broaden(ToolId::GhSearchHistory, &row, &drop, 5).expect("variants");
    let rows = lead["query"]["queries"].as_array().expect("rows").len();
    assert_eq!(rows, 5, "{lead}");
    let mut value = json!({});
    set_broadened(&mut value, &row, lead);
    let hints = value["hints"].as_array().expect("hints");
    assert!(hints.len() > 1, "{value}");
    for hint in hints {
        let hint = hint.as_str().expect("text");
        assert!(hint.chars().count() <= budget, "{hint}");
    }
    let all = hints
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    for index in 0..rows {
        assert!(all.contains(&format!("queries[{index}] ")), "{all}");
    }
    // The over-long keyword is named by its position; the others by value.
    assert!(all.contains("drops keyword 4"), "{all}");
    assert!(all.contains("drops keyword \"delta\""), "{all}");
    assert!(!all.contains(&long), "{all}");
}

/// `row` (an executor row with `next.broadenSearch`) as the response stage
/// publishes it for the request row `query`.
fn published(tool: ToolId, query: &Value, row: &Value) -> Value {
    use crate::response::continuations::{Scope, Sources, finalize};
    let mut shaped = crate::response::rows::result_row(tool, 0, query, row.clone(), Some("empty"));
    crate::response::rows::apply_hint_policy(&mut shaped, tool, query);
    crate::response::extras::prune_row(&mut shaped, tool, query);
    crate::response::rows::minimize_row(&mut shaped, tool, query);
    let mut public = json!({ "results": [shaped] });
    finalize(
        &mut public,
        tool,
        &Sources::Rows(&[Some(query)]),
        &Scope::everything(),
    )
    .expect("valid continuations");
    crate::response::channels::publish_hints(&mut public, tool);
    public
}

/// `lead` with `keys` removed from each of its rows.
fn without_keys(lead: &Value, keys: &[&str]) -> Value {
    let mut lead = lead.clone();
    for variant in lead["query"]["queries"]
        .as_array_mut()
        .expect("rows")
        .iter_mut()
        .filter_map(Value::as_object_mut)
    {
        for key in keys {
            variant.remove(*key);
        }
    }
    lead
}

/// `query`'s empty `row` with `lead` as its broadening (the note is the
/// builder's, from `noted`), as the response stage publishes it.
fn shown(tool: ToolId, query: &Value, row: &Value, noted: &Value, lead: &Value) -> Value {
    let mut row = row.clone();
    set_broadened(&mut row, query, noted.clone());
    row["next"]["broadenSearch"] = lead.clone();
    published(tool, query, &row)
}

/// The brief and `extras` are the continuation stage's: it drops the brief
/// from every broadened row and gives each the source row's `extras`, so a
/// builder that also stripped them changed nothing the response shows.
#[test]
fn the_stage_owns_the_brief_and_extras_of_broadened_rows() {
    let query = json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["a","b"],
        "qualifiers":"author:x","extras":["details"],"mainGoal":"g","reasoning":"r"});
    let drop = no_split(&[], &[&["qualifiers"]]);
    let lead = broaden(ToolId::GhSearchHistory, &query, &drop, 5).expect("variants");
    let stripped = without_keys(&lead, &["extras", "mainGoal", "reasoning"]);
    let row = json!({"pullRequests":[]});
    let tool = ToolId::GhSearchHistory;
    let kept = shown(tool, &query, &row, &lead, &lead);
    assert_eq!(kept, shown(tool, &query, &row, &lead, &stripped));
    let text = kept.to_string();
    assert!(
        !text.contains("mainGoal") && !text.contains("reasoning"),
        "{kept}"
    );
    assert!(
        text.contains("broadenSearch: Approximate: ghSearchHistory"),
        "{kept}"
    );
    assert!(text.contains(r#"\"extras\":[\"details\"]"#), "{kept}");
}

/// A search that judges its hits (`clasifyContext`) keeps its brief on its
/// broadened rows, as the stage keeps it on every context-selection call:
/// the judge reads the same research goal.
#[test]
fn a_judged_search_keeps_its_brief_on_broadened_rows() {
    let query = json!({"keywords":["a","b"],"owner":"o","mainGoal":"g","reasoning":"r",
        "clasifyContext":{"classifiers":["Does this evidence show the selected behavior?"]}});
    let lead = broaden(ToolId::GhSearchCode, &query, &no_split(&[], &[]), 2).expect("variants");
    let row = json!({"files":[]});
    let kept = shown(ToolId::GhSearchCode, &query, &row, &lead, &lead);
    let text = kept.to_string();
    assert!(text.contains(r#"\"mainGoal\":\"g\""#), "{kept}");
    assert!(text.contains(r#"\"reasoning\":\"r\""#), "{kept}");
}
