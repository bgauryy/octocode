//! The `includeSkipped` leads of an outline: each keeps the scope and its
//! filters and lifts the walk flags of every cause its walk skipped paths
//! by; nothing behind them is read to choose.

use super::*;
use crate::ast_search::test_rows::assert_unsearched;
use octocode_test_support::json_fixture;
use octocode_tools_shared::empty_walk::Cause;

/// The hidden-path lead keeps the scope, so a root-anchored `exclude` keeps
/// its meaning: the lead finds what the excluded walk can reach.
#[test]
fn the_hidden_lead_keeps_the_exclude_anchored_to_the_scan_root() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".cfg/skip")).expect("dirs");
    for file in [".cfg/keep.ts", ".cfg/skip/gone.ts"] {
        std::fs::write(root.0.join(file), "export function dup() {}\n").expect("source");
    }
    let out = empty_outline(
        &root.0,
        json_fixture(
            r#"{
                "symbolFilter": {"terms": ["dup"]},
                "exclude": [".cfg/skip/**"]
            }"#,
        ),
    );
    assert_eq!(out["status"], "empty", "{out}");
    assert_unsearched(&out, &[Cause::Hidden]);
    assert!(out["skippedPaths"].to_string().contains(".cfg"), "{out}");
    let lead = skipped_lead(&out);
    assert_eq!(
        lead["exclude"],
        json_fixture(r#"[".cfg/skip/**"]"#),
        "{lead}"
    );
    assert_eq!(lead["includeHidden"], json!(true), "{lead}");
    let ran = run_lead(&root.0, lead);
    assert_eq!(found_files(&ran).len(), 1, "{ran}");
    assert!(found_files(&ran)[0].ends_with("keep.ts"), "{ran}");
}

/// `parserGlobs` map a hidden header to a grammar: the probe counts it
/// as outlinable (as the scan does), and the lead parses it with the mapped
/// grammar.
#[test]
fn the_hidden_lead_keeps_the_language_globs_anchored_to_the_scan_root() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".cfg/include")).expect("dirs");
    root.write(
        ".cfg/include/widget.h",
        "namespace Space { class Widget {}; }\n",
    );
    let out = empty_outline(
        &root.0,
        json_fixture(
            r#"{
                "symbolFilter": {"terms": ["Widget"]},
                "parserGlobs": {"cpp": [".cfg/include/**/*.h"]}
            }"#,
        ),
    );
    assert_eq!(out["status"], "empty", "{out}");
    let lead = skipped_lead(&out);
    assert_eq!(
        lead["parserGlobs"],
        json_fixture(r#"{"cpp": [".cfg/include/**/*.h"]}"#),
        "{lead}"
    );
    let ran = run_lead(&root.0, lead);
    assert_eq!(found_files(&ran).len(), 1, "{ran}");
    assert!(declares(&ran, "Widget"), "{ran}");
}

/// A file only `parserGlobs` map to a grammar (an extension no outlined
/// language has) behind a `.gitignore` rule is counted and offered.
#[test]
fn a_language_globs_file_behind_an_ignore_rule_gets_a_lead() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
    root.write(".gitignore", "gen/\n");
    std::fs::create_dir_all(root.0.join("gen")).expect("dir");
    root.write("gen/api.tsx.in", "export function zzMapped() {}\n");
    root.write("main.ts", "export const kept = 1;\n");
    let out = empty_outline(
        &root.0,
        json_fixture(
            r#"{
                "symbolFilter": {"terms": ["zzMapped"]},
                "parserGlobs": {"typescript": ["**/*.tsx.in"]}
            }"#,
        ),
    );
    assert_eq!(out["status"], "empty", "{out}");
    let lead = skipped_lead(&out);
    assert_eq!(lead["includeIgnored"], json!(true), "{lead}");
    let ran = run_lead(&root.0, lead);
    assert!(declares(&ran, "zzMapped"), "{ran}");
}

