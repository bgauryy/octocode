//! Text mentions re-tier dead code only when they can name its subject: a
//! path that names another directory, the import the graph already
//! followed, and a file's own same-named binding are no evidence.
use super::*;
use octocode_test_support::json_fixture;

/// One `issues` row of `detector` on `subject` (under `app/`), at every
/// tier.
fn row(issues: &GraphOutput, detector: &str, subject: &str) -> Value {
    crate::ast_graph::test_support::issue_rows(&issues.value)
        .into_iter()
        .find(|row| row["detector"] == detector && row["subject"] == format!("app/{subject}"))
        .unwrap_or_else(|| panic!("{detector} {subject}: {}", issues.value))
}

fn every_tier(root: &Path, detectors: &[&str]) -> GraphOutput {
    ask(root, "issues", None, |o| {
        o.detectors = detectors.iter().map(|d| (*d).to_owned()).collect();
        o.min_tier = Some(0);
        o.limit = Some(200);
    })
}

/// A path names the directory it is in: `./elsewhere/index` is no mention
/// of `src/old/index.ts`, which stays a tier-100 orphan, while
/// `./legacy/index` names `src/legacy/index.ts`. A path whose directory is
/// a build output (`../dist/tool.js`) or that names no directory still
/// matches by file name.
#[test]
fn a_path_mention_names_the_directory_it_is_in() {
    let dir = git_app(&[
        ("package.json", r#"{"name":"app","main":"src/main.ts"}"#),
        (
            "src/main.ts",
            "export const routes = ['./legacy/index', \"./elsewhere/index\", '../dist/tool.js'];\n",
        ),
        ("src/legacy/index.ts", "export const legacy = 1;\n"),
        ("src/old/index.ts", "export const old = 1;\n"),
        ("src/tool.ts", "export const tool = 1;\n"),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = every_tier(root, &["unreachable-file"]);
    let old = row(&issues, "unreachable-file", "src/old/index.ts");
    assert_eq!(old["tier"], 100, "{old}");
    assert!(old["evidence"].get("mentionedIn").is_none(), "{old}");
    let legacy = row(&issues, "unreachable-file", "src/legacy/index.ts");
    assert_eq!(legacy["tier"], 60, "{legacy}");
    assert_eq!(
        legacy["evidence"]["mentionedIn"],
        json_fixture(r#"["app/src/main.ts:1"]"#),
        "{legacy}"
    );
    let tool = row(&issues, "unreachable-file", "src/tool.ts");
    assert_eq!(tool["tier"], 60, "{tool}");
}

/// The import the graph followed is no text mention: `src/dead/chain2.ts`,
/// imported only by the unreachable `chain1.ts`, stays tier 90 with no
/// `mentionedIn`. Liveness still spreads from a file text names: `b.ts`,
/// imported by the mentioned `a.ts`, drops to tier 60 with `reachedFrom`.
#[test]
fn the_import_the_graph_followed_is_no_text_mention() {
    let dir = git_app(&[
        ("package.json", r#"{"name":"app","main":"src/main.ts"}"#),
        ("src/main.ts", "export const entry = 'src/loaded/a.ts';\n"),
        (
            "src/dead/chain1.ts",
            "import { chain2 } from './chain2';\nexport const chain1 = () => chain2();\n",
        ),
        ("src/dead/chain2.ts", "export const chain2 = () => 1;\n"),
        (
            "src/loaded/a.ts",
            "import { b } from './b';\nexport const a = () => b();\n",
        ),
        ("src/loaded/b.ts", "export const b = () => 1;\n"),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = every_tier(root, &["unreachable-file"]);
    let chain2 = row(&issues, "unreachable-file", "src/dead/chain2.ts");
    assert_eq!(chain2["tier"], 90, "{chain2}");
    assert!(chain2["evidence"].get("mentionedIn").is_none(), "{chain2}");
    let a = row(&issues, "unreachable-file", "src/loaded/a.ts");
    assert_eq!(a["tier"], 60, "{a}");
    let b = row(&issues, "unreachable-file", "src/loaded/b.ts");
    assert_eq!(b["tier"], 60, "{b}");
    assert_eq!(b["evidence"]["reachedFrom"], "app/src/loaded/a.ts", "{b}");
    assert!(b["evidence"].get("mentionedIn").is_none(), "{b}");
}

/// A bare name in a file that declares that name itself is that file's
/// own binding, no reference to another file's export: `unusedHelper`
/// stays tier 90 with no `mentionedIn`.
#[test]
fn a_files_own_binding_is_no_reference_to_an_export() {
    let dir = git_app(&[
        ("package.json", r#"{"name":"app","main":"src/main.ts"}"#),
        (
            "src/main.ts",
            "import { used } from './util';\nimport { s } from './shadow';\nexport const main = () => used() + s;\n",
        ),
        (
            "src/util.ts",
            "export function used() { return 1; }\nexport function unusedHelper() { return 2; }\n",
        ),
        (
            "src/shadow.ts",
            "const unusedHelper = 1;\nexport const s = unusedHelper;\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = every_tier(root, &["unused-export"]);
    let util = row(&issues, "unused-export", "src/util.ts");
    assert_eq!(
        util["evidence"]["exports"],
        json_fixture(r#"["unusedHelper"]"#),
        "{util}"
    );
    assert_eq!(util["tier"], 90, "{util}");
    assert!(util["evidence"].get("mentionedIn").is_none(), "{util}");
}

/// Tier 90 claims no code reference for every name it lists: of `alpha`
/// and `beta`, only `beta` is named in code (`mod.beta()`), so the finding
/// splits by evidence: `[alpha]` at tier 90 with no `mentionedIn`, `[beta]`
/// at tier 60 with its reference. Both stay one finding each, with ids of
/// their own names.
#[test]
fn a_partly_referenced_export_group_splits_by_evidence() {
    let dir = git_app(&[
        ("package.json", r#"{"name":"app","main":"src/main.ts"}"#),
        (
            "src/main.ts",
            "import { used } from './api';\nexport const main = () => used();\n",
        ),
        (
            "src/api.ts",
            "export function used() { return 1; }\nexport function alpha() { return 2; }\nexport function beta() { return 3; }\n",
        ),
        (
            "src/loader.ts",
            "export async function load(file: string) {\n  const mod = await import(file);\n  return mod.beta();\n}\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = every_tier(root, &["unused-export"]);
    let rows = crate::ast_graph::test_support::issue_rows(&issues.value)
        .into_iter()
        .filter(|row| row["subject"] == "app/src/api.ts")
        .collect::<Vec<_>>();
    let by_names = |names: Value| {
        rows.iter()
            .find(|row| row["evidence"]["exports"] == names)
            .cloned()
            .unwrap_or_else(|| panic!("{names}: {}", issues.value))
    };
    assert_eq!(rows.len(), 2, "{}", issues.value);
    let alpha = by_names(json_fixture(r#"["alpha"]"#));
    assert_eq!(alpha["tier"], 90, "{alpha}");
    assert!(alpha["evidence"].get("mentionedIn").is_none(), "{alpha}");
    assert_eq!(alpha["evidence"]["count"], 1, "{alpha}");
    assert_eq!(alpha["evidence"]["exportLine"], 2, "{alpha}");
    let beta = by_names(json_fixture(r#"["beta"]"#));
    assert_eq!(beta["tier"], 60, "{beta}");
    assert_eq!(
        beta["evidence"]["mentionedIn"]["beta"],
        json_fixture(r#"["app/src/loader.ts:3"]"#),
        "{beta}"
    );
    assert_eq!(beta["evidence"]["exportLine"], 3, "{beta}");
    assert_ne!(alpha["id"], beta["id"]);
}

/// A text file the scan cannot read may name the subject: an orphan is
/// tier 100 ("nothing references it anywhere") only when every text file
/// was read, and the unread files are named.
#[cfg(unix)]
#[test]
fn an_unread_text_file_keeps_an_orphan_below_tier_100() {
    use std::os::unix::fs::PermissionsExt;
    let dir = git_app(&[
        ("package.json", r#"{"name":"app","main":"src/main.ts"}"#),
        ("src/main.ts", "export const main = 1;\n"),
        ("src/lonely.ts", "export const lonely = 1;\n"),
        ("notes/load.txt", "load src/lonely.ts\n"),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let locked = root.join("app/notes/load.txt");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let readable = std::fs::read(&locked).is_ok();
    let issues = every_tier(root, &["unreachable-file"]);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    if readable {
        return; // privileged run: permissions do not lock the file
    }
    let lonely = row(&issues, "unreachable-file", "src/lonely.ts");
    assert_eq!(lonely["tier"], 90, "{lonely}");
    let notes = issues.value["summary"]["notes"].to_string();
    assert!(
        notes.contains("app/notes/load.txt"),
        "{}",
        issues.value["summary"]
    );
}
