use super::*;
use octocode_test_support::json_fixture;

fn keys(data: &Value) -> Vec<String> {
    let mut units = page_units(data, &json!({}));
    distinct(&mut units);
    units.into_iter().map(|unit| unit.key).collect()
}

/// A search page: each hit of each file, named `path:line`; the file's
/// other lists (its declarations) are context, not units.
#[test]
fn search_hits_are_units_named_path_and_line() {
    let data = json_fixture(
        r#"{
            "matchCount": 3,
            "files": [
                {
                    "path": "src/a.rs",
                    "declarations": [{"symbolName": "f", "line": 1}],
                    "matches": [
                        {"line": 49, "value": "x"},
                        {"line": 125, "value": "y"},
                        {"line": 125, "value": "z"}
                    ]
                },
                {"path": "src/b.rs", "matches": [{"line": 7, "value": "w"}]}
            ],
            "related": [{"kind": "related"}]
        }"#,
    );
    assert_eq!(
        keys(&data),
        [
            "src/a.rs:49",
            "src/a.rs:125",
            "src/a.rs:125#2",
            "src/b.rs:7"
        ]
    );
    let units = page_units(&data, &json!({}));
    assert_eq!(
        units[0].state,
        Some(json_fixture(
            r#"{"path": "src/a.rs", "target": {"line": 49, "text": "x"}}"#
        ))
    );
}

fn units_of(data: &Value, query: &Value) -> Vec<Unit> {
    let mut units = page_units(data, query);
    distinct(&mut units);
    units
}

fn line_of(key: &str) -> u64 {
    key.rsplit_once(':')
        .and_then(|(_, line)| line.parse().ok())
        .expect("path:line key")
}

/// A merged context block (localSearch joins overlapping windows into one
/// row listing every hit in `matchedLines`): each hit is its own unit,
/// keyed `path:line`, its target that line, its window the block's lines
/// within the query's contextLines of it.
#[test]
fn merged_context_windows_are_one_unit_per_hit_each_with_its_own_window() {
    let (before, after) = (2_u64, 1_u64);
    let listed = (2..=11_u64).collect::<Vec<_>>();
    let text = |line: u64| format!("    step({line});");
    let value = listed
        .iter()
        .map(|&line| format!("{line}\t{}", text(line)))
        .collect::<Vec<_>>()
        .join("\n");
    let hits = [4_u64, 7, 10];
    let data = json!({"files":[{"path":"src/walk.rs",
        "declarations":[{"symbolName":"walk","kind":"function","line":1,"endLine":12}],
        "matches":[{"line":4,"value":value,"matchedLines":hits,"enclosing":0}]}]});
    let query = json!({"path":"src","matchString":["step"],
        "contextLines":{"before":before,"after":after}});
    let units = units_of(&data, &query);
    assert_eq!(
        units
            .iter()
            .map(|unit| unit.key.clone())
            .collect::<Vec<_>>(),
        hits.iter()
            .map(|line| format!("src/walk.rs:{line}"))
            .collect::<Vec<_>>()
    );
    for unit in &units {
        let hit = line_of(&unit.key);
        let state = unit.state.as_ref().expect("judged");
        assert_eq!(state["target"]["line"], hit, "{state}");
        assert_eq!(state["target"]["text"], text(hit), "{state}");
        let window = state["lines"]
            .as_array()
            .expect("window")
            .iter()
            .map(|entry| entry[0].as_u64().expect("line"))
            .collect::<Vec<_>>();
        let reach = listed
            .iter()
            .copied()
            .filter(|&line| line + before >= hit && line <= hit + after)
            .collect::<Vec<_>>();
        assert_eq!(window, reach, "{state}");
        for entry in state["lines"].as_array().into_iter().flatten() {
            assert_eq!(entry[1], text(entry[0].as_u64().unwrap()), "{state}");
        }
    }
}

/// A hit's judged text names which line is the hit and the declaration
/// the row says holds it (the innermost one), so a question about scope
/// is answerable from the unit; with no context its window is the target.
#[test]
fn a_hit_unit_marks_its_line_and_carries_its_enclosing_declaration() {
    let declarations = json_fixture(
        r#"[
            {
                "symbolName": "Walker",
                "kind": "impl",
                "line": 1,
                "endLine": 20
            },
            {
                "symbolName": "walk",
                "kind": "method",
                "line": 3,
                "endLine": 12
            }
        ]"#,
    );
    let data = json!({"files":[{"path":"src/walk.rs","declarations":declarations,
        "matches":[{"line":5,"value":"        walk(next)","enclosing":1},
                   {"line":15,"value":"    fn other() {}","enclosing":0}]}]});
    let units = units_of(
        &data,
        &json_fixture(r#"{"path": "src", "matchString": ["walk"]}"#),
    );
    let states = units
        .iter()
        .map(|unit| unit.state.clone().expect("judged"))
        .collect::<Vec<_>>();
    assert_eq!(states[0]["path"], "src/walk.rs");
    assert_eq!(states[0]["target"]["line"], 5);
    assert_eq!(states[0]["target"]["text"], "        walk(next)");
    assert_eq!(states[0]["target"]["enclosing"], declarations[1]);
    assert_eq!(states[1]["target"]["enclosing"], declarations[0]);
    assert!(states.iter().all(|state| state.get("lines").is_none()));
    // What is sent scopes every question to the target, with the
    // contract's text; a unit without a target is sent as it is.
    let sent = sent_state(&units[0], Some("task")).expect("judged");
    assert_eq!(
        Some(&sent["question"]),
        octocode_tools_shared::selection::section_question()
    );
    assert_eq!(sent["content"], states[0]);
    let item = &item_units(&[json!("a")])[0];
    assert_eq!(
        sent_state(item, None).expect("judged").as_ref(),
        &json!("a")
    );
}

