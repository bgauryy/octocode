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
    assert_eq!(value["errorCode"], "ast.rewrite.overlap");
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
    assert_eq!(result["errorCode"], "ast.rewrite.broken_syntax", "{result}");
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
    assert_eq!(first["totalMatches"], 3, "{first}");
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
    assert_eq!(first["totalMatches"], 2);
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
    // Preview reports boundary-relative `path` values; apply must accept
    // them back verbatim (absolute keys work too).
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
        "ast.rewrite.cancelled"
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
    let path = preview["files"][0]["path"].as_str().expect("path");
    let absolute = root.join(path);
    // A complete preview states each file hash once, in hints.apply.
    assert!(preview["files"][0].get("beforeHash").is_none(), "{preview}");
    apply["expectedHashes"] = json!({
        absolute.to_string_lossy():
            preview["next"]["apply"]["query"]["queries"][0]["expectedHashes"][path].clone()
    });
    apply["postconditions"] = json!([{"kind":"remainingMatches","equals":0}]);
    let failed = rewrite_row(apply, &policy, &security, &NeverCancel, &apply_options());
    assert_eq!(failed["errorCode"], "ast.rewrite.postcondition_failed");
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
    assert_eq!(first_files[0]["path"], "a.ts");
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
    assert_eq!(second_files[0]["path"], "b.ts");
    // The final page's guarded apply still covers every affected file.
    let apply = second["next"]["apply"]["query"]["queries"][0].clone();
    let hashes = apply["expectedHashes"].as_object().expect("hashes");
    assert_eq!(hashes.len(), 2, "{apply}");

    let options = apply_options();
    let mut wrong = apply.clone();
    wrong["expectedHashes"]["a.ts"] = json!("0".repeat(64));
    let mismatch = rewrite_row(wrong, &policy, &security, &NeverCancel, &options);
    assert_eq!(
        mismatch["errorCode"], "ast.rewrite.hash_mismatch",
        "{mismatch}"
    );
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
    assert_eq!(first_file["path"], "a.ts", "{first}");
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
    assert_eq!(second_file["path"], "a.ts", "{second}");
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
    for kept in ["afterHash", "patchBytes", "absolutePath"] {
        assert!(file.get(kept).is_some(), "{kept}: {preview}");
    }
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
    assert_eq!(
        unknown["errorCode"], "ast.rewrite.selection_invalid",
        "{unknown}"
    );
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
    assert_eq!(
        mixed["errorCode"], "ast.rewrite.language_required",
        "{mixed}"
    );
    assert!(mixed.to_string().contains("typescript"), "{mixed}");
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
    assert_eq!(error.code, "ast.rewrite.recovery_failed");
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
    assert_eq!(result["totalMatches"], 2);
    assert_eq!(result["isolation"]["workingDirectory"], "ephemeral");
    assert_eq!(result["executable"]["path"], "native");
    assert_eq!(result["executable"]["version"], "embedded");
    assert_eq!(result["executable"]["capabilityDigest"], "native");
    fs::remove_dir_all(root).expect("cleanup");
}
