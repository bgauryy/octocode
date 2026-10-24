//! The scan walk: path policy, withheld and skipped entries, links,
//! cancellation and the leads an empty match offers.

use super::*;

/// Each astSearch walk (the outline scan of `symbols`, the structural scan
/// of `match`; neither is structureSearch's walk) applies the path policy
/// before it counts or reads a descendant: with `includeHidden`, the
/// credential directory, the sensitive file names and `.env.ts` are neither
/// scanned nor named.
#[test]
fn descendant_policy_precedes_discovery_totals_and_line_reads() {
    let root = sensitive_fixture();
    root.write(".aws/hidden.ts", "oldCall(secret);\n");
    root.write(".env.ts", "oldCall(ignored);\n");
    root.write("visible.ts", "oldCall(visible);\n");
    let (paths, security) = simple_policy(&root.0);
    let canonical_root = root.0.canonicalize().expect("canonical root");
    for (row, count, expected) in [
        (
            json!({"operation":"symbols","path":root.0,"includeHidden":true}),
            "filesScanned",
            2,
        ),
        (
            json!({"operation":"match","path":root.0,"parser":"typescript","rule":{"pattern":"oldCall($A)"},"includeHidden":true}),
            "matchCount",
            1,
        ),
    ] {
        let out = execute_row(row.clone(), &paths, &security, &NeverCancel).expect("walk");
        assert_eq!(out[count], expected, "{row}: {out}");
        let listed = checks::paths_in(&out["files"]);
        assert!(!listed.is_empty(), "{row}: {out}");
        assert!(
            listed.iter().all(|path| canonical_root.join(path).parent()
                == Some(canonical_root.as_path())
                && std::path::Path::new(path)
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("visible."))),
            "{row}: {out}"
        );
        for withheld in ["hidden.ts", ".env.ts", "credentials", "tfstate"] {
            checks::assert_absent(&out, withheld);
        }
    }
}

/// A directory scan whose path policy withheld candidate entries says so,
/// as localSearch and structureSearch do: an empty or "complete" result
/// over a credential-store directory is not proof of absence. A source
/// folder merely named `secrets/` or `private/` is scanned, not withheld.
#[test]
fn directory_scans_disclose_policy_withheld_entries() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".aws")).expect("aws dir");
    root.write(".aws/store.ts", "register(secret);\n");
    // The repository's own `.git` is a dot entry out of the walk's
    // view, so it is not disclosed on every scan.
    std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
    root.write(
        "visible.ts",
        "register(visible);\nexport function shown() {}\n",
    );
    let disclosed = |out: &serde_json::Value| {
        let reasons = skipped_reasons(out, "withheld");
        reasons.len() == 1 && reasons[0].0 == "security-policy dir .aws/"
    };
    let matches = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"typescript","rule":{"pattern":"register($A)"},"includeHidden":true}),
    )
    .expect("match");
    assert_eq!(matches["matchCount"], 1, "{matches}");
    assert!(disclosed(&matches), "{matches}");
    assert!(matches.get("coverage").is_none(), "{matches}");
    let empty = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"typescript","rule":{"pattern":"register(secret)"},"includeHidden":true}),
    )
    .expect("empty match");
    assert!(disclosed(&empty), "{empty}");
    let symbols = run(&root.0, json!({"operation":"symbols","path":root.0})).expect("symbols");
    // Dot directories are out of a symbols walk's view, disclosed or not.
    assert_eq!(symbols["filesScanned"], 1, "{symbols}");
    checks::assert_absent(&symbols, "store.ts");
}

