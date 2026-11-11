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

/// Only a gitignored or hidden file that imports the name makes the row
/// partial. One the check could not decide, a scan that did not finish and
/// nested repositories are outside the answer's scope: a note and a lead, no
/// partial reason. Files the check ruled out (they only spell the name) are
/// no lead: the scan proved them.
#[test]
fn only_an_ignored_importer_makes_the_row_partial() {
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

    // An unfinished scan is outside the answer's scope: the row stays
    // whole, with a lead over the scope and a note.
    let unfinished = annotate(IgnoredMentions::unfinished());
    assert!(unfinished.get("isPartial").is_none(), "{unfinished}");
    assert!(unfinished.get("partialReasons").is_none(), "{unfinished}");
    assert_eq!(
        unfinished["coverage"]["importerScan"], SCAN_COMPLETE,
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

    // Bundled copies that only spell the name, however many: a finished
    // scan checked each and none imports it, so nothing is left to read
    // (an unfiltered walk of the whole root over more than a few of them
    // cost far more than the answer).
    let localsearch_leads = |row: &Value| {
        row["related"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|call| call["tool"] == "localSearch")
            .count()
    };
    for count in [1, crate::tools::lsp_search::failure::LISTED_TEXT_FILES + 1] {
        let copies = annotate(IgnoredMentions {
            files: (0..count)
                .map(|n| format!("/r/releases/copy{n}.mjs"))
                .collect(),
            rules: vec![".gitignore:1 'releases/**'".into()],
            ..IgnoredMentions::default()
        });
        assert!(copies.get("isPartial").is_none(), "{copies}");
        assert_eq!(localsearch_leads(&copies), 0, "{copies}");
    }
    // Beside a nested repository, the lead reaches only into it.
    let copies_nested = annotate(IgnoredMentions {
        files: vec!["/r/releases/copy.mjs".into()],
        nested_repositories: vec!["repos/a".into()],
        ..IgnoredMentions::default()
    });
    assert_eq!(
        lead_paths(&copies_nested),
        [json!("/r/repos/a")],
        "{copies_nested}"
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
    assert!(warning.contains("1 imports it"), "{hidden}");
    assert_eq!(
        hidden[crate::response::paging::NOTICES],
        json!([IGNORED_UNFINISHED_NOTICE]),
        "{hidden}"
    );
    assert_eq!(hidden["coverage"]["importerScan"], SCAN_SKIPPED, "{hidden}");
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

    // Files the check could not decide alone leave the row whole: the same
    // note on every row, and a lead to each of them.
    let only_undecided = annotate(IgnoredMentions {
        files: vec!["/r/gen/big.js".into(), "/r/gen/copy.ts".into()],
        unchecked: vec!["/r/gen/big.js".into()],
        ..IgnoredMentions::default()
    });
    assert!(
        only_undecided.get("isPartial").is_none(),
        "{only_undecided}"
    );
    assert!(only_undecided.get("warnings").is_none(), "{only_undecided}");
    assert_eq!(
        only_undecided["coverage"]["importerScan"], SCAN_COMPLETE,
        "{only_undecided}"
    );
    assert_eq!(
        only_undecided[crate::response::paging::NOTICES],
        json!([IGNORED_UNCHECKED_NOTICE]),
        "{only_undecided}"
    );
    assert_eq!(
        lead_paths(&only_undecided),
        [json!("/r/gen/big.js")],
        "{only_undecided}"
    );

    // Beside an importer they are noted apart, never counted as importers,
    // and still lead.
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
        warning.contains("1 imports it") && !warning.contains("could not be checked"),
        "{undecided}"
    );
    assert_eq!(
        undecided[crate::response::paging::NOTICES],
        json!([IGNORED_UNCHECKED_NOTICE]),
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
        &"// x\n".repeat(crate::security::SourceLimits::REAL.whole_source / 5 + 1),
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

/// A failed scan names what failed: the candidate list, the anchor's own
/// declaration, or each candidate with its cause (unread or unopened, or
/// unresolved by the server), never only "could not read, open, or resolve
/// every candidate".
#[test]
fn a_failed_scan_names_its_step_and_each_candidate_with_its_cause() {
    let query: LspSearchQuery = serde_json::from_value(json!({
        "operation": "references", "path": "/r/a.c", "symbolName": "dictFind", "line": 1
    }))
    .expect("query");
    let scope = Scope::new("/r".into(), Vec::new());
    let warning_of = |importers: Importers| {
        let mut row = json!({"status": "success"});
        importers.annotate(&mut row, &query, &scope);
        assert_eq!(row["coverage"]["importerScan"], SCAN_FAILED, "{row}");
        assert_eq!(row["isPartial"], true, "{row}");
        row["warnings"].to_string()
    };
    let list = warning_of(Importers {
        failed: true,
        failure: ScanFailure::CandidateList,
        ..Importers::default()
    });
    assert!(
        list.contains("could not list the files that spell this name"),
        "{list}"
    );
    let files = candidates(4);
    let Cut::Window(window) = Window::cut(files.clone(), WindowRequest::default()) else {
        panic!("window 1 is never stale");
    };
    let anchor = warning_of(Importers {
        window: Some(window),
        failed: true,
        failure: ScanFailure::AnchorDeclaration,
        ..Importers::default()
    });
    assert!(anchor.contains("anchor's own declaration"), "{anchor}");
    assert!(anchor.contains("none of the 4 candidate files"), "{anchor}");
    let Cut::Window(window) = Window::cut(files.clone(), WindowRequest::default()) else {
        panic!("window 1 is never stale");
    };
    let named = warning_of(Importers {
        window: Some(window),
        failed: true,
        failure: ScanFailure::Candidates,
        unverified: vec![files[1].clone(), files[2].clone()],
        unopened: vec![files[2].clone()],
        ..Importers::default()
    });
    let relative = |file: &str| {
        std::path::Path::new(file)
            .strip_prefix("/r")
            .map_or(file.to_owned(), |path| path.to_string_lossy().into_owned())
    };
    assert!(
        named.contains(&format!(
            "{} could not be read or opened",
            relative(&files[2])
        )),
        "{named}"
    );
    assert!(
        named.contains(&format!(
            "resolved no occurrence in {}",
            relative(&files[1])
        )),
        "{named}"
    );
}

/// The import check never runs whole-file facts on a file that merely spells
/// the name, and parses no file twice: a file with no module reference is
/// decided by its text, every other by one module-references parse, and the
/// verdicts are those the whole facts give.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_import_check_decides_each_ignored_file_by_its_cheapest_check() {
    use crate::tools::declarations::parses;
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    let write = |name: &str, text: &str| {
        let path = root.join(name);
        std::fs::write(&path, text).expect("write");
        path.to_string_lossy().into_owned()
    };
    // Bodies a whole-facts extraction would read call by call.
    let body = "export function run(a: string) { return a.split(',').map((p) => p.trim()); }\n";
    let texts = [
        // Spells the name and references no module: decided by its text.
        (
            "plain.ts",
            format!(
                "const x = 1;\nx.toString();\n{}",
                body.replace("export ", "")
            ),
        ),
        // References modules, binds the name nowhere.
        (
            "other.ts",
            format!("import fs from 'node:fs';\nconst x = fs;\n{body}"),
        ),
        (
            "own.ts",
            format!("import {{ y }} from './a';\nexport const x = y;\n{body}"),
        ),
        // Importers: by name, renamed, through a namespace.
        (
            "named.ts",
            format!("import {{ x }} from './a';\nx();\n{body}"),
        ),
        (
            "renamed.ts",
            format!("import {{ x as z }} from './a';\nz();\n{body}"),
        ),
        (
            "space.ts",
            format!("import * as a from './a';\na.x();\n{body}"),
        ),
        // A CommonJS load, which only the scope build reads: no import by name.
        (
            "load.cjs",
            "const { x } = require('./a');\nx('\\n padded');\n".to_owned(),
        ),
    ];
    let files = texts
        .iter()
        .map(|(name, text)| write(name, text))
        .collect::<Vec<_>>();
    let expected = files
        .iter()
        .zip(&texts)
        .filter(|(file, (_, text))| {
            octocode_engine::signatures::extract_graph_facts(text, file)
                .is_some_and(|facts| imports_name(&facts, text, "x"))
        })
        .map(|(file, _)| file.clone())
        .collect::<Vec<_>>();
    assert_eq!(expected.len(), 3, "{expected:?}");
    // The module references are the whole facts' own, load for load.
    for (file, (_, text)) in files.iter().zip(&texts) {
        let whole = octocode_engine::signatures::extract_graph_facts(text, file).expect("whole");
        let light =
            octocode_engine::signatures::extract_module_references(text, file).expect("light");
        for (part, whole, light) in [
            ("imports", json!(whole.imports), json!(light.imports)),
            ("exports", json!(whole.exports), json!(light.exports)),
            ("loads", json!(whole.common_js), json!(light.common_js)),
            (
                "diagnostics",
                json!(whole.diagnostics),
                json!(light.diagnostics),
            ),
        ] {
            assert_eq!(whole, light, "{part} of {file}");
        }
        assert!(light.calls.is_empty(), "{file}");
    }
    let mut ignored = IgnoredMentions {
        files: files.clone(),
        ..IgnoredMentions::default()
    };
    let policy = crate::tools::test_support::workspace_policy(&root);
    let cancel = &crate::execution::cancel::NeverCancel;
    check_imports(&mut ignored, &policy, cancel, "x", None)
        .await
        .expect("check");
    assert_eq!(ignored.importing, expected);
    assert!(ignored.unchecked.is_empty(), "{ignored:?}");
    // A second request reads the kept parses.
    let mut again = IgnoredMentions {
        files: files.clone(),
        ..IgnoredMentions::default()
    };
    check_imports(&mut again, &policy, cancel, "x", None)
        .await
        .expect("check");
    assert_eq!(again.importing, expected);
    for file in &files {
        assert_eq!(parses::facts_of(file), 0, "whole facts of {file}");
        assert_eq!(parses::of("declarations", file), 0, "{file}");
        let wanted = usize::from(!file.ends_with("plain.ts"));
        assert_eq!(parses::of("moduleReferences", file), wanted, "{file}");
    }
}
