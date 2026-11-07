//! What a candidate carries: the enclosing declaration, merged snippet
//! windows, and an exact glob.

use super::*;
use octocode_contract_types::LocalSearchQuery;

/// Records every admitted state and answers nothing.
struct Capture(std::sync::Mutex<Vec<Value>>);

impl SelectionJudge for Capture {
    fn identity(&self) -> Value {
        json!({"test":"capture"})
    }
    fn select(&self, call: JudgeCall<'_>) -> Judgments {
        let mut seen = self.0.lock().expect("lock");
        seen.extend(
            call.admitted
                .iter()
                .filter_map(|&i| call.candidates[i].state.clone()),
        );
        Ok(Vec::new())
    }
}

/// The states a classified search of `files` sends, with `fields` added to
/// the query.
fn states(files: &[(&str, &str)], fields: Value) -> Vec<Value> {
    let root = tempfile::tempdir().expect("fixture");
    for (name, text) in files {
        std::fs::write(root.path().join(name), text).expect("fixture file");
    }
    let mut query =
        json!({"path": root.path(), "classification": {"predicates": ["Is this test code?"]}});
    for (key, value) in fields.as_object().into_iter().flatten() {
        query[key] = value.clone();
    }
    let query: LocalSearchQuery = serde_json::from_value(query).expect("query");
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let judge = std::sync::Arc::new(Capture(std::sync::Mutex::new(Vec::new())));
    let selection = Runtime {
        judge: judge.clone(),
        ..runtime(None, &executor)
    };
    execute_local_search(
        &query,
        &crate::tools::test_support::workspace_policy(root.path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        Some(&selection),
    )
    .expect("search");
    judge.0.lock().expect("lock").clone()
}

/// A candidate names the declaration around each hit, so a predicate
/// about scope can be judged.
#[test]
fn candidates_carry_the_enclosing_declaration() {
    let seen = states(
        &[(
            "a.rs",
            "fn production() {\n    needle();\n}\n\n#[test]\nfn checks_it() {\n    needle();\n}\n",
        )],
        json!({"matchString": ["needle"], "contextLines": {"before":0,"after":0}}),
    );
    let names = seen
        .iter()
        .map(|state| state["target"]["enclosing"]["symbolName"].clone())
        .collect::<Vec<_>>();
    assert_eq!(names, [json!("production"), json!("checks_it")], "{seen:?}");
}

/// A matchOnly row is only its span; the judge reads the whole line that
/// holds it, as the content view shows it, and the label still quotes the
/// span.
#[test]
fn a_match_only_hit_is_judged_over_its_whole_line() {
    let line = "    fromBrowser.catch(() => undefined);";
    let seen = states(
        &[("a.ts", &format!("try {{\n{line}\n}}\n"))],
        json!({"matchString": [".catch("], "view": "matchOnly"}),
    );
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0]["lines"], json!([[2, line]]), "{seen:?}");
    assert_eq!(seen[0]["target"]["text"], line);
    assert_eq!(seen[0]["target"]["match"], ".catch(");
}

/// A hit whose window cannot show the section it sits in still names it:
/// with no context, `"x"` at line 15 of a package.json is a devDependency.
#[test]
fn a_hit_names_the_key_path_of_its_line() {
    let package = "{\n  \"name\": \"octocode-mcp\",\n  \"version\": \"1.0.0\",\n  \"type\": \"module\",\n  \"main\": \"dist/index.js\",\n  \"license\": \"MIT\",\n  \"scripts\": {},\n  \"dependencies\": {\n    \"a-dep\": \"^1.0.0\",\n    \"b-dep\": \"^1.0.0\",\n    \"c-dep\": \"^1.0.0\"\n  },\n  \"devDependencies\": {\n    \"d-dev\": \"^1.0.0\",\n    \"x\": \"^1.0.0\"\n  }\n}\n";
    let seen = states(
        &[("package.json", package)],
        json!({"matchString": ["\"x\""], "contextLines": {"before":0,"after":0}}),
    );
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0]["target"]["line"], 15);
    let key = seen[0]["target"]["key"].as_str().unwrap_or_default();
    assert!(key.starts_with("devDependencies"), "{seen:?}");
}

/// The judge reads a hit's window at its own lines: when `testCode` leaves
/// a neighbouring match line out, the window starts or ends where the
/// search's value does, not at the requested reach.
#[test]
fn a_window_beside_a_left_out_test_line_is_judged_at_its_own_lines() {
    let source = "#[cfg(test)]\nmod tests {\n    // marker t\n}\n// marker p\nl6\nl7\nl8\n";
    let context = json!({"before": 2, "after": 2});
    let excluded = states(
        &[("a.rs", source)],
        json!({"matchString": ["marker"], "contextLines": context, "testCode": "exclude"}),
    );
    assert_eq!(excluded.len(), 1, "{excluded:?}");
    assert_eq!(
        excluded[0]["lines"],
        json!([[4, "}"], [5, "// marker p"], [6, "l6"], [7, "l7"]]),
        "{excluded:?}"
    );
    assert_eq!(excluded[0]["target"]["text"], "// marker p");
    let only = states(
        &[("a.rs", source)],
        json!({"matchString": ["marker"], "contextLines": context, "testCode": "only"}),
    );
    assert_eq!(only.len(), 1, "{only:?}");
    assert_eq!(
        only[0]["lines"],
        json!([
            [1, "#[cfg(test)]"],
            [2, "mod tests {"],
            [3, "    // marker t"],
            [4, "}"]
        ]),
        "{only:?}"
    );
    assert_eq!(only[0]["target"]["text"], "    // marker t");
}

/// Rows keep their source order whatever order paging ranks them in, so a
/// hit's ordinal does not move between a page and its replay.
#[test]
fn hits_are_ordered_by_source_position() {
    let hit = |line| octocode_engine::types::TextSearchMatch {
        line,
        first_line: line,
        last_line: None,
        column: 0,
        byte_offset: 0,
        value: String::new(),
        deferred: None,
        value_digest: [0; 32],
        count: None,
        rank: None,
    };
    let file = TextSearchFile {
        term_counts: Vec::new(),
        path: "a.rs".into(),
        match_count: 4,
        matches: vec![hit(20), hit(1), hit(2), hit(9)],
        source: None,
    };
    assert_eq!(source_order(&file), [1, 2, 3, 0]);
}
