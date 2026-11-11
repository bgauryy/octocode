//! Cycle and dead-code detectors judged on one fixture each.
use super::*;

mod health;
mod liveness;
mod module_roots;
mod scope_notes;

/// One fixture per edge class a cycle can run through: a runtime import
/// loop, a type-only one, one closed by a dynamic `import()`, one closed by
/// a Python import inside a function (it runs on call), a Rust `use` loop,
/// and a Rust module tree (a parent calling its child, the child using
/// `super::`), which is no dependency cycle.
fn cycle_classes_app() -> Fixture {
    git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod ra;\npub mod rb;\npub mod tree;\n"),
        (
            "src/ra.rs",
            "use crate::rb::bee;\npub fn ay() -> u8 { bee() }\n",
        ),
        (
            "src/rb.rs",
            "use crate::ra::ay;\npub fn bee() -> u8 { ay() }\n",
        ),
        (
            "src/tree/mod.rs",
            "pub mod child;\npub fn top() -> u8 { child::leaf() }\n",
        ),
        (
            "src/tree/child.rs",
            "use super::top;\npub fn leaf() -> u8 { 1 }\npub fn up() -> u8 { top() }\n",
        ),
        (
            "web/a.ts",
            "import { b } from './b';\nexport const a = () => b();\n",
        ),
        (
            "web/b.ts",
            "import { a } from './a';\nexport const b = () => a();\n",
        ),
        (
            "web/t1.ts",
            "import type { T2 } from './t2';\nexport type T1 = { next: T2 };\n",
        ),
        (
            "web/t2.ts",
            "import type { T1 } from './t1';\nexport type T2 = { prev: T1 };\n",
        ),
        (
            "web/d1.ts",
            "import { d2 } from './d2';\nexport const d1 = () => d2();\n",
        ),
        (
            "web/d2.ts",
            "export const d2 = () => import('./d1').then((m) => m.d1);\n",
        ),
        (
            "p1.py",
            "from p2 import two\n\ndef one():\n    return two()\n",
        ),
        (
            "p2.py",
            "def two():\n    from p1 import one\n    return one\n",
        ),
    ])
}

/// Cycle groups as sorted member lists, with whether each is a runtime
/// (module-load) cycle.
type Groups = std::collections::BTreeSet<(Vec<String>, bool)>;

/// The `cycles` operation and the `cycle` detector of `issues`
/// read one cycle engine: the same groups, the same runtime verdicts.
#[test]
fn every_cycle_surface_reports_the_same_groups() {
    let dir = cycle_classes_app();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let group = |files: &[&str], runtime: bool| {
        (
            files.iter().map(|f| (*f).to_owned()).collect::<Vec<_>>(),
            runtime,
        )
    };
    let expected = Groups::from([
        group(&["p1.py", "p2.py"], false),
        group(&["src/ra.rs", "src/rb.rs"], false),
        group(&["web/a.ts", "web/b.ts"], true),
        group(&["web/d1.ts", "web/d2.ts"], false),
        group(&["web/t1.ts", "web/t2.ts"], false),
    ]);
    let listed = ask(root, "cycles", None, |_| {});
    let listed = listed.value["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let files = row["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|file| file.as_str().map(str::to_owned))
                .collect::<Vec<_>>();
            (files, row["runtime"] == true)
        })
        .collect::<Groups>();
    assert_eq!(listed, expected, "cycles");
    let issues = ask(root, "issues", None, |o| {
        o.detectors = vec!["cycle".into()];
        o.limit = Some(50);
    });
    let found = issues.value["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let mut files = row["evidence"]["members"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|file| file.as_str().map(str::to_owned))
                .collect::<Vec<_>>();
            files.sort();
            (files, row["evidence"]["runtime"] == true)
        })
        .collect::<Groups>();
    assert_eq!(found, expected, "issues: {}", issues.value);
    assert!(
        issues.value["summary"].get("detectorMs").is_none(),
        "{}",
        issues.value
    );
}

