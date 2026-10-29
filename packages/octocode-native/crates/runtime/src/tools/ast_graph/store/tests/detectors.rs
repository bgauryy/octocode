//! Cycle and dead-code detectors judged on one fixture each.
use super::*;

mod health;
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

/// `graph query cycles` and the `cycle` detector of `graph query issues`
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
    let listed = listed.value["results"]
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
    assert_eq!(listed, expected, "graph query cycles");
    let issues = ask(root, "issues", None, |o| {
        o.detectors = vec!["cycle".into()];
        o.limit = Some(50);
    });
    let found = issues.value["results"]
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
    assert_eq!(found, expected, "graph query issues: {}", issues.value);
    assert!(
        issues.value["summary"]["detectorMs"]["health"].is_number(),
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
    let rows = issues.value["results"]
        .as_array()
        .cloned()
        .unwrap_or_default();
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
    let row = all.value["results"]
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
        all.value["results"]
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