/// A GitHub code row lists one entry per line and its hits in the row's
/// `matchedLines`: the other entries are the hits' context, not units.
#[test]
fn listed_context_lines_are_windows_not_units() {
    let data = json_fixture(
        r#"{
            "files": [
                {
                    "path": "src/x.rs",
                    "matchedLines": [14],
                    "matches": [
                        {"line": 13, "value": "a"},
                        {"line": 14, "value": "needle"},
                        {"line": 15, "value": "c"}
                    ]
                }
            ]
        }"#,
    );
    let query = json_fixture(
        r#"{
            "keywords": ["needle"],
            "contextLines": {"before": 1, "after": 1}
        }"#,
    );
    let units = units_of(&data, &query);
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].key, "src/x.rs:14");
    let state = units[0].state.as_ref().expect("judged");
    assert_eq!(state["target"]["text"], "needle");
    assert_eq!(
        state["lines"],
        json_fixture(r#"[[13, "a"], [14, "needle"], [15, "c"]]"#)
    );
}

/// A keyed file's row names each hit line's key path (`matchKeys`, the
/// wire's `[{line, keyPath}]`): the hit's target carries its own.
#[test]
fn a_hit_in_a_keyed_file_carries_its_key_path() {
    let data = json_fixture(
        r#"{
            "files": [
                {
                    "path": "package.json",
                    "matchKeys": [
                        {"line": 2, "keyPath": "name"},
                        {"line": 9, "keyPath": "scripts.build"}
                    ],
                    "matches": [
                        {"line": 2, "value": "  \"name\": \"x\","},
                        {"line": 9, "value": "    \"build\": \"x\""}
                    ]
                }
            ]
        }"#,
    );
    let units = units_of(
        &data,
        &json_fixture(r#"{"path": ".", "matchString": ["x"]}"#),
    );
    let keys = &data["files"][0]["matchKeys"];
    for (unit, key) in units.iter().zip(keys.as_array().expect("keys")) {
        let target = &unit.state.as_ref().expect("judged")["target"];
        assert_eq!(target["line"], key["line"], "{target}");
        assert_eq!(target["key"], key["keyPath"], "{target}");
    }
    assert_eq!(units.len(), 2);
}

/// A match over several lines states its extent in `endLine`: one hit,
/// its target those lines. `matchedLines` are always separate hits (of a
/// merged context window), whatever the query's `multiline`.
#[test]
fn a_match_extent_comes_from_the_row_not_the_query() {
    let multiline = json_fixture(
        r#"{
            "path": ".",
            "matchString": ["x|fold\\(\\n.*"],
            "multiline": "on"
        }"#,
    );
    let data = json_fixture(
        r#"{
            "files": [
                {
                    "path": "a.rs",
                    "matches": [{"line": 20, "value": "fold(\n    init,", "endLine": 21}]
                }
            ]
        }"#,
    );
    let units = units_of(&data, &multiline);
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].key, "a.rs:20");
    let target = &units[0].state.as_ref().expect("judged")["target"];
    assert_eq!(target["lastLine"], 21);
    assert_eq!(target["text"], "fold(\n    init,");

    let data = json_fixture(
        r#"{
            "files": [
                {
                    "path": "a.rs",
                    "matches": [
                        {
                            "line": 21,
                            "value": "20\tfold(\n21\tx\n22\tx",
                            "matchedLines": [21, 22]
                        }
                    ]
                }
            ]
        }"#,
    );
    let units = units_of(&data, &multiline);
    assert_eq!(
        units
            .iter()
            .map(|unit| unit.key.as_str())
            .collect::<Vec<_>>(),
        ["a.rs:21", "a.rs:22"]
    );
    assert!(units.iter().all(|unit| {
        unit.state.as_ref().expect("judged")["target"]
            .get("lastLine")
            .is_none()
    }));
}