/// A TypeScript package for the dead-code detectors: an entry, a
/// used module with one unused export, an orphan file, a pair of files only
/// each other import, a barrel, a default export, a test, and declaration
/// files (one beside a used module, one nothing imports).
fn dead_code_app() -> Fixture {
    git_app(&[
        ("package.json", r#"{"name":"app","main":"src/main.ts"}"#),
        (
            "src/main.ts",
            "import { used } from './lib';\nimport run from './runner';\nimport { fromBarrel } from './barrel';\nexport const main = () => used() + run() + fromBarrel();\n",
        ),
        (
            "src/lib.ts",
            "export function used() { return 1; }\nexport function unused() { return 2; }\n",
        ),
        (
            "src/runner.ts",
            "export default function run() { return 3; }\n",
        ),
        ("src/barrel.ts", "export * from './inner';\n"),
        (
            "src/inner.ts",
            "export const fromBarrel = () => 4;\nexport const alsoExported = 5;\n",
        ),
        ("src/orphan.ts", "export const lonely = 1;\n"),
        (
            "src/ping.ts",
            "import { pong } from './pong';\nexport const ping = () => pong();\n",
        ),
        (
            "src/pong.ts",
            "import { ping } from './ping';\nexport const pong = () => ping();\n",
        ),
        (
            "src/lib.test.ts",
            "import { used } from './lib';\nused();\n",
        ),
        (
            "src/runner.d.ts",
            "export default function run(): number;\n",
        ),
        ("types/extra.d.mts", "export declare const extra: number;\n"),
    ])
}

/// The store's `unreachable-file` and `unused-export` detectors judge the
/// dead-code fixture: the orphan and the pair only each other import are
/// unreachable, and only the exports no importer names are unused.
#[test]
fn dead_code_detectors_judge_one_fixture() {
    let dir = dead_code_app();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = ask(root, "issues", None, |o| {
        o.limit = Some(200);
        o.min_tier = Some(0);
    });
    let rows = issues.value["rows"].as_array().cloned().unwrap_or_default();
    let store_unreachable = rows
        .iter()
        .filter(|row| row["detector"] == "unreachable-file")
        .filter_map(|row| row["subject"].as_str().map(str::to_owned))
        .collect::<std::collections::BTreeSet<_>>();
    let store_unused = rows
        .iter()
        .filter(|row| row["detector"] == "unused-export")
        .flat_map(|row| {
            let file = row["subject"].as_str().unwrap_or_default().to_owned();
            row["evidence"]["exports"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|name| name.as_str().map(|name| format!("{file}#{name}")))
                .collect::<Vec<_>>()
        })
        .collect::<std::collections::BTreeSet<_>>();
    let set = |items: &[&str]| {
        items
            .iter()
            .map(|item| (*item).to_owned())
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        store_unreachable,
        set(&["src/orphan.ts", "src/ping.ts", "src/pong.ts"]),
        "{}",
        issues.value
    );
    assert_eq!(
        store_unused,
        set(&["src/inner.ts#alsoExported", "src/lib.ts#unused"]),
        "{}",
        issues.value
    );
}

/// An export name of any length is text-scanned: a short name loaded
/// dynamically (`mod.run(...)`) is a text reference, not "no reference".
#[test]
fn short_export_names_are_text_checked() {
    let dir = fixture();
    dir.write(
        "app/src/main.ts",
        "import { other } from './cmd';\nexport function main() {\n  return other();\n}\n",
    );
    dir.write(
        "app/src/cmd.ts",
        "export function other() { return 1; }\nexport function run() { return 2; }\n",
    );
    dir.write(
        "app/src/loader.ts",
        "export async function load(file: string) {\n  const mod = await import(file);\n  return mod.run(1);\n}\n",
    );
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let all = ask(dir.0.as_path(), "issues", None, |o| {
        o.detectors = vec!["unused-export".into()];
        o.min_tier = Some(0);
        o.limit = Some(500);
    });
    let row = all.value["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["subject"] == "src/cmd.ts")
        .cloned()
        .unwrap_or_else(|| panic!("cmd.ts unused export: {}", all.value));
    assert_eq!(row["evidence"]["exports"], json!(["run"]), "{row}");
    assert_eq!(row["tier"], 60, "{row}");
    assert_eq!(
        row["evidence"]["mentionedIn"]["run"],
        json!(["src/loader.ts:3"]),
        "{row}"
    );
}

/// Prose naming an export is not a reference to it: an export mentioned
/// only in docs stays tier 90 with every prose mention as context, and an
/// export a loader reaches keeps that code reference in `mentionedIn`
/// however many docs sort before it.
#[test]
fn prose_mentions_never_hide_an_unused_export() {
    let dir = fixture();
    dir.write(
        "app/src/main.ts",
        "import { other } from './cmd';\nimport { helper } from './tool';\nexport function main() {\n  return other() + helper();\n}\n",
    );
    dir.write(
        "app/src/cmd.ts",
        "export function other() { return 1; }\nexport function run() { return 2; }\n",
    );
    dir.write(
        "app/src/tool.ts",
        "export function helper() { return 1; }\nexport function go() { return 2; }\n",
    );
    dir.write(
        "app/src/loader.ts",
        "export async function load(file: string) {\n  const mod = await import(file);\n  return mod.go(1);\n}\n",
    );
    for doc in ["a/README.md", "b/NOTES.md", "c/OPS.md", "d/MORE.md"] {
        dir.write(
            &format!("app/docs/{doc}"),
            "# Usage\n\nThen run it, or go home.\n",
        );
    }
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let all = ask(dir.0.as_path(), "issues", None, |o| {
        o.detectors = vec!["unused-export".into()];
        o.min_tier = Some(0);
        o.limit = Some(500);
    });
    let row = |subject: &str| {
        all.value["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .find(|row| row["subject"] == subject)
            .cloned()
            .unwrap_or_else(|| panic!("{subject} unused export: {}", all.value))
    };
    let prose = json!([
        "docs/a/README.md:3",
        "docs/b/NOTES.md:3",
        "docs/c/OPS.md:3",
        "docs/d/MORE.md:3",
    ]);
    let cmd = row("src/cmd.ts");
    assert_eq!(cmd["evidence"]["exports"], json!(["run"]), "{cmd}");
    assert!(cmd["tier"].as_u64().is_some_and(|tier| tier >= 90), "{cmd}");
    assert!(cmd["evidence"].get("mentionedIn").is_none(), "{cmd}");
    assert_eq!(cmd["evidence"]["textMentions"]["run"], prose, "{cmd}");
    let tool = row("src/tool.ts");
    assert_eq!(tool["tier"], 60, "{tool}");
    assert_eq!(
        tool["evidence"]["mentionedIn"]["go"],
        json!(["src/loader.ts:3"]),
        "{tool}"
    );
    assert_eq!(tool["evidence"]["textMentions"]["go"], prose, "{tool}");
}

/// An export run that finds nothing says what it checked: how many exports,
/// and the rule that settled each, counted per name (the entry re-exports
/// one name of `api.ts`), with a `deps` lead to what the entry re-exports.
#[test]
fn an_empty_export_run_states_what_it_evaluated_and_leads_to_the_reexports() {
    let dir = git_app(&[
        ("package.json", r#"{"name":"lib","main":"./index.ts"}"#),
        (
            "index.ts",
            "export { visible } from './api';\nimport { shared } from './util';\nexport const total = shared;\n",
        ),
        ("api.ts", "export function visible() { return 1; }\n"),
        ("util.ts", "export const shared = 2;\n"),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let issues = ask(dir.0.as_path(), "issues", None, |o| {
        o.detectors = vec!["unused-export".into(), "export-only-local".into()];
        o.min_tier = Some(60);
    });
    let value = &issues.value;
    assert_eq!(value["rows"], json!([]), "{value}");
    let evaluated = value["summary"]["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|note| note.get("evaluated"))
        .unwrap_or_else(|| panic!("no evaluated note: {value}"));
    assert_eq!(evaluated["reexportedByEntry"], 1, "{value}");
    assert_eq!(evaluated["usedByAnotherFile"], 1, "{value}");
    assert_eq!(evaluated["entry"], 1, "{value}");
    assert_eq!(evaluated["exports"], 3, "{value}");
    assert!(evaluated.get("reexportingEntries").is_none(), "{value}");
    let lead = &value["related"][0]["input"]["queries"][0];
    assert_eq!(lead["operation"], "deps", "{value}");
    assert_eq!(lead["target"], "index.ts", "{value}");
}

/// The public API is per name: an entry's named, type and default
/// re-exports make only those names public, `export *` makes every name
/// of its module public, and the other exports of a re-exported file are
/// still judged. An export only a test imports is listed, as a hypothesis.
#[test]
fn only_the_names_an_entry_reexports_are_public() {
    let dir = git_app(&[
        ("package.json", r#"{"name":"lib","main":"./index.ts"}"#),
        (
            "index.ts",
            "export { visible, default as Widget } from './api';\nexport type { Shape } from './types';\nexport * from './all';\nexport { start } from './client';\n",
        ),
        (
            "api.ts",
            "export type Hidden = { a: number };\nexport function visible(h: Hidden) { return h.a; }\nexport function orphan() { return 1; }\nexport default function widget() { return 2; }\n",
        ),
        (
            "types.ts",
            "export type Shape = { s: string };\nexport type Unseen = number;\n",
        ),
        ("all.ts", "export const one = 1;\nexport const two = 2;\n"),
        (
            "client.ts",
            "export function connect() { return 3; }\nexport function start(c = connect) { return c(); }\n",
        ),
        (
            "client.test.ts",
            "import { connect } from './client';\nconnect();\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let issues = ask(dir.0.as_path(), "issues", None, |o| {
        o.detectors = vec!["unused-export".into(), "export-only-local".into()];
        o.min_tier = Some(0);
    });
    let value = &issues.value;
    let rows = value["rows"].as_array().expect("rows");
    let listed = |detector: &str, subject: &str| {
        rows.iter()
            .find(|row| row["detector"] == detector && row["subject"] == subject)
            .unwrap_or_else(|| panic!("{detector} {subject}: {value}"))
    };
    assert_eq!(
        listed("export-only-local", "api.ts")["evidence"]["exports"],
        json!(["Hidden"]),
        "{value}"
    );
    assert_eq!(
        listed("unused-export", "api.ts")["evidence"]["exports"],
        json!(["orphan"]),
        "{value}"
    );
    assert_eq!(
        listed("unused-export", "types.ts")["evidence"]["exports"],
        json!(["Unseen"]),
        "{value}"
    );
    let tested = listed("export-only-local", "client.ts");
    assert_eq!(tested["evidence"]["exports"], json!(["connect"]), "{value}");
    assert_eq!(tested["tier"], 60, "{value}");
    assert!(
        rows.iter().all(|row| row["subject"] != "all.ts"),
        "export * keeps every name public: {value}"
    );
    assert_eq!(rows.len(), 4, "{value}");
}

/// A file whose local-only exports are partly test-imported gives one
/// finding per evidence: the untested names keep their tier, the
/// test-imported names drop to tier 60 with the test control, so `minTier`
/// never shows a test-imported name as a confident finding.
#[test]
fn test_imported_exports_split_from_untested_ones_in_one_file() {
    let dir = git_app(&[
        ("package.json", r#"{"name":"lib","main":"./index.ts"}"#),
        ("index.ts", "import { run } from './client';\nrun();\n"),
        (
            "client.ts",
            "export function helper() { return 1; }\nexport function connect() { return helper(); }\nexport function run() { return connect(); }\nexport function quiet() { return 2; }\nexport function loud() { return quiet(); }\n",
        ),
        (
            "client.test.ts",
            "import { connect, quiet } from './client';\nconnect();\nquiet();\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let issues = ask(dir.0.as_path(), "issues", None, |o| {
        o.detectors = vec!["export-only-local".into()];
        o.min_tier = Some(0);
    });
    let value = &issues.value;
    let rows = value["rows"].as_array().expect("rows");
    let local = rows
        .iter()
        .filter(|row| row["subject"] == "client.ts")
        .collect::<Vec<_>>();
    assert_eq!(local.len(), 2, "{value}");
    let untested = local
        .iter()
        .find(|row| row["evidence"]["exports"] == json!(["helper"]))
        .unwrap_or_else(|| panic!("untested row: {value}"));
    let tested = local
        .iter()
        .find(|row| row["evidence"]["exports"] == json!(["connect", "quiet"]))
        .unwrap_or_else(|| panic!("tested row: {value}"));
    assert!(untested["tier"].as_u64() > Some(60), "{value}");
    assert_eq!(tested["tier"], 60, "{value}");
    let controls = |row: &Value| row["controls"].to_string();
    assert!(controls(tested).contains("a test imports it"), "{value}");
    assert!(!controls(untested).contains("a test imports it"), "{value}");

    let confident = ask(dir.0.as_path(), "issues", None, |o| {
        o.detectors = vec!["export-only-local".into()];
        o.min_tier = Some(untested["tier"].as_u64().unwrap_or(90) as u8);
    });
    let shown = confident.value["rows"].to_string();
    assert!(shown.contains("helper"), "{}", confident.value);
    assert!(
        !shown.contains("connect") && !shown.contains("quiet"),
        "{}",
        confident.value
    );
}
