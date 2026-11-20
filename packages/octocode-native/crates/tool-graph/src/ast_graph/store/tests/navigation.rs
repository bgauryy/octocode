//! Traversal targets and empty answers: `kind` never hides the target, and
//! an empty call answer points to the weaker confidence that has rows.
use super::*;

/// `app/src`: `util/parse.ts` (`parse` calls `helper`), `app.ts` (imports
/// and calls `parse`), `bare.ts` (`distinctiveName`) and `callsite.ts`
/// (calls `distinctiveName` bare, without an import).
fn navigation_fixture() -> Fixture {
    let dir = fixture();
    write_under(
        &dir,
        "app/src",
        &[
            (
                "util/parse.ts",
                "export function helper(x: string) { return x; }\nexport function parse(t: string) { return helper(t); }\n",
            ),
            (
                "app.ts",
                "import { parse } from './util/parse';\nexport function run() { return parse('x'); }\n",
            ),
            (
                "bare.ts",
                "export function distinctiveName() { return 1; }\n",
            ),
            (
                "callsite.ts",
                "export function callsBare() { return distinctiveName(); }\n",
            ),
        ],
    );
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    dir
}

fn hints(out: &GraphOutput) -> String {
    out.value["hints"].to_string()
}

/// `kind` narrows the rows a traversal lists, not the target: a symbol
/// target under `kind:"file"` is found, and the empty answer says that
/// `kind` excluded the rows the walk reached.
#[test]
fn kind_filters_traversal_rows_not_the_target() {
    let dir = navigation_fixture();
    let out = ask(
        dir.0.as_path(),
        "callers",
        Some("app/src/util/parse.ts#helper"),
        |o| {
            o.kind = Some("file".into());
            o.depth = Some(3);
        },
    );
    assert_ne!(out.value["errorCode"], "notFound", "{}", out.value);
    assert_eq!(out.class, ExitClass::Empty, "{}", out.value);
    assert_eq!(
        out.value["target"]["id"], "app/src/util/parse.ts#helper",
        "{}",
        out.value
    );
    assert!(hints(&out).contains("kind"), "{}", out.value);
}

/// An empty call answer at the default confidence names the weaker
/// confidence that lists rows, with the call that lists them.
#[test]
fn an_empty_call_answer_points_to_the_weaker_confidence_with_rows() {
    let dir = navigation_fixture();
    let target = "src/bare.ts#distinctiveName";
    let medium = ask(dir.0.as_path(), "callers", Some(target), |o| {
        o.confidence = Some("medium".into());
    });
    assert!(
        !ids(&medium).is_empty(),
        "fixture: a medium caller: {}",
        medium.value
    );
    let high = ask(dir.0.as_path(), "callers", Some(target), |_| {});
    assert_eq!(high.class, ExitClass::Empty, "{}", high.value);
    assert!(hints(&high).contains("medium"), "{}", high.value);
    let related = high.value["related"].as_array().expect("related calls");
    assert!(
        related.iter().any(|call| {
            let row = &call["input"]["queries"][0];
            row["operation"] == "callers"
                && row["confidence"] == "medium"
                && row["target"] == target
        }),
        "{}",
        high.value
    );
}

/// A walk with direction both states, on each row, which way its edge
/// runs: `in` rows are the target's callers, `out` rows its callees, as
/// the one-direction walks list them.
#[test]
fn a_walk_both_ways_states_each_rows_direction() {
    let dir = navigation_fixture();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let target = Some("src/util/parse.ts#parse");
    let calls = |o: &mut QueryOptions| o.edges = vec!["calls".into()];
    let both = ask(root, "walk", target, |o| {
        calls(o);
        o.direction = Some("both".into());
        o.depth = Some(1);
    });
    let rows = both.value["rows"].as_array().expect("rows");
    let callers = ids(&ask(root, "callers", target, calls));
    let callees = ids(&ask(root, "callees", target, calls));
    let (mut incoming, mut outgoing) = (Vec::new(), Vec::new());
    for row in rows {
        let id = row["id"].as_str().expect("id").to_owned();
        match row["direction"].as_str() {
            Some("in") => incoming.push(id),
            Some("out") => outgoing.push(id),
            other => panic!("direction {other:?}: {row}"),
        }
    }
    assert!(
        !incoming.is_empty() && !outgoing.is_empty(),
        "{}",
        both.value
    );
    incoming.sort();
    outgoing.sort();
    let (mut callers, mut callees) = (callers, callees);
    callers.sort();
    callees.sort();
    assert_eq!(incoming, callers, "{}", both.value);
    assert_eq!(outgoing, callees, "{}", both.value);
    // A one-direction walk's rows need no direction: the request states it.
    let out = ask(root, "walk", target, |o| {
        calls(o);
        o.depth = Some(1);
    });
    assert!(
        out.value["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|row| row.get("direction").is_none()),
        "{}",
        out.value
    );
}

/// A scoped snapshot's answers name every node from one root, the
/// workspace's: a symbol id's file as its file's id, so ids never mix the
/// scope's keys with workspace paths. A target reads either spelling, the
/// ids an answer states and the scope's own keys alike.
#[test]
fn a_scoped_answer_names_files_and_symbols_from_the_workspace() {
    let dir = navigation_fixture();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let calls = |o: &mut QueryOptions| o.edges = vec!["calls".into()];
    let scoped = ask(root, "callers", Some("src/util/parse.ts#helper"), calls);
    let target = scoped.value["target"]["id"].as_str().expect("target id");
    assert_eq!(target, "app/src/util/parse.ts#helper", "{}", scoped.value);
    let rows = scoped.value["rows"].as_array().expect("rows");
    assert!(!rows.is_empty(), "{}", scoped.value);
    for row in rows {
        let id = row["id"].as_str().expect("row id");
        let path = row["path"].as_str().expect("row path");
        assert!(id.starts_with(&format!("{path}#")), "{row}");
        assert!(path.starts_with("app/"), "{row}");
    }
    let restated = ask(root, "callers", Some(target), calls);
    assert_eq!(restated.value["rows"], scoped.value["rows"]);
}