/// A page that lists files without hits (a files-only view): each file.
#[test]
fn rows_without_child_rows_are_the_units() {
    assert_eq!(
        keys(&json_fixture(
            r#"{
                "files": [{"path": "a.rs", "matchCount": 3}, {"path": "b.rs"}]
            }"#
        )),
        ["a.rs", "b.rs"]
    );
    assert_eq!(
        keys(&json_fixture(
            r#"{
                "repositories": [{"owner": "o", "repo": "r", "topics": ["x"]}]
            }"#
        )),
        ["o/r"]
    );
    assert_eq!(
        keys(&json_fixture(
            r#"{"artifacts": [{"packageName": "zod", "version": "4"}]}"#
        )),
        ["zod"]
    );
    assert_eq!(
        keys(&json_fixture(
            r#"{
                "commits": [{"commitSha": "abc", "messageHeadline": "fix"}],
                "pullRequests": [{"number": 12, "title": "t"}],
                "issues": [{"number": 7}]
            }"#
        )),
        ["abc", "12", "7"]
    );
    assert_eq!(
        keys(&json_fixture(
            r#"{
                "symbols": [
                    {"symbolName": "run", "line": 4},
                    {"symbolName": "stop", "line": 9}
                ]
            }"#
        )),
        ["run:4", "stop:9"]
    );
}

/// A directory listing: each entry under its directory, by path.
#[test]
fn listing_entries_are_units_named_by_path() {
    let data = json_fixture(
        r#"{
            "files": [
                {"dir": "src", "files": ["a.rs (10)", "sub/"]},
                {"path": "top.md"}
            ]
        }"#,
    );
    assert_eq!(keys(&data), ["src/a.rs", "src/sub/", "top.md"]);
}

/// A pull request read: each discussion entry and changed file, under
/// the pull request's number.
#[test]
fn discussion_entries_and_changed_files_are_units() {
    let data = json_fixture(
        r#"{
            "pullRequests": [
                {
                    "number": 42,
                    "title": "t",
                    "body": "long",
                    "comments": [{"id": 1, "body": "a"}, {"id": 2, "body": "b"}],
                    "reviews": [{"id": 9, "body": "c"}],
                    "reviewComments": [{"id": 5, "path": "src/x.rs", "body": "d"}],
                    "files": [{"path": "src/x.rs", "patch": "@@"}]
                }
            ]
        }"#,
    );
    assert_eq!(keys(&data), ["42:src/x.rs", "42:1", "42:2", "42:9", "42:5"]);
    // The pull request's body is not repeated to each entry's judge.
    let units = page_units(&data, &json!({}));
    assert_eq!(
        units[1].state,
        Some(json_fixture(
            r#"{"of": "42", "comments": {"id": 1, "body": "a"}}"#
        ))
    );
}

/// A credential-named file is never shown to the judge.
#[test]
fn a_credential_named_unit_is_withheld() {
    let data = json_fixture(
        r#"{
            "files": [
                {"path": ".env", "matches": [{"line": 1, "value": "KEY=1"}]},
                {"dir": "cfg", "files": [".env.local", "app.toml"]}
            ]
        }"#,
    );
    let units = page_units(&data, &json!({}));
    let withheld: Vec<&str> = units
        .iter()
        .filter(|unit| unit.state.is_none())
        .map(|unit| unit.key.as_str())
        .collect();
    assert_eq!(withheld, [".env:1", "cfg/.env.local"]);
}

#[test]
fn content_items_are_units_by_index_and_a_blank_one_is_withheld() {
    let units = item_units(&[json!("a"), json!(""), json_fixture(r#"{"k": 1}"#)]);
    assert_eq!(
        units
            .iter()
            .map(|unit| unit.key.as_str())
            .collect::<Vec<_>>(),
        ["0", "1", "2"]
    );
    assert!(units[1].state.is_none());
}

/// A line-number gutter is leading digits and a separator; any other
/// character after the digits, multibyte ones included, is no gutter.
#[test]
fn a_gutter_is_leading_digits_and_a_separator() {
    assert_eq!(gutter("12\tlet x"), Some("12".to_owned()));
    assert_eq!(gutter("7: λ"), Some("7".to_owned()));
    assert_eq!(gutter("12λ"), None);
    assert_eq!(gutter("λ12\t"), None);
    assert_eq!(gutter("123"), None);
    assert_eq!(gutter(""), None);
}

#[test]
fn only_file_readers_are_not_row_tools() {
    assert_eq!(
        rows_tool(&json_fixture(r#"{"tool": "localSearch", "query": {}}"#)),
        Some(ToolId::LocalSearch)
    );
    assert_eq!(
        rows_tool(&json_fixture(r#"{"tool": "localFetch", "query": {}}"#)),
        None
    );
    assert_eq!(
        rows_tool(&json_fixture(r#"{"url": "https://example.com"}"#)),
        None
    );
}

mod source_progress;
