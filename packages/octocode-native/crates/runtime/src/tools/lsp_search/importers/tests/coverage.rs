//! Coverage: verification failures, ignored importers and unfinished scans.
use super::*;

#[test]
fn failed_importer_verification_never_reports_complete_coverage() {
    let importers = Importers {
        failed: true,
        ..Importers::default()
    };
    let mut row = json!({"status": "success"});
    let query = serde_json::from_value(json!({
        "operation": "references", "path": "/r/a.ts", "symbolName": "x", "line": 1
    }))
    .expect("query");
    importers.annotate(&mut row, &query, &Scope::new("/r".into(), Vec::new()));
    assert_eq!(row["coverage"]["importerScan"], SCAN_FAILED);
}

/// A scope the request's workspaceRoot narrowed says that importers outside
/// it were not scanned; the default scope says nothing.
#[test]
fn a_narrowed_scope_discloses_its_root() {
    let query = serde_json::from_value(json!({
        "operation": "references", "path": "/r/p/a.ts", "symbolName": "x", "line": 1
    }))
    .expect("query");
    let mut scope = Scope::new("/r/p".into(), Vec::new());
    let mut row = json!({"status": "success"});
    Importers::default().annotate(&mut row, &query, &scope);
    assert!(row["coverage"].get("importerScanRoot").is_none(), "{row}");
    scope.narrowed = true;
    let mut row = json!({"status": "success"});
    Importers::default().annotate(&mut row, &query, &scope);
    assert_eq!(
        row["coverage"]["importerScanRoot"], "workspaceRoot",
        "{row}"
    );
}

/// Unverified candidates stay out of the answer, so `textOnlyFiles` and
/// the lexical lead disclose them, and the scan is never complete.
#[test]
fn unverified_candidates_are_not_answered_and_fail_the_scan() {
    let list = candidates(3);
    let Cut::Window(window) = Window::cut(list.clone(), WindowRequest::default()) else {
        panic!("window 1 is never stale");
    };
    let importers = Importers {
        window: Some(window),
        failed: true,
        unverified: vec![list[1].clone()],
        ..Importers::default()
    };
    assert_eq!(importers.covered(), vec![list[0].clone(), list[2].clone()]);
    let scope = Scope::new("/r".into(), Vec::new());
    let query = serde_json::from_value(json!({
        "operation": "references", "path": "/r/a.ts", "symbolName": "x", "line": 1
    }))
    .expect("query");
    let mut row = json!({"status": "success"});
    importers.annotate(&mut row, &query, &scope);
    assert_eq!(row["coverage"]["importerScan"], SCAN_FAILED);
}

