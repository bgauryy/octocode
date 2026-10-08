use super::journal::{journal_directory, persist_journal};
use super::patch::{LineOp, line_diff};
use super::*;
use crate::tools::cancel::NeverCancel;
use std::time::{SystemTime, UNIX_EPOCH};

/// Tests speak JSON rows; the runtime owns the typed parse.
fn rewrite_row(
    query: Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancellation: &dyn CancellationCheck,
    options: &AstRewriteRuntimeOptions,
) -> Value {
    let mut query = query;
    for field in ["mainGoal", "reasoning"] {
        if query[field].is_null() {
            query[field] = Value::from("test");
        }
    }
    let query: RewriteRequest = serde_json::from_value(query).expect("typed astRewrite row");
    execute_ast_rewrite_with_options(query, paths, security, cancellation, options)
}

#[test]
fn overlap_error_identifies_ranges_and_how_to_narrow_the_preview() {
    let matched = |start, end| RawMatch {
        file: "a.ts".into(),
        text: "matched".into(),
        replacement: "replaced".into(),
        range: RawRange {
            byte_offset: RawByteRange { start, end },
            start: RawPosition {
                line: 0,
                column: start as u32,
            },
            end: RawPosition {
                line: 0,
                column: end as u32,
            },
        },
        replacement_offsets: None,
        meta_variables: RawMetaVariables::default(),
    };
    let error = prepare_matches("a.ts", "before", vec![matched(0, 12), matched(5, 14)])
        .expect_err("overlap must reject preview");
    let value = error.value();
    assert_eq!(value["errorCode"], "editOverlap");
    assert!(
        value["error"].as_str().is_some_and(|message| {
            message.contains("Narrow the pattern") && message.contains("preview again")
        }),
        "{value}"
    );
    // The message and the hint agree: both narrow, neither broadens.
    let hint = value["hints"][0].as_str().expect("overlap hint");
    assert!(
        hint.contains("not:{has:") && !hint.contains("Broaden"),
        "{value}"
    );
    // Whole within the response guidance cap, so it is never clipped.
    assert!(hint.chars().count() <= 120, "{hint}");
    let nested = prepare_matches("a.ts", "before", vec![matched(0, 12), matched(0, 5)])
        .expect_err("nested matches reject preview")
        .value();
    assert!(
        nested["error"]
            .as_str()
            .is_some_and(|message| message.contains("nested inside another match")),
        "{nested}"
    );
    assert_eq!(
        value["details"]["firstRange"],
        json!({"start": 0, "end": 12})
    );
    assert_eq!(
        value["details"]["secondRange"],
        json!({"start": 5, "end": 14})
    );
}

#[test]
fn unified_patch_preserves_crlf_line_endings() {
    let patch = create_unified_patch("f.txt", "a\r\nb\r\n", "a\r\nB\r\n");
    // The CRLF endings from the source must survive into the hunk body.
    assert!(patch.contains("-b\r\n"), "patch was: {patch:?}");
    assert!(patch.contains("+B\r\n"), "patch was: {patch:?}");
    assert!(patch.starts_with("--- a/f.txt\n+++ b/f.txt\n@@ "));
}

#[test]
fn unified_patch_is_minimal_with_separate_hunks_and_context() {
    let before = "fn a() {\n    fill_goal(next, goal);\n    x();\n    y();\n    z();\n    w();\n    v();\n    u();\n    t();\n    fill_goal(next, goal);\n}\n";
    let after = before.replace("fill_goal(", "fill_goal2(");
    let patch = create_unified_patch("a.rs", before, &after);
    assert_eq!(
        patch,
        "--- a/a.rs\n+++ b/a.rs\n\
         @@ -1,5 +1,5 @@\n fn a() {\n-    fill_goal(next, goal);\n+    fill_goal2(next, goal);\n     x();\n     y();\n     z();\n\
         @@ -7,5 +7,5 @@\n     v();\n     u();\n     t();\n-    fill_goal(next, goal);\n+    fill_goal2(next, goal);\n }\n"
    );
}

#[test]
fn unified_patch_merges_near_changes_and_counts_pure_insertions() {
    // Changes 6 lines apart share one hunk: only the two changed lines are
    // marked, the lines between them are context.
    let before = "a\nb\nc\nd\ne\nf\ng\nh\n";
    let after = "A\nb\nc\nd\ne\nf\ng\nH\n";
    let patch = create_unified_patch("f", before, after);
    assert!(
        patch.contains("@@ -1,8 +1,8 @@\n-a\n+A\n b\n c\n d\n e\n f\n g\n-h\n+H\n"),
        "{patch}"
    );
    assert_eq!(patch.matches("@@ -").count(), 1, "{patch}");
    let inserted = create_unified_patch("f", "a\nb\n", "a\nx\nb\n");
    assert!(
        inserted.contains("@@ -1,2 +1,3 @@\n a\n+x\n b\n"),
        "{inserted}"
    );
    let appended = create_unified_patch("f", "", "x\n");
    assert!(appended.contains("@@ -0,0 +1,1 @@\n+x\n"), "{appended}");
}

#[test]
fn line_diff_over_budget_falls_back_to_a_replaced_block() {
    assert!(line_diff(&["a\n", "b\n"], &["c\n", "d\n"], 1).is_none());
    assert_eq!(
        line_diff(&["a\n", "b\n"], &["a\n", "c\n"], 8),
        Some(vec![LineOp::Equal, LineOp::Delete, LineOp::Insert])
    );
}

#[test]
fn unified_patch_marks_missing_final_newline() {
    // A file whose final line lacks a trailing newline must be flagged
    // rather than silently presented as newline-terminated.
    let patch = create_unified_patch("f.txt", "a\nb", "a\nB");
    assert!(
        patch.contains("\\ No newline at end of file\n"),
        "patch was: {patch:?}"
    );
}

struct Cancelled;
impl CancellationCheck for Cancelled {
    fn check(&self) -> Result<(), String> {
        Err("cancelled by test".to_owned())
    }
}

