//! A file past a limit of its own source map is disclosed by path and limit
//! with a call that answers for part of it; the rest of the scope answers.
use super::*;
use serde_json::Value;

/// A file with more identifiers than one file's source map holds segments.
fn oversized(root: &Fixture) {
    let limit = octocode_engine::text::analysis::MapLimits::default().segments;
    root.write("big.ts", "a;\n".repeat(limit + 1));
}

fn identifiers(path: &std::path::Path) -> Value {
    json!({"operation":"match","path":path,"parser":"typescript","rule":{"kind":"identifier"}})
}

fn narrowed(out: &Value) -> Value {
    out["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|lead| lead["tool"] == "astSearch")
        .map_or(Value::Null, |lead| lead["input"]["queries"][0].clone())
}

#[test]
fn a_directory_scan_skips_the_oversized_file_and_answers_the_rest() {
    let root = Fixture::new();
    oversized(&root);
    root.write("small.ts", "b;\n");
    let out = run(&root.0, identifiers(&root.0)).expect("the scope completes");
    assert_eq!(out["matchCount"], 1, "{out}");
    assert!(out["files"].to_string().contains("small.ts"), "{out}");
    let limit = octocode_engine::text::analysis::MapLimits::default().segments;
    let diagnostics = out["diagnostics"].to_string();
    assert!(diagnostics.contains("big.ts"), "{out}");
    assert!(diagnostics.contains(&limit.to_string()), "{out}");
    assert!(!diagnostics.contains("source correspondence"), "{out}");
    assert!(out["isPartial"] == true, "{out}");
    let narrow = narrowed(&out);
    assert!(
        narrow["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("big.ts")),
        "{out}"
    );
    let part = run(&root.0, narrow).expect("the narrower call answers");
    assert!(part["matchCount"].as_u64().unwrap() > 0, "{part}");
}

#[test]
fn a_requested_oversized_file_is_refused_by_path_and_limit_with_a_call_that_answers() {
    let root = Fixture::new();
    oversized(&root);
    let file = root.0.join("big.ts");
    let error = run(&root.0, identifiers(&file)).expect_err("the file passes its limit");
    assert_eq!(error.code, ErrorCode::ScopeTooBroad);
    let limit = octocode_engine::text::analysis::MapLimits::default().segments;
    assert!(error.message.contains("big.ts"), "{}", error.message);
    assert!(
        error.message.contains(&limit.to_string()),
        "{}",
        error.message
    );
    assert!(
        !error.message.contains("source correspondence"),
        "{}",
        error.message
    );
    let calls = error.calls.expect("a narrower call");
    let narrow = calls["narrowSearch"]["input"]["queries"][0].clone();
    let part = run(&root.0, narrow).expect("the narrower call answers");
    assert!(part["matchCount"].as_u64().unwrap() > 0, "{part}");
}

/// A file whose halves still pass its limit: following the leads halves the
/// searched range each time and covers every identifier exactly once.
#[test]
fn following_the_narrow_leads_reaches_every_line_once_when_one_halving_is_not_enough() {
    let root = Fixture::new();
    let limit = octocode_engine::text::analysis::MapLimits::default().segments;
    // Three identifiers a line: each half of the file still passes the limit.
    let lines = limit / 2 + 1;
    root.write("big.ts", "a; b; c;\n".repeat(lines));
    let mut pending = vec![identifiers(&root.0.join("big.ts"))];
    let (mut matches, mut calls) = (0, 0);
    while let Some(query) = pending.pop() {
        calls += 1;
        assert!(calls < 64, "the leads do not converge");
        match run(&root.0, query.clone()) {
            Ok(part) => matches += part["matchCount"].as_u64().unwrap(),
            Err(error) => {
                assert_eq!(error.code, ErrorCode::ScopeTooBroad);
                let leads = error.calls.expect("narrower calls");
                for name in ["narrowSearch", "narrowRest"] {
                    let next = leads[name]["input"]["queries"][0].clone();
                    assert_ne!(next, query, "{name} repeats the refused call");
                    pending.push(next);
                }
            }
        }
    }
    assert_eq!(matches, 3 * lines as u64, "every identifier once");
}