/// A link inside the workspace is never followed by a directory scan: a
/// scope whose sources sit only behind links says so, with localSearch's
/// wording, instead of an empty outline or a `languageRequired` that blames
/// the grammar.
#[cfg(unix)]
#[test]
fn directory_scans_disclose_in_workspace_links_they_do_not_follow() {
    let root = Fixture::new();
    root.write(
        "real/skill/run.ts",
        "register(linked);\nexport function shown() {}\n",
    );
    std::fs::create_dir_all(root.0.join("links")).expect("links dir");
    std::os::unix::fs::symlink(root.0.join("real/skill"), root.0.join("links/skill"))
        .expect("dir link");
    std::os::unix::fs::symlink(
        root.0.join("real/skill/run.ts"),
        root.0.join("links/run.ts"),
    )
    .expect("file link");
    let notice = [("symlink not followed".into(), 2)];
    let scope = root.0.join("links");
    let matched = run(
        &root.0,
        json!({"operation":"match","path":scope,"rule":{"pattern":"register($A)"}}),
    )
    .expect("no languageRequired behind links");
    assert_eq!(skipped_reasons(&matched, "withheld"), notice, "{matched}");
    let typed = run(
        &root.0,
        json!({"operation":"match","path":scope,"parser":"typescript","rule":{"pattern":"register($A)"}}),
    )
    .expect("typed match");
    assert_eq!(skipped_reasons(&typed, "withheld"), notice, "{typed}");
    let symbols = run(&root.0, json!({"operation":"symbols","path":scope})).expect("symbols");
    assert_eq!(skipped_reasons(&symbols, "withheld"), notice, "{symbols}");
    // A scope with no file at all names no grammar to set either.
    std::fs::create_dir_all(root.0.join("empty")).expect("empty dir");
    let empty = run(
        &root.0,
        json!({"operation":"match","path":root.0.join("empty"),"rule":{"pattern":"register($A)"}}),
    )
    .expect("an empty scope is no languageRequired");
    assert_eq!(empty["matchCount"], 0, "{empty}");
}

#[test]
fn directory_prefilter_skips_are_aggregated_once() {
    let root = Fixture::new();
    for name in ["one.ts", "two.ts", "three.ts"] {
        std::fs::write(root.0.join(name), "const value = 1;\n").expect("source");
    }
    let (paths, security) = simple_policy(&root.0);
    let result = execute_row(
        json!({"operation":"match","path":root.0,"parser":"typescript","rule":{"pattern":"missingCall($A);"}}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("structural zero");
    let prefilter = result["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "structural.prefilter.skipped")
        .collect::<Vec<_>>();
    assert_eq!(prefilter.len(), 1, "{result}");
    assert!(
        prefilter[0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("3 file(s)")),
        "{result}"
    );
    assert_eq!(result["status"], "empty", "{result}");
}

/// A cancellation during either astSearch directory walk stops it with
/// `cancelled`.
#[test]
fn cancellation_interrupts_descendant_traversal() {
    let root = Fixture::new();
    root.write("source.rs", "source");
    let (paths, security) = simple_policy(&root.0);
    for row in [
        json!({"operation":"symbols","path":root.0}),
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"source($A)"}}),
    ] {
        let error = execute_row(
            row.clone(),
            &paths,
            &security,
            &AfterRoot(std::sync::atomic::AtomicUsize::new(0)),
        )
        .expect_err("cancel the walk");
        assert_eq!(error.code, ErrorCode::Cancelled, "{row}");
    }
}

/// A directory whose files the walk skipped as ignored or hidden parsed
/// nothing: the empty match leads to the same query with those walks on,
/// and does not offer a repair that drops `language`.
#[test]
fn an_empty_match_over_ignored_and_hidden_files_leads_to_them() {
    for (dir, flag) in [("gen", "includeIgnored"), (".cfg", "includeHidden")] {
        let root = Fixture::new();
        std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
        root.write(".gitignore", "gen/\n");
        let pkg = root.0.join("pkg");
        std::fs::create_dir_all(pkg.join(dir)).expect("dir");
        std::fs::write(pkg.join(dir).join("a.py"), "print(1)\n").expect("source");
        let request = json!({"operation":"match","path":pkg,"parser":"python","rule":{"pattern":"print($A)"}});
        let empty = run(&root.0, request.clone()).expect("empty match");
        assert!(empty["files"].is_null(), "{dir}: {empty}");
        assert!(empty["next"].get("repair").is_none(), "{dir}: {empty}");
        let lead = &super::super::test_rows::skipped_call(&empty)
            .unwrap_or_else(|| panic!("missing skipped-scope lead: {empty}"));
        assert_eq!(lead["tool"], "astSearch", "{dir}: {empty}");
        let retry = &lead["input"]["queries"][0];
        assert_eq!(retry[flag], true, "{dir}: {empty}");
        assert!(
            empty["hints"]
                .to_string()
                .contains(if flag == "includeIgnored" {
                    ".gitignore"
                } else {
                    "hidden"
                }),
            "{dir}: {empty}"
        );
        let found = run(&root.0, retry.clone()).expect("lead runs");
        assert_eq!(found["matchCount"], 1, "{dir}: {found}");
    }
}

