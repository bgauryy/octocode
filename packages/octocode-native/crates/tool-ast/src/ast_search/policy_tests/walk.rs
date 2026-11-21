//! The scan walk: path policy, withheld and skipped entries, links,
//! cancellation and the leads an empty match offers.

use super::*;
use octocode_test_support::json_fixture;

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

/// `include` only narrows, on `match` and on `symbols` alike: a glob selects
/// among the files the walk visits anyway, so a dot file needs
/// `includeHidden` and a gitignored one `includeIgnored`, as on
/// structureSearch. The selected files the walk left out are disclosed with
/// their causes.
#[test]
fn include_selects_only_among_the_walked_files() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
    root.write(".gitignore", "ignored.ts\n");
    for name in ["shown.ts", ".hidden.ts", "ignored.ts"] {
        root.write(name, "export function shown() { oldCall(1); }\n");
    }
    let names = |out: &serde_json::Value| {
        checks::paths_in(&out["files"])
            .iter()
            .map(|path| {
                std::path::Path::new(path)
                    .file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    let named = |names: &[&str]| {
        names
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<std::collections::BTreeSet<_>>()
    };
    for base in [
        json!({"operation":"match","path":root.0,"include":["*.ts"],"parser":"typescript","rule":{"pattern":"oldCall($A)"}}),
        json!({"operation":"symbols","path":root.0,"include":["*.ts"]}),
    ] {
        let with = |flag: Option<&str>| {
            let mut row = base.clone();
            if let Some(flag) = flag {
                row[flag] = json!(true);
            }
            run(&root.0, row).expect("walk")
        };
        let plain = with(None);
        assert_eq!(names(&plain), named(&["shown.ts"]), "{base}: {plain}");
        let causes = plain["skippedPaths"]["entries"]
            .as_array()
            .unwrap_or_else(|| panic!("skippedPaths: {plain}"))
            .iter()
            .map(|entry| entry["cause"].as_str().expect("cause").to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(causes, named(&["hidden", "ignored"]), "{base}: {plain}");
        let hidden = with(Some("includeHidden"));
        assert_eq!(
            names(&hidden),
            named(&[".hidden.ts", "shown.ts"]),
            "{base}: {hidden}"
        );
        let ignored = with(Some("includeIgnored"));
        assert_eq!(
            names(&ignored),
            named(&["ignored.ts", "shown.ts"]),
            "{base}: {ignored}"
        );
        // A glob that selects only a left-out file: the empty answer names
        // it with its cause and leads to the call that lifts that cause.
        for (glob, cause, flag) in [
            (".hidden.ts", "hidden", "includeHidden"),
            ("ignored.ts", "ignored", "includeIgnored"),
        ] {
            let mut row = base.clone();
            row["include"] = json!([glob]);
            let empty = run(&root.0, row).expect("walk");
            assert_eq!(empty["status"], "empty", "{glob}: {empty}");
            let entries = empty["skippedPaths"]["entries"]
                .as_array()
                .unwrap_or_else(|| panic!("skippedPaths: {empty}"));
            assert!(
                entries.iter().any(|entry| entry["cause"] == cause
                    && entry["path"]
                        .as_str()
                        .is_some_and(|path| path.ends_with(glob))),
                "{glob}: {empty}"
            );
            let lead = &skipped_call(&empty)
                .unwrap_or_else(|| panic!("a skipped-scope lead: {empty}"))["input"]["queries"][0];
            assert_eq!(lead[flag], json!(true), "{glob}: {empty}");
            let found = run(&root.0, lead.clone()).expect("lead");
            assert_eq!(names(&found), named(&[glob]), "{glob}: {found}");
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
    assert_eq!(empty["status"], "empty", "{empty}");
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
    // The note counts the files `filesScanned` counts, and says they were
    // read: the two numbers are one fact, not a scan beside a skip.
    let message = prefilter[0]["message"].as_str().expect("message");
    let scanned = result["filesScanned"].as_u64().expect("filesScanned");
    assert_eq!(scanned, 3, "{result}");
    assert!(
        message.contains(&format!("{scanned} files read")) && !message.contains("skipped"),
        "{result}"
    );
    assert_eq!(result["status"], "empty", "{result}");
    // No file spells the literal, so none was parsed: the note is the whole
    // explanation, and no pattern shape is stated for nodes nobody read.
    assert!(
        !result["diagnostics"]
            .to_string()
            .contains("structural.pattern.shape"),
        "{result}"
    );
}

/// When some scanned files spell the literal and were parsed, the prefilter
/// note counts the others as part of the files scanned, and the pattern's
/// shape is stated for the nodes that were read.
#[test]
fn a_partial_prefilter_skip_is_counted_within_the_files_scanned() {
    let root = Fixture::new();
    std::fs::write(root.0.join("one.ts"), "const missingCall = 1;\n").expect("source");
    for name in ["two.ts", "three.ts"] {
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
    assert_eq!(result["status"], "empty", "{result}");
    let scanned = result["filesScanned"].as_u64().expect("filesScanned");
    let of_code = |code: &str| {
        result["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|diagnostic| diagnostic["code"] == code)
            .filter_map(|diagnostic| diagnostic["message"].as_str())
            .collect::<Vec<_>>()
    };
    let [note] = of_code("structural.prefilter.skipped")[..] else {
        panic!("one prefilter note: {result}");
    };
    assert!(
        note.starts_with(&format!("{} of {scanned} files scanned", scanned - 1)),
        "{note}"
    );
    assert_eq!(of_code("structural.pattern.shape").len(), 1, "{result}");
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
    use octocode_tools_shared::empty_walk::Cause;
    for (dir, cause) in [("gen", Cause::Ignored), (".cfg", Cause::Hidden)] {
        let (flag, _) = cause.flag();
        let root = Fixture::new();
        std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
        root.write(".gitignore", "gen/\n");
        let pkg = root.0.join("pkg");
        std::fs::create_dir_all(pkg.join(dir)).expect("dir");
        std::fs::write(pkg.join(dir).join("a.py"), "print(1)\n").expect("source");
        let request = json!({"operation":"match","path":pkg,"parser":"python","rule":{"pattern":"print($A)"}});
        let empty = run(&root.0, request.clone()).expect("empty match");
        assert_eq!(empty["files"], json!([]), "{dir}: {empty}");
        assert!(empty["matchCount"].is_null(), "{dir}: {empty}");
        assert!(empty["next"].get("repair").is_none(), "{dir}: {empty}");
        let lead = &super::super::test_rows::skipped_call(&empty)
            .unwrap_or_else(|| panic!("missing skipped-scope lead: {empty}"));
        assert_eq!(lead["tool"], "astSearch", "{dir}: {empty}");
        let retry = &lead["input"]["queries"][0];
        assert_eq!(retry[flag], true, "{dir}: {empty}");
        assert_unsearched(&empty, &[cause]);
        // The ledger names the one path, under its cause.
        let entries = empty["skippedPaths"]["entries"].as_array().expect("ledger");
        assert!(
            matches!(entries.as_slice(), [entry] if entry["cause"] == cause.label()
                && entry["path"].as_str().is_some_and(|path| path.ends_with(dir))),
            "{dir}: {empty}"
        );
        let found = run(&root.0, retry.clone()).expect("lead runs");
        assert_eq!(found["matchCount"], 1, "{dir}: {found}");
    }
}

/// Without `language` each file selects its grammar, so an empty scan that
/// parsed nothing still names the paths its walk skipped: an ignored or a
/// hidden directory, with the query that lifts it.
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

/// An empty match that did parse files still names the ignored paths it left
/// out, and leads to the same query with them on: the hint about the
/// pattern's shape is not the only explanation.
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
    assert_unsearched(&empty, &[octocode_tools_shared::empty_walk::Cause::Ignored]);
    assert!(empty["skippedPaths"].to_string().contains("gen"), "{empty}");
    let retry = &super::super::test_rows::skipped_call(&empty)
        .unwrap_or_else(|| panic!("missing skipped-scope lead: {empty}"))["input"]["queries"][0];
    assert_eq!(retry["includeIgnored"], true, "{empty}");
    let found = run(&root.0, retry.clone()).expect("lead runs");
    assert_eq!(found["matchCount"], 1, "{found}");
}

/// A file the scan skips (over the parse limit, binary) is one skip:
/// `filesSkipped` alone (never also `filesUnevaluated`), one diagnostic per
/// file, and its recovery names tools that reach it.
#[test]
fn a_skipped_file_is_one_skip_with_one_diagnostic() {
    let root = Fixture::new();
    root.write("a.ts", "foo(1);\n");
    let large = "foo(2);\n"
        .repeat(octocode_runtime_base::security::SourceLimits::REAL.whole_source / 8 + 1);
    root.write("big.ts", large);
    std::fs::write(root.0.join("bin.ts"), [0_u8, b'f', b'o', b'o', b'(', 0xfe]).expect("binary");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"typescript","rule":{"pattern":"foo($A)"}}),
    )
    .expect("match");
    assert_eq!(out["matchCount"], 1, "{out}");
    assert_eq!(
        out["partialReasons"],
        json_fixture(r#"["filesSkipped"]"#),
        "{out}"
    );
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

/// A `parser` that parsed nothing in a directory offers the other grammars the
/// request's own walk met: no walk of its own, and neither what that walk skips
/// (ignored, hidden, default-excluded) nor a link counts, while a generated file
/// the scan leaves out does. With a depth bound the scan saw less than the scope.
#[test]
fn the_other_grammars_of_an_unparsed_directory_come_from_the_scan_walk() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
    root.write(".gitignore", "gen/\n");
    root.write("src/a.rs", "fn a() {}\n");
    root.write("src/b.rs", "fn b() {}\n");
    root.write("src/deep/c.ts", "const c = 1;\n");
    root.write("min.bundle.min.js", "hit();\n");
    root.write("gen/d.go", "package gen\n");
    root.write(".hidden/e.java", "class E {}\n");
    root.write("node_modules/f/g.rb", "x = 1\n");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.0.join("src/a.rs"), root.0.join("link.cs")).expect("link");
    // The repair is the related call over the other grammars, ahead of the
    // lead over what the walk skipped.
    let languages = |out: &serde_json::Value| {
        out["related"][0]["input"]["queries"]
            .as_array()
            .expect("repair")
            .iter()
            .map(|row| row["parser"].as_str().expect("parser").to_owned())
            .collect::<Vec<_>>()
    };
    let query =
        json!({"operation":"match","path":root.0,"rule":{"pattern":"$A"},"parser":"python"});
    let before = octocode_tools_shared::ast_rule::present_walks();
    let out = run(&root.0, query.clone()).expect("match");
    assert_eq!(
        octocode_tools_shared::ast_rule::present_walks(),
        before,
        "no second walk: {out}"
    );
    assert_eq!(
        languages(&out),
        ["rust", "javascript", "typescript"],
        "{out}"
    );
    let mut bounded = query;
    bounded["maxDepth"] = json!(1);
    let out = run(&root.0, bounded).expect("match");
    assert_eq!(
        octocode_tools_shared::ast_rule::present_walks(),
        before + 1,
        "the whole scope still decides: {out}"
    );
    assert_eq!(
        languages(&out),
        ["rust", "javascript", "typescript"],
        "{out}"
    );
}
