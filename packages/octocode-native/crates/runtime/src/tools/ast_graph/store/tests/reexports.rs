//! Re-export paths preserve exact identities without admitting ambiguous calls.
use super::*;

#[test]
fn reexport_callers_keep_high_confidence_across_aliases_and_stars() {
    let dir = git_app(&[
        ("impl.ts", "export function validate() { return 1; }\n"),
        ("named.ts", "export { validate as check } from './impl';\n"),
        ("barrel.ts", "export * from './named';\n"),
        (
            "use.ts",
            "import { check as verify } from './barrel';\nexport function run() { return verify(); }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "callers", Some("impl.ts#validate"), |o| {
        o.confidence = Some("high".into());
    });
    assert_eq!(ids(&out), vec!["use.ts#run"], "{}", out.value);
    assert_eq!(out.value["rows"][0]["confidence"], "high");
    assert_eq!(out.value["confidence"], "high");
}

#[test]
fn stats_call_ranking_excludes_global_name_guesses() {
    let dir = git_app(&[
        ("impl.ts", "export function uniqueTarget() { return 1; }\n"),
        (
            "linked.ts",
            "import { uniqueTarget } from './impl';\nexport function run() { return uniqueTarget(); }\n",
        ),
        (
            "guessed.ts",
            "export function guess() { return uniqueTarget(); }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let stats = ask(dir.0.as_path(), "stats", None, |_| {});
    assert_eq!(stats.value["callConfidence"], "high");
    let rows = stats.value["mostCalledSymbols"]
        .as_array()
        .expect("ranked calls");
    let target = rows
        .iter()
        .find(|r| r["id"] == "impl.ts#uniqueTarget")
        .expect("target");
    assert_eq!(target["fanIn"], 1, "{}", stats.value);
}

#[test]
fn explicit_reexports_override_stars_but_ambiguous_stars_stay_candidates() {
    let dir = git_app(&[
        ("a.ts", "export function check() { return 1; }\n"),
        ("b.ts", "export function check() { return 2; }\n"),
        (
            "explicit.ts",
            "export { check } from './a';\nexport * from './b';\n",
        ),
        (
            "ambiguous.ts",
            "export * from './a';\nexport * from './b';\n",
        ),
        (
            "use.ts",
            "import { check } from './explicit';\nexport function run() { return check(); }\n",
        ),
        (
            "maybe.ts",
            "import { check } from './ambiguous';\nexport function run() { return check(); }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let high = |target| {
        ask(dir.0.as_path(), "callers", Some(target), |o| {
            o.confidence = Some("high".into());
        })
    };
    assert_eq!(ids(&high("a.ts#check")), vec!["use.ts#run"]);
    assert!(ids(&high("b.ts#check")).is_empty());
    let medium = ask(dir.0.as_path(), "callers", Some("b.ts#check"), |o| {
        o.confidence = Some("medium".into());
    });
    assert_eq!(ids(&medium), vec!["maybe.ts#run"], "{}", medium.value);
}

#[test]
fn long_reexport_chains_terminate_on_cycles_without_losing_the_target() {
    let dir = git_app(&[
        ("impl.ts", "export function check() { return 1; }\n"),
        (
            "use.ts",
            "import { check } from './barrel0';\nexport function run() { return check(); }\n",
        ),
    ]);
    for index in 0..12 {
        let next = if index == 11 {
            "impl".into()
        } else {
            format!("barrel{}", index + 1)
        };
        std::fs::write(
            dir.0.as_path().join(format!("app/barrel{index}.ts")),
            format!("export * from './{next}';\nexport * from './barrel0';\n"),
        )
        .expect("barrel");
    }
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "callers", Some("impl.ts#check"), |o| {
        o.confidence = Some("high".into());
    });
    assert_eq!(ids(&out), vec!["use.ts#run"], "{}", out.value);
}