/// A gitignored file that imports the name, one the check could not
/// decide, or a scan that stopped at its bound makes the row partial;
/// files that only spell it and nested repositories leave a lead and no
/// partial reason.
#[test]
fn ignored_importers_or_an_unfinished_scan_make_the_row_partial() {
    let query: LspSearchQuery = serde_json::from_value(json!({
        "operation": "references", "path": "/r/a.ts", "symbolName": "x", "line": 1
    }))
    .expect("query");
    let scope = Scope::new("/r".into(), vec!["*.ts".into()]);
    let annotate = |ignored: IgnoredMentions| {
        let importers = Importers {
            ignored,
            ..Importers::default()
        };
        let mut row = json!({"status": "success"});
        importers.annotate(&mut row, &query, &scope);
        row
    };
    let quiet = annotate(IgnoredMentions::default());
    assert!(quiet.get("isPartial").is_none(), "{quiet}");

    // Nested repositories the scan did not enter are counted, whatever
    // else it found; the lead names each one.
    let nested = annotate(IgnoredMentions {
        nested_repositories: vec!["repos/a".into(), "repos/b".into()],
        ..IgnoredMentions::default()
    });
    assert_eq!(
        nested[crate::response::paging::NOTICES],
        json!([
            "The scan for gitignored files did not enter 2 nested repositories (directories with their own .git) under the scope root."
        ]),
        "{nested}"
    );
    assert!(
        !quiet["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "localSearch"),
        "{quiet}"
    );
    // They are no evidence, so the row stays whole, but the lead reaches
    // into each of them.
    assert!(nested.get("isPartial").is_none(), "{nested}");
    assert!(nested.get("partialReasons").is_none(), "{nested}");
    let lead_paths = |row: &Value| {
        crate::tools::test_support::checks::related_call(row, "localSearch", |_| true)["input"]["queries"]
            .as_array()
            .expect("leads")
            .iter()
            .map(|lead| {
                assert_eq!(lead["includeIgnored"], true, "{row}");
                lead["path"].clone()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        lead_paths(&nested),
        [json!("/r/repos/a"), json!("/r/repos/b")],
        "{nested}"
    );
    // With evidence, one lead covers it and the nested repositories.
    let nested_hidden = annotate(IgnoredMentions {
        files: vec!["/r/gen/g.ts".into()],
        importing: vec!["/r/gen/g.ts".into()],
        nested_repositories: vec!["repos/a".into()],
        ..IgnoredMentions::default()
    });
    assert_eq!(
        lead_paths(&nested_hidden),
        [json!("/r/gen/g.ts"), json!("/r/repos/a")],
        "{nested_hidden}"
    );

    // An unfinished scan left importers unverified: a partial row, a
    // lead over the scope, and a note.
    let unfinished = annotate(IgnoredMentions::unfinished());
    assert_eq!(unfinished["isPartial"], true, "{unfinished}");
    assert_eq!(
        unfinished["partialReasons"],
        json!([IGNORED_REASON]),
        "{unfinished}"
    );
    let lead = &crate::tools::test_support::checks::related_call(
        &unfinished,
        "localSearch",
        |_| true,
    )["input"]["queries"][0];
    assert_eq!(lead["path"], "/r", "{unfinished}");
    assert_eq!(lead["includeIgnored"], true, "{unfinished}");
    assert_eq!(
        unfinished[crate::response::paging::NOTICES],
        json!([IGNORED_UNFINISHED_NOTICE]),
        "{unfinished}"
    );

    // Bundled copies that only spell the name: their exact lead.
    let copies = annotate(IgnoredMentions {
        files: vec!["/r/releases/copy.mjs".into()],
        rules: vec![".gitignore:1 'releases/**'".into()],
        ..IgnoredMentions::default()
    });
    assert!(copies.get("isPartial").is_none(), "{copies}");
    assert_eq!(
        crate::tools::test_support::checks::related_call(&copies, "localSearch", |_| true)["input"]
            ["queries"][0]["path"],
        "/r/releases/copy.mjs",
        "{copies}"
    );

    // An ignored importer: partial, named with its rule, lead to it.
    let hidden = annotate(IgnoredMentions {
        files: vec!["/r/gen/g.ts".into(), "/r/gen/h.ts".into()],
        rules: vec![".gitignore:1 'gen/'".into()],
        unfinished: true,
        importing: vec!["/r/gen/g.ts".into()],
        ..IgnoredMentions::default()
    });
    assert_eq!(
        hidden["partialReasons"],
        json!([IGNORED_REASON]),
        "{hidden}"
    );
    let warning = hidden["warnings"][0].as_str().expect("warning");
    assert!(
        warning.contains("1 imports it")
            && !warning.contains("could not be checked")
            && warning.contains("more may exist"),
        "{hidden}"
    );
    // More may exist past where the scan stopped: the lead names the
    // importer it found, then covers the scope for what it did not reach.
    assert_eq!(
        lead_paths(&hidden),
        [json!("/r/gen/g.ts"), json!("/r")],
        "{hidden}"
    );
    let past_stop = &crate::tools::test_support::checks::related_call(
        &hidden,
        "localSearch",
        |_| true,
    )["input"]["queries"][1];
    assert_eq!(past_stop["include"], json!(["*.ts"]), "{hidden}");
    // Undecided files and nested repositories are named the same way.
    let hidden_unchecked = annotate(IgnoredMentions {
        files: vec!["/r/gen/big.js".into(), "/r/gen/g.ts".into()],
        unfinished: true,
        importing: vec!["/r/gen/g.ts".into()],
        unchecked: vec!["/r/gen/big.js".into()],
        nested_repositories: vec!["repos/a".into()],
        ..IgnoredMentions::default()
    });
    assert_eq!(
        lead_paths(&hidden_unchecked),
        [
            json!("/r/gen/g.ts"),
            json!("/r/gen/big.js"),
            json!("/r/repos/a"),
            json!("/r")
        ],
        "{hidden_unchecked}"
    );

    // Files the check could not decide are counted apart, never as
    // importers, and still lead.
    let undecided = annotate(IgnoredMentions {
        files: vec!["/r/gen/big.js".into(), "/r/gen/g.ts".into()],
        rules: vec![".gitignore:1 'gen/'".into()],
        importing: vec!["/r/gen/g.ts".into()],
        unchecked: vec!["/r/gen/big.js".into()],
        ..IgnoredMentions::default()
    });
    assert_eq!(undecided["partialReasons"], json!([IGNORED_REASON]));
    let warning = undecided["warnings"][0].as_str().expect("warning");
    assert!(
        warning.contains("1 imports it and 1 could not be checked"),
        "{undecided}"
    );
    let leads = crate::tools::test_support::checks::related_call(&undecided, "localSearch", |_| {
        true
    })["input"]["queries"]
        .as_array()
        .expect("leads")
        .iter()
        .map(|lead| lead["path"].clone())
        .collect::<Vec<_>>();
    assert_eq!(leads, [json!("/r/gen/g.ts"), json!("/r/gen/big.js")]);
}

/// The import check reads each ignored file through the bounded reader:
/// a file too large for the parser or unparsable stays unchecked rather
/// than counting as an importer, and every other file is checked.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ignored_files_the_check_cannot_decide_stay_unchecked() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    let write = |name: &str, text: &str| {
        let path = root.join(name);
        std::fs::write(&path, text).expect("write");
        path.to_string_lossy().into_owned()
    };
    let importer = write("imp.ts", "import { x } from './a';\nx();\n");
    let copy = write("copy.ts", "export function x() {}\n");
    let large = write(
        "large.js",
        &"// x\n".repeat(octocode_engine::signatures::MAX_PARSE_SIZE / 5 + 1),
    );
    let mut files = vec![importer.clone(), copy, large.clone()];
    // More files than any fixed bound once held: every one is checked.
    let many = (0..128)
        .map(|index| write(&format!("p{index:03}.ts"), "import { x } from './a';\n"))
        .collect::<Vec<_>>();
    files.extend(many.iter().cloned());
    let mut ignored = IgnoredMentions {
        files,
        ..IgnoredMentions::default()
    };
    let policy = crate::tools::test_support::workspace_policy(&root);
    check_imports(
        &mut ignored,
        &policy,
        &crate::execution::cancel::NeverCancel,
        "x",
        None,
    )
    .await
    .expect("check");
    let mut importing = vec![importer];
    importing.extend(many);
    assert_eq!(
        ignored.importing, importing,
        "no file past a bound is skipped"
    );
    assert_eq!(
        ignored.unchecked,
        vec![large],
        "only the file the parser refuses"
    );
}