/// Without `language` each file selects its grammar, so an empty scan that
/// parsed nothing still probes the skipped walks: the source behind an ignore
/// rule or a hidden directory is named, with the query that lifts it.
#[test]
fn an_empty_match_without_language_leads_to_skipped_files() {
    for (dir, flag) in [("gen", "includeIgnored"), (".cfg", "includeHidden")] {
        let root = Fixture::new();
        std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
        root.write(".gitignore", "gen/\n");
        let pkg = root.0.join("pkg");
        std::fs::create_dir_all(pkg.join(dir)).expect("dir");
        std::fs::write(pkg.join(dir).join("a.py"), "print(1)\n").expect("source");
        let request = json!({"operation":"match","path":pkg,"rule":{"pattern":"print($A)"}});
        let empty = run(&root.0, request).expect("empty match");
        let retry = &super::super::test_rows::skipped_call(&empty)
            .unwrap_or_else(|| panic!("missing skipped-scope lead: {empty}"))["input"]["queries"]
            [0];
        assert_eq!(retry[flag], true, "{dir}: {empty}");
        assert!(retry.get("parser").is_none(), "{dir}: {empty}");
        let found = run(&root.0, retry.clone()).expect("lead runs");
        assert_eq!(found["matchCount"], 1, "{dir}: {found}");
    }
}

/// An empty match that did parse files still names the ignored files of its
/// grammar it left out, and leads to the same query with them on: the hint
/// about the pattern's shape is not the only explanation.
#[test]
fn an_empty_match_beside_parsed_files_names_the_ignored_ones() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
    root.write(".gitignore", "gen/\n");
    std::fs::create_dir_all(root.0.join("gen")).expect("dir");
    root.write("gen/b.ts", "export const y = double(2);\n");
    root.write("a.ts", "export const x = 1;\n");
    let request = json!({"operation":"match","path":root.0,"parser":"typescript","rule":{"pattern":"double($A)"}});
    let empty = run(&root.0, request).expect("empty match");
    assert_eq!(empty["status"], "empty", "{empty}");
    assert_eq!(empty["filesScanned"], json!(1), "{empty}");
    let hints = empty["hints"].to_string();
    assert!(hints.contains(".gitignore:1 'gen/'"), "{hints}");
    let retry = &super::super::test_rows::skipped_call(&empty)
        .unwrap_or_else(|| panic!("missing skipped-scope lead: {empty}"))["input"]["queries"][0];
    assert_eq!(retry["includeIgnored"], true, "{empty}");
    let found = run(&root.0, retry.clone()).expect("lead runs");
    assert_eq!(found["matchCount"], 1, "{found}");
}

/// A file the scan skips (over the parse limit, not UTF-8) is one skip:
/// `filesSkipped` alone (never also `filesUnevaluated`), one diagnostic per
/// file, and its recovery names tools that reach it.
#[test]
fn a_skipped_file_is_one_skip_with_one_diagnostic() {
    let root = Fixture::new();
    root.write("a.ts", "foo(1);\n");
    let large =
        "foo(2);\n".repeat(crate::tools::ast_search::source::MAX_PARSE_SOURCE_BYTES / 8 + 1);
    root.write("big.ts", large);
    std::fs::write(
        root.0.join("bin.ts"),
        [0xff_u8, b'f', b'o', b'o', b'(', 0xfe],
    )
    .expect("binary");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"typescript","rule":{"pattern":"foo($A)"}}),
    )
    .expect("match");
    assert_eq!(out["matchCount"], 1, "{out}");
    assert_eq!(out["partialReasons"], json!(["filesSkipped"]), "{out}");
    let diagnostics = out["diagnostics"].as_array().expect("diagnostics");
    for name in ["big.ts", "bin.ts"] {
        let about = diagnostics
            .iter()
            .filter(|d| d["path"].as_str().is_some_and(|path| path.ends_with(name)))
            .collect::<Vec<_>>();
        assert_eq!(about.len(), 1, "{name}: {out}");
    }
    let large = diagnostics
        .iter()
        .find(|d| d["code"] == "structural.file.tooLarge")
        .expect("too large");
    let recovery = large["recovery"].as_str().expect("recovery");
    assert!(
        recovery.contains("localSearch") && recovery.contains("localFetch"),
        "{recovery}"
    );
    assert!(!recovery.contains("maxFileBytes"), "{recovery}");
}