fn make_temp_dir(prefix: &str) -> Result<PathBuf, RewriteError> {
    // pid keeps names unique across processes; the atomic counter keeps them
    // unique within a process even when two threads read the same clock tick.
    // Before the counter, concurrent fixtures under `cargo test` collided on
    // `{pid}-{nanos}` and panicked on `create_dir` (AlreadyExists) — the suite
    // only went green under `--test-threads=1`.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("{prefix}{}-{unique}-{seq}", std::process::id()));
    fs::create_dir(&path).map_err(io_error)?;
    Ok(path)
}

fn fixture() -> (PathBuf, PathPolicy, ContentSecurity) {
    let root = make_temp_dir("octocode-rewrite-test-").expect("create fixture");
    fs::write(
        root.join("a.ts"),
        "const first = oldCall(1);\nconst second = oldCall(2);\n",
    )
    .expect("write fixture");
    let policy = crate::tools::test_support::workspace_policy(&root);
    let security = ContentSecurity::new();
    (root, policy, security)
}

fn query(root: &Path) -> Value {
    json!({
        "path":root,"language":"typescript","mainGoal": "test", "reasoning":"test",
        "pattern":"oldCall($A)","rewrite":"newCall($A)","pageSize":1
    })
}

/// The query and its whole-page preview (every match on one page).
fn full_preview(root: &Path, policy: &PathPolicy, security: &ContentSecurity) -> (Value, Value) {
    let mut preview_query = query(root);
    preview_query["pageSize"] = json!(100);
    let preview = rewrite_row(
        preview_query.clone(),
        policy,
        security,
        &NeverCancel,
        &Default::default(),
    );
    (preview_query, preview)
}

/// Runtime options that permit writes.
fn apply_options() -> AstRewriteRuntimeOptions {
    AstRewriteRuntimeOptions {
        allow_apply: true,
        ..Default::default()
    }
}