/// A hidden *file* is reached the same way: the lead keeps the scope and its
/// filters and lifts `includeHidden`.
#[test]
fn the_hidden_file_lead_keeps_the_scope_and_its_filters() {
    let root = Fixture::new();
    root.write(".hidden.ts", "export function hf() {}\n");
    root.write("plain.ts", "export function other() {}\n");
    let out = empty_outline(
        &root.0,
        json_fixture(
            r#"{
                "symbolFilter": {"terms": ["hf"]},
                "parserGlobs": {"typescript": ["**/*.ts"]},
                "exclude": ["**/none/**"]
            }"#,
        ),
    );
    assert_eq!(out["status"], "empty", "{out}");
    let lead = skipped_lead(&out);
    assert_eq!(
        lead["parserGlobs"],
        json_fixture(r#"{"typescript": ["**/*.ts"]}"#),
        "{lead}"
    );
    assert_eq!(lead["exclude"], json_fixture(r#"["**/none/**"]"#), "{lead}");
    assert_eq!(lead["includeHidden"], json!(true), "{lead}");
    let ran = run_lead(&root.0, lead);
    assert!(ran.get("errorCode").is_none(), "{ran}");
    assert!(declares(&ran, "hf"), "{ran}");
}

/// The fixture of a lead-replay defect: the name sits in a
/// `.gitignore`d, default-excluded `build/`, next to a hidden directory that
/// holds another symbol and a pruned `node_modules/`.
fn skipped_workspace(hidden_name: &str, ignored_name: &str) -> Fixture {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
    root.write(".gitignore", "build/\n");
    for (file, name) in [
        (".hidden/h.ts", hidden_name),
        ("build/out.ts", ignored_name),
        ("node_modules/pkg/i.ts", "defaultExcludedSym"),
        ("src/a.ts", "visibleSym"),
    ] {
        let at = root.0.join(file);
        std::fs::create_dir_all(at.parent().expect("parent")).expect("dirs");
        std::fs::write(at, format!("export function {name}() {{ return 1; }}\n")).expect("source");
    }
    root
}

/// A name that sits only in the ignored `build/` leads to the same scope
/// with every flag the walk skipped by lifted, `includeHidden` too: nothing
/// behind the skipped paths is read to tell which of them holds it.
#[test]
fn an_ignored_symbol_leads_to_its_own_scope_with_the_skipped_flags_lifted() {
    let root = skipped_workspace("hiddenSym", "ignoredSym");
    let out = empty_outline(
        &root.0,
        json_fixture(r#"{"symbolFilter": {"terms": ["ignoredSym"]}}"#),
    );
    assert_eq!(out["status"], "empty", "{out}");
    assert!(out["next"].get("narrowScope").is_none(), "{out}");
    assert_unsearched(
        &out,
        &[Cause::Ignored, Cause::Hidden, Cause::DefaultExcluded],
    );
    let skipped = out["skippedPaths"].to_string();
    assert!(
        skipped.contains(".hidden") && skipped.contains("build"),
        "{out}"
    );
    let lead = skipped_lead(&out);
    assert_eq!(lead["path"], json!(root.0.to_string_lossy()), "{out}");
    assert_eq!(lead["includeIgnored"], json!(true), "{out}");
    assert_eq!(lead["includeHidden"], json!(true), "{out}");
    assert_eq!(lead["defaultExcludes"], json!(false), "{out}");
    let found = run_lead(&root.0, lead);
    assert_eq!(found_files(&found).len(), 1, "{found}");
    assert!(declares(&found, "ignoredSym"), "{found}");
}

/// An empty outline's lead lifts the flags of what its walk still skipped
/// (with `includeIgnored` set, the ignore rules skip nothing more), keeps the
/// ones the request set, and running it finds the name.
#[test]
fn a_symbol_lead_lifts_the_flags_left_to_reach_it() {
    let root = skipped_workspace("hiddenSym", "ignoredSym");
    for (request, lifted) in [
        (
            json_fixture(r#"{"symbolFilter": {"terms": ["hiddenSym"]}}"#),
            json_fixture(
                r#"{"includeHidden": true, "includeIgnored": true, "defaultExcludes": false}"#,
            ),
        ),
        (
            json_fixture(
                r#"{
                    "symbolFilter": {"terms": ["hiddenSym"]},
                    "includeIgnored": true
                }"#,
            ),
            json_fixture(
                r#"{"includeHidden": true, "includeIgnored": true, "defaultExcludes": false}"#,
            ),
        ),
    ] {
        let out = empty_outline(&root.0, request.clone());
        assert_eq!(out["status"], "empty", "{request}: {out}");
        let lead = skipped_lead(&out);
        for flag in ["includeHidden", "includeIgnored", "defaultExcludes"] {
            assert_eq!(lead.get(flag), lifted.get(flag), "{request} {flag}: {lead}");
        }
        let found = run_lead(&root.0, lead);
        assert!(declares(&found, "hiddenSym"), "{request}: {found}");
    }
}

/// A name held both by a hidden directory and by an ignored one: one lead
/// lifts both causes and reaches every file, and the ledger names both.
#[test]
fn a_symbol_behind_hidden_and_ignored_paths_gets_one_lead_lifting_both() {
    let root = skipped_workspace("sharedSym", "sharedSym");
    let out = empty_outline(
        &root.0,
        json_fixture(r#"{"symbolFilter": {"terms": ["sharedSym"]}}"#),
    );
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(
        out["related"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|call| call["tool"] == "astSearch")
            .count(),
        1,
        "{out}"
    );
    assert_unsearched(
        &out,
        &[Cause::Ignored, Cause::Hidden, Cause::DefaultExcluded],
    );
    let skipped = out["skippedPaths"].to_string();
    assert!(
        skipped.contains(".hidden") && skipped.contains("build"),
        "{out}"
    );
    let lead = skipped_lead(&out);
    assert_eq!(lead["path"], json!(root.0.to_string_lossy()), "{out}");
    assert_eq!(lead["includeIgnored"], json!(true), "{out}");
    assert_eq!(lead["includeHidden"], json!(true), "{out}");
    let found = run_lead(&root.0, lead);
    let files = found_files(&found);
    assert_eq!(files.len(), 2, "{found}");
    assert!(files.iter().any(|f| f.ends_with("out.ts")), "{found}");
    assert!(files.iter().any(|f| f.ends_with("h.ts")), "{found}");
}

/// Several hidden directories holding the name are all reached by the one
/// lead, so no hidden file stays unreachable behind the first one.
#[test]
fn every_hidden_directory_holding_the_name_is_reached() {
    let root = Fixture::new();
    for dir in [".alpha", ".beta"] {
        std::fs::create_dir_all(root.0.join(dir)).expect("dir");
        std::fs::write(
            root.0.join(dir).join("x.ts"),
            "export function twinSym() {}\n",
        )
        .expect("source");
    }
    root.write("main.ts", "export const kept = 1;\n");
    let out = empty_outline(
        &root.0,
        json_fixture(r#"{"symbolFilter": {"terms": ["twinSym"]}}"#),
    );
    let found = run_lead(&root.0, skipped_lead(&out));
    let files = found_files(&found);
    assert_eq!(files.len(), 2, "{found}");
    assert!(files.iter().any(|p| p.contains(".alpha")), "{found}");
    assert!(files.iter().any(|p| p.contains(".beta")), "{found}");
}

/// One gitignore policy for both operations: a directory outline leaves
/// `.gitignore`d files out as `match` does, and an empty one names them and
/// leads to the outline with `includeIgnored` on.
#[test]
fn an_outline_leaves_out_gitignored_files_and_leads_to_them() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
    root.write(".gitignore", "gen/\n");
    std::fs::create_dir_all(root.0.join("gen")).expect("dir");
    root.write("gen/out.ts", "export const zzGenerated = 1;\n");
    root.write("main.ts", "export const kept = 1;\n");
    let kept = empty_outline(&root.0, json!({}));
    assert_eq!(found_files(&kept).len(), 1, "{kept}");
    assert_eq!(kept["filesScanned"], 1, "{kept}");
    let out = empty_outline(
        &root.0,
        json_fixture(r#"{"symbolFilter": {"terms": ["zzGenerated"]}}"#),
    );
    assert_eq!(out["status"], "empty", "{out}");
    assert_unsearched(&out, &[Cause::Ignored]);
    assert!(out["skippedPaths"].to_string().contains("gen"), "{out}");
    let lead = super::super::test_rows::skipped_call(&out)
        .unwrap_or_else(|| panic!("missing skipped-scope lead: {out}"))["input"]["queries"][0]
        .clone();
    assert_eq!(lead["includeIgnored"], json!(true), "{out}");
    assert_eq!(lead["path"], json!(root.0.to_string_lossy()), "{out}");
    let found = run_lead(&root.0, lead);
    assert_eq!(found_files(&found).len(), 1, "{found}");
    assert!(declares(&found, "zzGenerated"), "{found}");
    let direct = empty_outline(
        &root.0,
        json_fixture(
            r#"{
                "symbolFilter": {"terms": ["zzGenerated"]},
                "includeIgnored": true
            }"#,
        ),
    );
    assert!(declares(&direct, "zzGenerated"), "{direct}");
}

/// A file skipped as too dense to parse is still searched and read as text:
/// the outline's skip names that recovery once, as a match's skip does.
#[test]
fn a_file_skipped_as_too_large_to_parse_names_the_text_recovery_once() {
    let root = Fixture::new();
    let budget = octocode_engine::structural::parse_budget();
    let calls = budget / octocode_engine::structural::BYTES_PER_TOKEN / 5 + 1000;
    std::fs::write(
        root.0.join("dense.js"),
        format!("var a=[{}];\n", "f(1),".repeat(calls)),
    )
    .expect("dense");
    std::fs::write(root.0.join("plain.js"), "function hello() { return 1 }\n").expect("plain");
    let out = empty_outline(&root.0, json!({}));
    let recovery =
        octocode_tools_shared::ast_rule::diagnostic_recovery("structural.file.tooLarge", None)
            .expect("text");
    let named = out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|diagnostic| diagnostic["recovery"] == json!(recovery))
        .count();
    assert_eq!(named, 1, "{out}");
    assert!(declares(&out, "hello"), "{out}");
}
