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
    let out = ask(
        dir.0.as_path(),
        "callers",
        Some("app/impl.ts#validate"),
        |o| {
            o.confidence = Some("high".into());
        },
    );
    assert_eq!(ids(&out), vec!["app/use.ts#run"], "{}", out.value);
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
    let stats = ask(dir.0.as_path(), "hubs", None, |o| {
        o.edges = vec!["calls".into()];
        o.confidence = Some("high".into());
    });
    let rows = stats.value["rows"].as_array().expect("ranked calls");
    let target = rows
        .iter()
        .find(|r| r["id"] == "app/impl.ts#uniqueTarget")
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
    assert_eq!(ids(&high("a.ts#check")), vec!["app/use.ts#run"]);
    assert!(ids(&high("app/b.ts#check")).is_empty());
    let medium = ask(dir.0.as_path(), "callers", Some("app/b.ts#check"), |o| {
        o.confidence = Some("medium".into());
    });
    assert_eq!(ids(&medium), vec!["app/maybe.ts#run"], "{}", medium.value);
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
    let out = ask(dir.0.as_path(), "callers", Some("app/impl.ts#check"), |o| {
        o.confidence = Some("high".into());
    });
    assert_eq!(ids(&out), vec!["app/use.ts#run"], "{}", out.value);
}

/// A module that reaches a re-exported item through `use super::*` and
/// names it is a dependent of the item's file, also when the glob passes
/// through a parent that only globs its own parent, and so is one that
/// names it by a path through the re-export (`super::Saved`); a glob
/// importer that names nothing of it is not.
#[test]
fn rust_glob_users_of_a_reexport_are_dependents_through_a_glob_chain() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "mod store;\n"),
        (
            "src/store.rs",
            "mod idle;\nmod keep;\nmod opts;\nmod saved;\nmod tests;\nmod tools;\npub use saved::Saved;\n",
        ),
        (
            "src/store/saved.rs",
            "pub struct Saved {}\nimpl Saved {\n    pub fn new() -> Self {\n        Saved {}\n    }\n}\n",
        ),
        (
            "src/store/tools.rs",
            "use super::*;\npub fn make() {\n    let _ = Saved::new();\n}\n",
        ),
        (
            "src/store/tests.rs",
            "use super::*;\nmod deep;\nmod rows;\n",
        ),
        (
            "src/store/tests/rows.rs",
            "use super::*;\npub fn retain() {\n    let _ = Box::new(Saved::new());\n}\n",
        ),
        ("src/store/idle.rs", "use super::*;\npub fn idle() {}\n"),
        (
            "src/store/keep.rs",
            "use super::*;\npub fn keep(_rows: Option<Saved>) {}\n",
        ),
        (
            "src/store/opts.rs",
            "pub struct Opts {\n    pub rows: Option<super::Saved>,\n}\n",
        ),
        ("src/store/tests/deep.rs", "use super::*;\nmod lit;\n"),
        (
            "src/store/tests/deep/lit.rs",
            "use super::*;\npub fn lit() {\n    let _ = Saved {};\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(
        dir.0.as_path(),
        "dependents",
        Some("app/src/store/saved.rs"),
        |_| {},
    );
    let listed = ids(&out);
    for user in [
        "app/src/store.rs",
        "app/src/store/tools.rs",
        "app/src/store/tests/rows.rs",
        "app/src/store/keep.rs",
        "app/src/store/tests/deep/lit.rs",
        "app/src/store/opts.rs",
    ] {
        assert!(listed.contains(&user.to_owned()), "{user}: {}", out.value);
    }
    for bystander in ["app/src/store/idle.rs", "app/src/store/tests.rs"] {
        assert!(
            !listed.contains(&bystander.to_owned()),
            "{bystander}: {}",
            out.value
        );
    }
}