#[test]
fn syntax_breaking_template_is_rejected_before_any_commit() {
    let (root, policy, security) = fixture();
    let mut broken = query(&root);
    // Unbalanced replacement: splices cleanly but no longer parses.
    broken["rewrite"] = json!("newCall($A");
    let result = rewrite_row(
        broken.clone(),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    assert_eq!(result["errorCode"], "brokenSyntax", "{result}");
    // The fix is the template, never a broader pattern.
    let hint = result["hints"][0].as_str().unwrap_or_default();
    assert!(
        hint.contains("template") && !hint.contains("Broaden"),
        "{result}"
    );

    // The removed escape hatch is rejected rather than silently ignored.
    broken["allowSyntaxRegression"] = json!(true);
    assert!(serde_json::from_value::<RewriteRequest>(broken).is_err());
}

#[test]
fn an_unparseable_pattern_is_an_invalid_pattern_in_every_form() {
    let (root, policy, security) = fixture();
    let mut with_language = query(&root);
    with_language["pattern"] = json!("neu(");
    let mut inferred = with_language.clone();
    inferred.as_object_mut().map(|row| row.remove("language"));
    for row in [with_language, inferred] {
        let result = rewrite_row(row, &policy, &security, &NeverCancel, &Default::default());
        assert_eq!(result["errorCode"], "invalidPattern", "{result}");
        let error = result["error"].as_str().unwrap_or_default();
        assert!(
            !error.starts_with('[') && !error.contains("AST kinds"),
            "engine text stays inside: {result}"
        );
        let hint = result["hints"][0].as_str().unwrap_or_default();
        assert!(hint.contains("syntaxTree"), "{result}");
    }
}

/// Each matched file is parsed once by the scan and once staged; the
/// syntax check and the postcondition reuse those trees, and the rule is
/// validated by compiling it rather than parsing an empty probe.
/// `include` reads like astSearch's: a bare word or a plain path scopes to
/// the files under the directory it names, so a search and its rewrite see
/// the same files.
#[test]
fn include_scopes_to_the_files_under_a_named_directory() {
    let (root, policy, security) = fixture();
    fs::create_dir_all(root.join("src/api")).expect("api dir");
    fs::write(root.join("src/api/handler.ts"), "oldCall(3);\n").expect("api file");
    for include in ["api", "src/api"] {
        let mut preview = query(&root);
        preview["pageSize"] = json!(10);
        preview["include"] = json!([include]);
        let row = rewrite_row(
            preview,
            &policy,
            &security,
            &NeverCancel,
            &Default::default(),
        );
        assert_eq!(row["matchCount"], 1, "include {include}: {row}");
        assert!(
            row.to_string().contains("handler.ts"),
            "include {include}: {row}"
        );
    }
}

#[test]
fn preview_and_apply_parse_each_file_twice() {
    let (root, policy, security) = fixture();
    fs::write(root.join("b.ts"), "oldCall(3);\n").expect("second file");
    let mut preview = query(&root);
    preview["pageSize"] = json!(10);
    staged::take_parses();
    let first = rewrite_row(
        preview,
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    assert_eq!(first["matchCount"], 3, "{first}");
    assert_eq!(staged::take_parses(), 4, "2 files × (scan + staged)");
    // The lead that writes says so, and carries no preview paging.
    let lead = &first["next"]["apply"];
    assert!(
        lead["why"]
            .as_str()
            .is_some_and(|why| why.starts_with("Writes")),
        "{first}"
    );
    assert!(
        lead["query"]["queries"][0].get("pageSize").is_none(),
        "{first}"
    );
    assert!(
        lead["query"]["queries"][0].get("ruleKind").is_none(),
        "{first}"
    );
    let mut apply = lead["query"]["queries"][0].clone();
    apply["postconditions"] = json!([{"kind":"remainingMatches","equals":0}]);
    let options = apply_options();
    let applied = rewrite_row(apply, &policy, &security, &NeverCancel, &options);
    assert_eq!(applied["mode"], "apply", "{applied}");
    // After apply, a read-only lead confirms the old pattern is gone.
    let verify = &applied["next"]["verify"];
    assert_eq!(verify["tool"], "astSearch", "{applied}");
    let row = &verify["query"]["queries"][0];
    assert_eq!(row["operation"], "match", "{applied}");
    assert_eq!(row["pattern"], "oldCall($A)", "{applied}");
    assert_eq!(
        staged::take_parses(),
        4,
        "postcondition reuses the staged parse"
    );
}

/// Apply states what changed on disk by hash; the patch and match rows
/// were already shown by the preview.
#[test]
fn apply_returns_a_hash_receipt_not_the_preview_again() {
    let (root, policy, security) = fixture();
    let (_, preview) = full_preview(&root, &policy, &security);
    let options = apply_options();
    let applied = rewrite_row(
        preview["next"]["apply"]["query"]["queries"][0].clone(),
        &policy,
        &security,
        &NeverCancel,
        &options,
    );
    assert_eq!(applied["transaction"]["committed"], true, "{applied}");
    assert!(applied.get("matches").is_none(), "{applied}");
    assert!(applied.get("root").is_none(), "{applied}");
    assert!(applied["transaction"].get("files").is_none(), "{applied}");
    let file = &applied["files"][0];
    assert!(file.get("patch").is_none(), "{applied}");
    assert!(file.get("beforeHash").is_none(), "{applied}");
    assert_eq!(file["matchCount"], 2, "{applied}");
    let bytes = fs::read(root.join(file["path"].as_str().expect("path"))).expect("read");
    assert_eq!(file["afterHash"], sha256(&bytes), "{applied}");
    fs::remove_dir_all(root).expect("cleanup");
}

/// The receipt is the apply shape, not a debug-off view: `debug` adds the
/// executable/isolation diagnostics but never re-sends the preview.
#[test]
fn debug_apply_still_returns_the_receipt() {
    let (root, policy, security) = fixture();
    let (_, preview) = full_preview(&root, &policy, &security);
    let options = apply_options();
    let mut apply = preview["next"]["apply"]["query"]["queries"][0].clone();
    apply["debug"] = json!(true);
    let applied = rewrite_row(apply, &policy, &security, &NeverCancel, &options);
    assert_eq!(applied["transaction"]["committed"], true, "{applied}");
    assert!(applied.get("matches").is_none(), "{applied}");
    assert!(applied.get("root").is_none(), "{applied}");
    assert!(applied["transaction"].get("files").is_none(), "{applied}");
    assert!(applied["files"][0].get("patch").is_none(), "{applied}");
    assert_eq!(applied["executable"]["path"], "native", "{applied}");
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn complete_preview_offers_an_executable_guarded_apply() {
    let (root, policy, security) = fixture();
    let first = rewrite_row(
        query(&root),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    // A partial preview page never offers apply.
    assert!(first["next"].get("apply").is_none(), "{}", first["next"]);
    let last = rewrite_row(
        first["next"]["nextPage"]["query"]["queries"][0].clone(),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    let apply = last["next"]["apply"]["query"]["queries"][0].clone();
    assert_eq!(apply["apply"], true, "{apply}");
    assert_eq!(apply["snapshot"], last["snapshot"]);
    let options = apply_options();
    let applied = rewrite_row(apply.clone(), &policy, &security, &NeverCancel, &options);
    assert_eq!(applied["mode"], "apply", "{applied}");
    let replay = rewrite_row(apply, &policy, &security, &NeverCancel, &options);
    assert_eq!(replay["errorCode"], "staleSnapshot", "{replay}");
}

#[test]
fn capped_scan_pages_remain_partial_and_apply_only_the_guarded_subset() {
    let (root, policy, security) = fixture();
    let untouched = "const third = oldCall(3);\n";
    fs::write(root.join("b.ts"), untouched).expect("second source");
    fs::write(root.join("c.ts"), untouched).expect("third source");
    let mut capped = query(&root);
    capped["maxFiles"] = json!(1);
    let first = rewrite_row(
        capped,
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    assert_eq!(first["coverage"]["scanTruncated"], true, "{first}");
    assert_eq!(first["isPartial"], true, "{first}");
    assert_eq!(first["isPartial"], true, "{first}");
    assert_eq!(first["terminalLimit"], true, "{first}");
    assert!(first["next"].get("apply").is_none(), "{first}");

    let last = rewrite_row(
        first["next"]["nextPage"]["query"]["queries"][0].clone(),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    assert_eq!(last["pagination"]["hasMore"], false, "{last}");
    assert_eq!(last["isPartial"], true, "{last}");
    assert_eq!(last["isPartial"], true, "{last}");
    assert_eq!(last["terminalLimit"], true, "{last}");
    let apply = last["next"]["apply"]["query"]["queries"][0].clone();
    assert_eq!(
        apply["expectedHashes"]
            .as_object()
            .map(|hashes| hashes.len()),
        Some(1)
    );
    let options = apply_options();
    let before = fs::read(root.join("a.ts")).expect("selected source");
    fs::write(root.join("a.ts"), "const changed = oldCall(9);\n").expect("change source");
    let rejected = rewrite_row(apply.clone(), &policy, &security, &NeverCancel, &options);
    assert_eq!(rejected["errorCode"], "staleSnapshot", "{rejected}");
    assert_eq!(
        fs::read_to_string(root.join("a.ts")).expect("unchanged rejected source"),
        "const changed = oldCall(9);\n"
    );
    fs::write(root.join("a.ts"), before).expect("restore selected source");
    let applied = rewrite_row(apply, &policy, &security, &NeverCancel, &options);
    assert_eq!(applied["transaction"]["committed"], true, "{applied}");
    assert_eq!(applied["isPartial"], true, "{applied}");
    assert_eq!(applied["isPartial"], true, "{applied}");
    assert_eq!(applied["terminalLimit"], true, "{applied}");
    assert_eq!(
        fs::read_to_string(root.join("a.ts")).expect("rewritten selected source"),
        "const first = newCall(1);\nconst second = newCall(2);\n"
    );
    for name in ["b.ts", "c.ts"] {
        assert_eq!(
            fs::read_to_string(root.join(name)).expect("unselected source"),
            untouched
        );
    }
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn preview_continuation_is_lossless_and_apply_is_hash_guarded() {
    let (root, policy, security) = fixture();
    let first = rewrite_row(
        query(&root),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    assert_eq!(first["mode"], "preview");
    assert_eq!(first["matchCount"], 2);
    assert_eq!(first["matches"].as_array().map(Vec::len), Some(1));
    let second = rewrite_row(
        first["next"]["nextPage"]["query"]["queries"][0].clone(),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    let ids = [
        first["matches"][0]["id"].clone(),
        second["matches"][0]["id"].clone(),
    ];
    assert_ne!(ids[0], ids[1]);
    let mut apply = query(&root);
    apply["apply"] = json!(true);
    apply["snapshot"] = first["snapshot"].clone();
    // Rows carry absolute paths at the tool; apply accepts absolute keys as
    // well as the workspace-relative keys of hints.apply.
    apply["expectedHashes"] = json!({
        first["files"][0]["path"].as_str().expect("path"):
            first["files"][0]["beforeHash"].clone()
    });
    let applied = rewrite_row(apply, &policy, &security, &NeverCancel, &apply_options());
    assert_eq!(applied["transaction"]["committed"], true);
    assert_eq!(
        fs::read_to_string(root.join("a.ts")).expect("read"),
        "const first = newCall(1);\nconst second = newCall(2);\n"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn stale_source_postcondition_and_cancellation_never_mutate() {
    let (root, policy, security) = fixture();
    let preview = rewrite_row(
        query(&root),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    let original = fs::read_to_string(root.join("a.ts")).expect("read");
    fs::write(root.join("a.ts"), format!("{original}// drift\n")).expect("drift");
    let mut apply = query(&root);
    apply["apply"] = json!(true);
    apply["snapshot"] = preview["snapshot"].clone();
    let absolute = root.join(preview["files"][0]["path"].as_str().expect("path"));
    apply["expectedHashes"] = json!({
        absolute.to_string_lossy():
            preview["files"][0]["beforeHash"].clone()
    });
    assert_eq!(
        rewrite_row(apply, &policy, &security, &NeverCancel, &apply_options())["errorCode"],
        "staleSnapshot"
    );
    assert_eq!(
        rewrite_row(
            query(&root),
            &policy,
            &security,
            &Cancelled,
            &Default::default()
        )["errorCode"],
        "cancelled"
    );
    assert!(
        fs::read_to_string(root.join("a.ts"))
            .expect("read")
            .ends_with("// drift\n")
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn selection_and_failed_postcondition_preserve_unselected_bytes() {
    let (root, policy, security) = fixture();
    let (preview_query, preview) = full_preview(&root, &policy, &security);
    let mut apply = preview_query;
    apply["apply"] = json!(true);
    apply["snapshot"] = preview["snapshot"].clone();
    // Preview rows carry a 16-hex id prefix; apply accepts it.
    apply["selectedMatchIds"] = json!([preview["matches"][0]["id"]]);
    let absolute = preview["files"][0]["path"].as_str().expect("path");
    // A complete preview states each file hash once, in hints.apply, keyed
    // by the workspace-relative path (here the workspace is the root).
    assert!(preview["files"][0].get("beforeHash").is_none(), "{preview}");
    apply["expectedHashes"] = json!({
        absolute:
            preview["next"]["apply"]["query"]["queries"][0]["expectedHashes"]["a.ts"].clone()
    });
    apply["postconditions"] = json!([{"kind":"remainingMatches","equals":0}]);
    let failed = rewrite_row(apply, &policy, &security, &NeverCancel, &apply_options());
    assert_eq!(failed["errorCode"], "postconditionFailed");
    assert_eq!(failed["details"]["scope"], "rewrittenFiles", "{failed}");
    assert_eq!(
        failed["details"]["scannedFiles"],
        json!(["a.ts"]),
        "{failed}"
    );
    assert!(
        failed["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("remainingMatches")),
        "{failed}"
    );
    assert_eq!(
        fs::read_to_string(root.join("a.ts")).expect("read"),
        "const first = oldCall(1);\nconst second = oldCall(2);\n"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn preview_pages_carry_only_their_files_with_one_based_lines() {
    let (root, policy, security) = fixture();
    fs::write(root.join("b.ts"), "const third = oldCall(3);\n").expect("write b");
    let mut preview_query = query(&root);
    preview_query["pageSize"] = json!(2);
    let first = rewrite_row(
        preview_query.clone(),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    assert_eq!(first["affectedFiles"], 2, "{first}");
    let first_files = first["files"].as_array().expect("files");
    assert_eq!(first_files.len(), 1, "page 1 touches only a.ts: {first}");
    // Rows carry the absolute path; the response stage names it relative
    // to the workspace root (D4).
    assert_eq!(
        first_files[0]["path"],
        json!(
            fs::canonicalize(&root)
                .expect("canonical root")
                .join("a.ts")
        )
    );
    // Match rows locate a hunk: a 16-hex id prefix, path and line; the
    // patch already shows the text and its replacement.
    let matched = &first["matches"][0];
    assert_eq!(matched["line"], 1, "{matched}");
    assert_eq!(first["matches"][1]["line"], 2);
    assert_eq!(matched["id"].as_str().map(str::len), Some(16), "{matched}");
    for dropped in ["range", "byteRange", "text", "replacement", "captures"] {
        assert!(matched.get(dropped).is_none(), "{dropped}: {matched}");
    }
    for dropped in ["afterHash", "patchBytes", "absolutePath"] {
        assert!(first_files[0].get(dropped).is_none(), "{dropped}: {first}");
    }
    // The executable/isolation receipts are debug-only diagnostics.
    assert!(first.get("executable").is_none(), "{first}");
    assert!(first.get("isolation").is_none(), "{first}");

    let second = rewrite_row(
        first["next"]["nextPage"]["query"]["queries"][0].clone(),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    let second_files = second["files"].as_array().expect("files");
    assert_eq!(second_files.len(), 1, "{second}");
    assert_eq!(
        second_files[0]["path"],
        json!(
            fs::canonicalize(&root)
                .expect("canonical root")
                .join("b.ts")
        )
    );
    // The final page's guarded apply still covers every affected file.
    let apply = second["next"]["apply"]["query"]["queries"][0].clone();
    let hashes = apply["expectedHashes"].as_object().expect("hashes");
    assert_eq!(hashes.len(), 2, "{apply}");

    let options = apply_options();
    let mut wrong = apply.clone();
    wrong["expectedHashes"]["a.ts"] = json!("0".repeat(64));
    let mismatch = rewrite_row(wrong, &policy, &security, &NeverCancel, &options);
    assert_eq!(mismatch["errorCode"], "hashMismatch", "{mismatch}");
    assert!(
        mismatch["next"]["restart"]["query"]["queries"][0].is_object(),
        "{mismatch}"
    );
    assert!(
        mismatch["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("next.restart")),
        "{mismatch}"
    );

    let applied = rewrite_row(apply, &policy, &security, &NeverCancel, &options);
    assert_eq!(applied["transaction"]["committed"], true, "{applied}");
    assert!(
        applied["transaction"].get("beforeHashes").is_none(),
        "{applied}"
    );
    assert!(
        applied["transaction"].get("afterHashes").is_none(),
        "{applied}"
    );
    assert_eq!(
        applied["files"].as_array().map(Vec::len),
        Some(2),
        "{applied}"
    );
    assert_eq!(
        fs::read_to_string(root.join("b.ts")).expect("read"),
        "const third = newCall(3);\n"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn preview_pages_carry_only_the_patch_hunks_of_their_own_matches() {
    let (root, policy, security) = fixture();
    // pageSize 1 over a.ts's two matches: each page shows one match, so
    // each page's patch holds only that match's hunk, not the whole file.
    let first = rewrite_row(
        query(&root),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    let first_file = &first["files"][0];
    assert_eq!(
        first_file["path"],
        json!(
            fs::canonicalize(&root)
                .expect("canonical root")
                .join("a.ts")
        ),
        "{first}"
    );
    let first_patch = first_file["patch"].as_str().expect("page-1 patch");
    assert!(
        first_patch.contains("+const first = newCall(1);"),
        "{first}"
    );
    assert!(!first_patch.contains("newCall(2)"), "{first}");
    assert_eq!(first_file["matchCount"], 2, "{first}");
    assert_eq!(first_file["patchMatchCount"], 1, "{first}");
    assert!(first_file.get("patchBytes").is_none(), "{first}");

    let second = rewrite_row(
        first["next"]["nextPage"]["query"]["queries"][0].clone(),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    let second_file = &second["files"][0];
    assert_eq!(
        second_file["path"],
        json!(
            fs::canonicalize(&root)
                .expect("canonical root")
                .join("a.ts")
        ),
        "{second}"
    );
    let second_patch = second_file["patch"].as_str().expect("page-2 patch");
    assert!(
        second_patch.contains("+const second = newCall(2);"),
        "{second}"
    );
    assert!(!second_patch.contains("newCall(1)"), "{second}");
    assert!(second_file.get("patchOnPage").is_none(), "{second}");
    // The final page states the file hash once, in its guarded apply,
    // which still covers the whole file.
    assert!(second_file.get("beforeHash").is_none(), "{second}");
    assert!(second_file.get("afterHash").is_none(), "{second}");
    let apply = &second["next"]["apply"]["query"]["queries"][0];
    assert_eq!(apply["expectedHashes"]["a.ts"], first_file["beforeHash"]);

    // One page covering every match keeps the whole-file patch.
    let mut whole = query(&root);
    whole["pageSize"] = json!(10);
    let all = rewrite_row(whole, &policy, &security, &NeverCancel, &Default::default());
    let all_patch = all["files"][0]["patch"].as_str().expect("whole patch");
    assert!(all_patch.contains("newCall(1)") && all_patch.contains("newCall(2)"));
    assert!(all["files"][0].get("patchMatchCount").is_none(), "{all}");
    assert_eq!(
        fs::read_to_string(root.join("a.ts")).expect("read"),
        "const first = oldCall(1);\nconst second = oldCall(2);\n",
        "preview must never write"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn debug_preview_keeps_full_match_and_file_rows() {
    let (root, policy, security) = fixture();
    let mut preview_query = query(&root);
    preview_query["debug"] = json!(true);
    let preview = rewrite_row(
        preview_query,
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    let matched = &preview["matches"][0];
    assert_eq!(matched["id"].as_str().map(str::len), Some(64), "{matched}");
    assert_eq!(matched["range"]["start"]["line"], 1, "{matched}");
    assert_eq!(matched["text"], "oldCall(1)", "{matched}");
    assert_eq!(matched["replacement"], "newCall(1)", "{matched}");
    let file = &preview["files"][0];
    for kept in ["afterHash", "patchBytes"] {
        assert!(file.get(kept).is_some(), "{kept}: {preview}");
    }
    // `path` is absolute at the tool; the response stage relativizes it.
    assert!(file.get("absolutePath").is_none(), "{preview}");
    assert_eq!(
        file["path"],
        json!(
            fs::canonicalize(&root)
                .expect("canonical root")
                .join("a.ts")
        ),
        "{preview}"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn selected_match_id_prefixes_must_name_exactly_one_match() {
    let (root, policy, security) = fixture();
    let (_, preview) = full_preview(&root, &policy, &security);
    let options = apply_options();
    let mut apply = preview["next"]["apply"]["query"]["queries"][0].clone();
    let short = preview["matches"][1]["id"].as_str().expect("id")[..12].to_owned();
    apply["selectedMatchIds"] = json!(["f".repeat(16)]);
    let unknown = rewrite_row(apply.clone(), &policy, &security, &NeverCancel, &options);
    assert_eq!(unknown["errorCode"], "selectionInvalid", "{unknown}");
    apply["selectedMatchIds"] = json!([short]);
    let applied = rewrite_row(apply, &policy, &security, &NeverCancel, &options);
    assert_eq!(applied["transaction"]["committed"], true, "{applied}");
    assert_eq!(
        fs::read_to_string(root.join("a.ts")).expect("read"),
        "const first = oldCall(1);\nconst second = newCall(2);\n"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn yaml_rule_and_inferred_language_preview_like_the_explicit_object_rule() {
    let (root, policy, security) = fixture();
    let explicit = rewrite_row(
        json!({"path":root,"language":"typescript","mainGoal":"test","reasoning":"test",
            "rule":{"pattern":"oldCall($A)"},"fix":"newCall($A)","pageSize":10}),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    for row in [
        json!({"path":root,"mainGoal":"test","reasoning":"test","rule":"pattern: oldCall($A)","fix":"newCall($A)","pageSize":10}),
        json!({"path":root,"mainGoal":"test","reasoning":"test",
            "rule":"id: rename\nlanguage: typescript\nrule:\n  pattern: oldCall($A)\n","fix":"newCall($A)","pageSize":10}),
    ] {
        let inferred = rewrite_row(row, &policy, &security, &NeverCancel, &Default::default());
        assert_eq!(inferred["files"], explicit["files"], "{inferred}");
        assert_eq!(inferred["snapshot"], explicit["snapshot"], "{inferred}");
        assert_eq!(
            inferred["next"]["apply"]["query"]["queries"][0]["language"], "typescript",
            "{inferred}"
        );
    }
    // Relational keys too: both shapes are one canonical rule.
    let nested = |rule: Value| {
        rewrite_row(
            json!({"path":root,"language":"typescript","mainGoal":"test","reasoning":"test",
                "rule":rule,"fix":"newCall($A)","pageSize":10}),
            &policy,
            &security,
            &NeverCancel,
            &Default::default(),
        )["snapshot"]
            .clone()
    };
    assert_eq!(
        nested(json!(
            "pattern: oldCall($A)\ninside:\n  kind: lexical_declaration\n  stopBy: end"
        )),
        nested(
            json!({"pattern":"oldCall($A)","inside":{"kind":"lexical_declaration","stopBy":"end"}})
        ),
    );
    let foreign = serde_json::from_value::<RewriteRequest>(
        json!({"path":root,"mainGoal":"test","reasoning":"test","rule":"rule:\n  pattern: oldCall($A)\nfix: x\n","fix":"newCall($A)"}),
    )
    .expect_err("a rule-file fix inside the rule string is not read");
    assert!(
        foreign.to_string().contains("rule file key `fix`"),
        "{foreign}"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

/// AS5: a rule-config object `{rule, constraints?, utils?, transform?}` in
/// `rule`, the same YAML rule file, and the split fields preview alike; a
/// section given twice is an error, never a silent pick.
#[test]
fn rule_config_object_matches_like_yaml_rule_file() {
    let (root, policy, security) = fixture();
    let preview = |extra: Value| {
        let mut row =
            json!({"path":root,"language":"typescript","fix":"newCall($B)","pageSize":10});
        for (key, value) in extra.as_object().expect("fields") {
            row[key] = value.clone();
        }
        rewrite_row(row, &policy, &security, &NeverCancel, &Default::default())
    };
    let constraints = json!({"A":{"regex":"^1$"}});
    let transform = json!({"B":{"substring":{"source":"$A"}}});
    let split = preview(
        json!({"rule":{"pattern":"oldCall($A)"},"constraints":constraints,"transform":transform}),
    );
    assert_eq!(split["matchCount"], 1, "{split}");
    let object = preview(
        json!({"rule":{"rule":{"pattern":"oldCall($A)"},"constraints":constraints,"transform":transform}}),
    );
    let yaml = preview(
        json!({"rule":"id: x\nrule:\n  pattern: oldCall($A)\nconstraints:\n  A:\n    regex: ^1$\ntransform:\n  B:\n    substring:\n      source: $A\n"}),
    );
    for shape in [&object, &yaml] {
        assert_eq!(shape["files"], split["files"], "{shape}");
        assert_eq!(shape["snapshot"], split["snapshot"], "{shape}");
    }
    let twice = serde_json::from_value::<RewriteRequest>(json!({"path":root,
        "rule":{"rule":{"pattern":"oldCall($A)"},"constraints":constraints},
        "constraints":constraints,"fix":"newCall($A)"}))
    .expect_err("a section given twice");
    assert!(twice.to_string().contains("`constraints`"), "{twice}");
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn directory_with_several_grammars_requires_language() {
    let (root, policy, security) = fixture();
    fs::write(root.join("b.rs"), "fn b() { oldCall(3); }\n").expect("rust file");
    let mixed = rewrite_row(
        json!({"path":root,"mainGoal":"test","reasoning":"test","pattern":"oldCall($A)","rewrite":"newCall($A)"}),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    assert_eq!(mixed["errorCode"], "languageRequired", "{mixed}");
    assert!(mixed.to_string().contains("typescript"), "{mixed}");
    fs::remove_dir_all(root).expect("cleanup");
}

/// An unknown `language` fails as astSearch's does: `languageUnsupported`
/// (not a generic invalidInput), naming the accepted selector forms.
#[test]
fn unsupported_language_is_language_unsupported_like_ast_search() {
    let (root, policy, security) = fixture();
    let row = rewrite_row(
        json!({"path":root,"language":"cobol","pattern":"oldCall($A)","rewrite":"newCall($A)"}),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    assert_eq!(row["errorCode"], "languageUnsupported", "{row}");
    assert!(
        row.to_string()
            .contains("is not a supported structural grammar"),
        "{row}"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn interrupted_multi_file_transaction_is_rolled_back_from_journal() {
    let root = make_temp_dir("octocode-rewrite-recovery-test-").expect("fixture");
    let id = "recovery-fixture";
    let targets = [root.join("a.ts"), root.join("b.ts")];
    let before = [b"old-a\n".to_vec(), b"old-b\n".to_vec()];
    let after = [b"new-a\n".to_vec(), b"new-b\n".to_vec()];
    for (target, bytes) in targets.iter().zip(&before) {
        fs::write(target, bytes).expect("write original");
    }
    let journal_dir = journal_directory(&root);
    fs::create_dir_all(&journal_dir).expect("journal directory");
    let journal_path = journal_dir.join(format!("{JOURNAL_PREFIX}{id}.json"));
    let mut journal = Journal {
        version: 1,
        id: id.to_owned(),
        root: root.clone(),
        phase: JournalPhase::Committing,
        files: targets
            .iter()
            .enumerate()
            .map(|(index, target)| JournalFile {
                target: target.clone(),
                stage: target.with_file_name(format!(".octocode-{id}.stage-{index}")),
                backup: target.with_file_name(format!(".octocode-{id}.backup-{index}")),
                before_hash: sha256(&before[index]),
                after_hash: sha256(&after[index]),
                state: FileState::Planned,
            })
            .collect(),
    };
    persist_journal(&journal_path, &journal).expect("journal");
    for (index, (file, after_bytes)) in journal
        .files
        .iter_mut()
        .zip(after.iter())
        .take(2)
        .enumerate()
    {
        fs::write(&file.stage, after_bytes).expect("stage");
        fs::rename(&file.target, &file.backup).expect("backup");
        file.state = FileState::BackedUp;
        if index == 0 {
            fs::rename(&file.stage, &file.target).expect("promote");
            file.state = FileState::Promoted;
        }
    }
    persist_journal(&journal_path, &journal).expect("persist interruption");

    recover_transactions(&root, &NeverCancel).expect("recover");
    for index in 0..2 {
        assert_eq!(fs::read(&targets[index]).expect("restored"), before[index]);
        assert!(!journal.files[index].stage.exists());
        assert!(!journal.files[index].backup.exists());
    }
    assert!(!journal_path.exists());
    fs::remove_dir_all(root).expect("cleanup");
}

/// A one-file journal on `root/a.ts` in `phase`/`state`, with the stage
/// written; the caller arranges the target and backup.
fn single_file_journal(
    root: &Path,
    id: &str,
    phase: JournalPhase,
    state: FileState,
) -> (PathBuf, JournalFile) {
    let target = root.join("a.ts");
    let file = JournalFile {
        target: target.clone(),
        stage: target.with_file_name(format!(".octocode-{id}.stage-0")),
        backup: target.with_file_name(format!(".octocode-{id}.backup-0")),
        before_hash: sha256(b"old\n"),
        after_hash: sha256(b"new\n"),
        state,
    };
    let returned = JournalFile {
        target: file.target.clone(),
        stage: file.stage.clone(),
        backup: file.backup.clone(),
        before_hash: file.before_hash.clone(),
        after_hash: file.after_hash.clone(),
        state,
    };
    let journal_dir = journal_directory(root);
    fs::create_dir_all(&journal_dir).expect("journal directory");
    let journal_path = journal_dir.join(format!("{JOURNAL_PREFIX}{id}.json"));
    let journal = Journal {
        version: 1,
        id: id.to_owned(),
        root: root.to_path_buf(),
        phase,
        files: vec![file],
    };
    persist_journal(&journal_path, &journal).expect("journal");
    (journal_path, returned)
}

/// A staged target that was edited by someone else before its backup was
/// taken was never moved by the transaction: recovery drops the stage, keeps
/// the external bytes, and retires the journal so later applies still run.
#[test]
fn recovery_keeps_an_external_edit_to_a_target_that_was_never_backed_up() {
    let root = make_temp_dir("octocode-rewrite-external-staged-").expect("fixture");
    let (journal_path, file) = single_file_journal(
        &root,
        "external-staged",
        JournalPhase::Prepared,
        FileState::Staged,
    );
    fs::write(&file.stage, b"new\n").expect("stage");
    fs::write(&file.target, b"edited elsewhere\n").expect("external edit");

    let warnings = recover_transactions(&root, &NeverCancel).expect("recovery settles");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(
        fs::read(&file.target).expect("target"),
        b"edited elsewhere\n"
    );
    assert!(!file.stage.exists());
    assert!(!journal_path.exists());
    recover_transactions(&root, &NeverCancel).expect("later applies are not blocked");
    fs::remove_dir_all(root).expect("cleanup");
}

/// A target edited after its transaction committed (the process died before
/// cleanup) keeps the edit; cleanup still removes the artifacts and retires
/// the journal instead of blocking every later apply.
#[test]
fn recovery_retires_a_committed_journal_whose_target_was_edited_after_commit() {
    let root = make_temp_dir("octocode-rewrite-external-committed-").expect("fixture");
    let (journal_path, file) = single_file_journal(
        &root,
        "external-committed",
        JournalPhase::Committed,
        FileState::Promoted,
    );
    fs::write(&file.backup, b"old\n").expect("backup");
    fs::write(&file.target, b"edited after commit\n").expect("external edit");

    let warnings = recover_transactions(&root, &NeverCancel).expect("recovery settles");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(
        fs::read(&file.target).expect("target"),
        b"edited after commit\n"
    );
    assert!(!file.backup.exists());
    assert!(!journal_path.exists());
    fs::remove_dir_all(root).expect("cleanup");
}

/// The live path: a target edited between prepare and verify fails the
/// apply as `transactionFailed`, and recovery leaves no journal behind.
#[test]
fn an_edit_between_prepare_and_verify_fails_once_and_leaves_no_journal() {
    let root = make_temp_dir("octocode-rewrite-external-live-").expect("fixture");
    let target = root.join("a.ts");
    fs::write(&target, b"edited elsewhere\n").expect("external edit");
    let prepared = PreparedFile {
        path: "a.ts".to_owned(),
        shown: "a.ts".to_owned(),
        absolute: target.clone(),
        before_hash: sha256(b"old\n"),
        after_hash: sha256(b"new\n"),
        before: b"old\n".to_vec(),
        after: b"new\n".to_vec(),
        patch: String::new(),
        matches: Vec::new(),
        permissions: fs::metadata(&target).expect("metadata").permissions(),
        before_errors: 0,
        after_facts: None,
    };
    let error = super::journal::commit_transaction(&root, &[prepared], &NeverCancel)
        .expect_err("verify fails");
    assert_eq!(error.code, "transactionFailed");
    let rollback = &error.details.as_ref().expect("details")["rollback"];
    assert_eq!(rollback["restored"], true, "{rollback}");
    assert_eq!(
        rollback["warnings"].as_array().map(Vec::len),
        Some(1),
        "{rollback}"
    );
    assert_eq!(fs::read(&target).expect("target"), b"edited elsewhere\n");
    recover_transactions(&root, &NeverCancel).expect("no journal blocks the next apply");
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn invalid_recovery_journal_cannot_touch_an_outside_file() {
    let root = make_temp_dir("octocode-rewrite-journal-test-").expect("fixture");
    let outside = root.parent().expect("parent").join(format!(
        "octocode-rewrite-outside-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::write(&outside, b"keep\n").expect("outside");
    let id = "malicious";
    let journal = Journal {
        version: 1,
        id: id.to_owned(),
        root: root.clone(),
        phase: JournalPhase::Committing,
        files: vec![JournalFile {
            target: outside.clone(),
            stage: outside.with_file_name(format!(".octocode-{id}.stage-0")),
            backup: outside.with_file_name(format!(".octocode-{id}.backup-0")),
            before_hash: sha256(b"keep\n"),
            after_hash: sha256(b"changed\n"),
            state: FileState::Planned,
        }],
    };
    let journal_dir = journal_directory(&root);
    fs::create_dir_all(&journal_dir).expect("journal directory");
    let path = journal_dir.join(format!("{JOURNAL_PREFIX}{id}.json"));
    persist_journal(&path, &journal).expect("journal");
    let error = recover_transactions(&root, &NeverCancel).expect_err("reject journal");
    assert_eq!(error.code, "recoveryFailed");
    assert_eq!(fs::read(&outside).expect("outside preserved"), b"keep\n");
    fs::remove_file(outside).expect("outside cleanup");
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn embedded_engine_supports_inline_rules_without_an_executable() {
    let (root, policy, security) = fixture();
    let result = rewrite_row(
        json!({
            "path":root,
            "language":"typescript",
            "mainGoal": "test", "reasoning":"test",
            "rule":{"pattern":"oldCall($A)"},
            "fix":"newCall($A)",
            "pageSize":100,
            "debug":true
        }),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    assert_eq!(result["matchCount"], 2);
    assert_eq!(result["isolation"]["workingDirectory"], "ephemeral");
    assert_eq!(result["executable"]["path"], "native");
    assert_eq!(result["executable"]["version"], "embedded");
    assert_eq!(result["executable"]["capabilityDigest"], "native");
    fs::remove_dir_all(root).expect("cleanup");
}

/// B11: preview page 2 of an unchanged scope reuses page 1's prepared files
/// (no second scan, rewrite, or syntax check) and shows the same result a
/// fresh prepare would.
#[test]
fn preview_page_two_reuses_prepared_files() {
    let (root, policy, security) = fixture();
    let first = rewrite_row(
        query(&root),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    let next = first["next"]["nextPage"]["query"]["queries"][0].clone();
    let before = prepares();
    let second = rewrite_row(
        next.clone(),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    assert_eq!(
        prepares(),
        before,
        "page 2 reused the prepared files: {second}"
    );
    assert_eq!(second["matches"][0]["line"], 2, "{second}");
    assert_eq!(second["snapshot"], first["snapshot"], "{second}");
    fs::remove_dir_all(root).expect("cleanup");
}

/// B11: a file edited between pages is never served from the memo: the
/// page re-prepares and the changed snapshot restarts the preview.
#[test]
fn edited_file_between_preview_pages_restarts() {
    let (root, policy, security) = fixture();
    let first = rewrite_row(
        query(&root),
        &policy,
        &security,
        &NeverCancel,
        &Default::default(),
    );
    let next = first["next"]["nextPage"]["query"]["queries"][0].clone();
    fs::write(
        root.join("a.ts"),
        "const first = oldCall(1);\nconst second = oldCall(22);\n",
    )
    .expect("edit");
    let before = prepares();
    let second = rewrite_row(next, &policy, &security, &NeverCancel, &Default::default());
    assert_eq!(prepares(), before + 1, "{second}");
    assert_eq!(second["errorCode"], "staleSnapshot", "{second}");
    fs::remove_dir_all(root).expect("cleanup");
}

/// B11: apply never uses the preview memo; it prepares under its lock.
#[test]
fn apply_always_reprepares() {
    let (root, policy, security) = fixture();
    let (_, preview) = full_preview(&root, &policy, &security);
    let apply = preview["next"]["apply"]["query"]["queries"][0].clone();
    let before = prepares();
    let options = AstRewriteRuntimeOptions {
        allow_apply: true,
        ..Default::default()
    };
    let applied = rewrite_row(apply, &policy, &security, &NeverCancel, &options);
    assert_eq!(prepares(), before + 1, "{applied}");
    assert_eq!(applied["mode"], "apply", "{applied}");
    fs::remove_dir_all(root).expect("cleanup");
}

/// L3: an overlapping root lock that is released within the wait window is
/// waited for, not failed at once under a `lockTimeout` label.
#[test]
fn overlapping_root_lock_is_waited_for_until_released() {
    let root = std::env::temp_dir().join(format!(
        "octocode-rewrite-lock-wait-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let nested = root.join("nested");
    fs::create_dir_all(&nested).expect("roots");
    let held = super::lock::RootLock::acquire(&root).expect("first lock");
    let waiter = {
        let nested = nested.clone();
        std::thread::spawn(move || super::lock::RootLock::acquire(&nested).map(drop))
    };
    std::thread::sleep(std::time::Duration::from_millis(300));
    drop(held);
    waiter
        .join()
        .expect("waiter thread")
        .expect("the overlapping lock is acquired once released");
    fs::remove_dir_all(root).expect("cleanup");
}

/// L4: an interrupted transaction on a nested root (`/ws/sub`) is recovered
/// by a later apply on the enclosing root (`/ws`): the lock on `/ws` covers it.
#[test]
fn recovery_on_a_root_settles_journals_of_nested_roots() {
    let root = make_temp_dir("octocode-rewrite-nested-journal-").expect("fixture");
    let nested = root.join("sub");
    fs::create_dir_all(&nested).expect("nested");
    let (journal_path, file) = single_file_journal(
        &nested,
        "nested-backed-up",
        JournalPhase::Committing,
        FileState::BackedUp,
    );
    fs::write(&file.backup, b"old\n").expect("backup");
    fs::write(&file.stage, b"new\n").expect("stage");
    // The crash left the target moved to its backup: it is missing now.
    let warnings = recover_transactions(&root, &NeverCancel).expect("recovery settles");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(fs::read(&file.target).expect("restored"), b"old\n");
    assert!(!journal_path.exists(), "nested journal retired");
    fs::remove_dir_all(root).expect("cleanup");
}

/// L4: a pending journal of an enclosing root is not this lock's to touch,
/// but the apply says so instead of staying silent.
#[test]
fn recovery_names_a_pending_journal_of_an_enclosing_root() {
    let root = make_temp_dir("octocode-rewrite-outer-journal-").expect("fixture");
    let nested = root.join("sub");
    fs::create_dir_all(&nested).expect("nested");
    let (journal_path, file) = single_file_journal(
        &root,
        "outer-backed-up",
        JournalPhase::Committing,
        FileState::BackedUp,
    );
    fs::write(&file.backup, b"old\n").expect("backup");
    let warnings = recover_transactions(&nested, &NeverCancel).expect("not blocked");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains(&root.display().to_string()),
        "{warnings:?}"
    );
    assert!(
        journal_path.exists(),
        "the enclosing root keeps its journal"
    );
    assert!(
        !file.target.exists(),
        "outer files are left to the outer root"
    );
    fs::remove_dir_all(root).expect("cleanup");
}
