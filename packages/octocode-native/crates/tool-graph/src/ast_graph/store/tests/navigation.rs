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
        Some("src/util/parse.ts#helper"),
        |o| {
            o.kind = Some("file".into());
            o.depth = Some(3);
        },
    );
    assert_ne!(out.value["errorCode"], "notFound", "{}", out.value);
    assert_eq!(out.class, ExitClass::Empty, "{}", out.value);
    assert_eq!(
        out.value["target"]["id"], "src/util/parse.ts#helper",
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
