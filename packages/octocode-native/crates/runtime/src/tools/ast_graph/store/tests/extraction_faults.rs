//! A file whose extraction or secret scan panics or fails is named with its
//! phase, counted as skipped (the build is partial), and left out of the
//! snapshot; the rest of the build publishes.
use super::*;
use octocode_engine::graph::GraphFactsDocument;
use octocode_engine::signatures::languages::{FactProvider, RegistryView, all_entries};
use octocode_engine::text::analysis::{AnalysisOperation, SourceError};

/// A facts provider that panics on one marker and fails on another.
fn faulty_extract(
    content: &str,
    _file: &str,
    _operation: AnalysisOperation,
) -> Result<Option<GraphFactsDocument>, SourceError> {
    if content.contains("panic-here") {
        panic!("injected extractor panic");
    }
    Err(SourceError::InvalidMap("injected extractor failure"))
}

fn faulty_registry() -> RegistryView<'static> {
    static PROVIDERS: [FactProvider; 1] = [FactProvider {
        id: "fixture-faulty",
        version: "1",
        name: "Faulty fixture",
        extensions: &["faulty"],
        capabilities: octocode_engine::types::GrammarFactCapabilities {
            declarations: true,
            calls: false,
            imports: false,
            exports: false,
            heritage: false,
        },
        extract: faulty_extract,
    }];
    RegistryView::new(all_entries(), &PROVIDERS).expect("distinct extension")
}

fn build(dir: &Path, registry: RegistryView<'_>) -> GraphOutput {
    super::super::ingest::ingest_with_registry(
        &IngestOptions {
            path: dir.join("app"),
            workspace: Some(dir.to_path_buf()),
            ..Default::default()
        },
        &workspace_policy(dir),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        registry,
        &crate::tools::ast_graph::limits::LIMITS,
    )
}

/// The `diagnostics` row about `file`, as `code: message` text.
fn diagnostic_of(dir: &Path, file: &str) -> String {
    let out = ask(dir, "diagnostics", None, |options| {
        options.limit = Some(100);
    });
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    let rows = out.value["rows"].as_array().cloned().unwrap_or_default();
    let row = rows
        .iter()
        .find(|row| row["file"] == file)
        .unwrap_or_else(|| panic!("no diagnostic names {file}: {}", out.value));
    format!("{} {}", row["code"], row["message"])
}

fn found(dir: &Path, name: &str) -> u64 {
    let out = ask(dir, "find", Some(name), |_| {});
    assert!(
        matches!(out.class, ExitClass::Success | ExitClass::Empty),
        "{}",
        out.value
    );
    out.value["pagination"]["totalItems"].as_u64().unwrap_or(0)
}

#[test]
fn panicking_and_failing_extraction_is_named_by_file_and_phase() {
    let dir = git_app(&[
        ("good.ts", "export function healthy() { return 1; }\n"),
        ("bad.faulty", "panic-here\n"),
        ("err.faulty", "fail-here\n"),
    ]);
    let built = build(&dir.0, faulty_registry());
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    assert_eq!(built.value["scan"]["filesSkipped"], 2, "{}", built.value);
    let panicked = diagnostic_of(&dir.0, "bad.faulty");
    assert!(
        panicked.contains("extract") && panicked.contains("panicked"),
        "{panicked}"
    );
    assert!(
        !panicked.contains("injected extractor panic"),
        "a panic message can quote input: {panicked}"
    );
    let failed = diagnostic_of(&dir.0, "err.faulty");
    assert!(
        failed.contains("extract") && failed.contains("injected extractor failure"),
        "{failed}"
    );
    assert_eq!(found(&dir.0, "healthy"), 1);
    let summary = ask(&dir.0, "stats", None, |_| {});
    assert_eq!(
        summary.value["graph"]["isPartial"], true,
        "a skipped file leaves answers partial: {}",
        summary.value
    );
    assert_eq!(summary.value["graph"]["partialReasons"][0], "filesSkipped");
}

#[test]
fn a_failed_secret_scan_keeps_the_file_out_of_the_snapshot() {
    let marker = "secret-scan-fault-5f1c0a";
    crate::tools::ast_graph::graph::inject_secret_scan_fault(marker);
    let dir = git_app(&[
        ("good.ts", "export function healthy() { return 1; }\n"),
        (
            "leak.ts",
            &format!("// {marker}\nexport function unscanned() {{ return 2; }}\n"),
        ),
    ]);
    let built = build(&dir.0, RegistryView::installed());
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    assert_eq!(built.value["scan"]["filesSkipped"], 1, "{}", built.value);
    let failed = diagnostic_of(&dir.0, "leak.ts");
    assert!(failed.contains("secret scan"), "{failed}");
    assert_eq!(found(&dir.0, "unscanned"), 0);
    assert_eq!(found(&dir.0, "healthy"), 1);
    assert_partial(&dir.0);
}

fn assert_partial(dir: &Path) {
    let summary = ask(dir, "stats", None, |_| {});
    assert_eq!(
        summary.value["graph"]["isPartial"], true,
        "{}",
        summary.value
    );
    assert_eq!(summary.value["graph"]["partialReasons"][0], "filesSkipped");
}

/// A panic the sanitizer's own guard recovered from replaces that string
/// only: the rest of its batch is stored as it was.
#[test]
fn a_recovered_sanitizer_panic_replaces_only_its_own_string() {
    let marker = "redactrecovered7c2e";
    super::super::ingest::inject_redact_fault(marker, super::super::ingest::Fault::Recovered);
    let odd = format!("export function {marker}() {{ return 2; }}\n");
    let dir = git_app(&[
        ("good.ts", "export function healthy() { return 1; }\n"),
        ("odd.ts", &odd),
    ]);
    let built = build(&dir.0, RegistryView::installed());
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    assert_eq!(found(&dir.0, marker), 0, "the failed string is replaced");
    assert_eq!(
        found(&dir.0, "healthy"),
        1,
        "the batch's other strings stand"
    );
}

/// A redaction scan that unwinds fails the build closed: its strings may be
/// unscanned, so no snapshot is published. The error names the phase and,
/// from a rescan of the batch string by string, the file whose string
/// unwound, with a graphBuild retry that excludes it.
#[test]
fn an_unwinding_redaction_scan_publishes_nothing() {
    let marker = "redactunwind4b9d";
    super::super::ingest::inject_redact_fault(marker, super::super::ingest::Fault::Unwind);
    let odd = format!("export function {marker}() {{ return 2; }}\n");
    let dir = git_app(&[
        ("good.ts", "export function healthy() { return 1; }\n"),
        ("odd.ts", &odd),
    ]);
    let built = build(&dir.0, RegistryView::installed());
    assert_ne!(built.class, ExitClass::Success, "{}", built.value);
    let text = built.value.to_string();
    assert!(text.contains("secret scan phase panicked"), "{text}");
    assert!(!text.contains("injected"), "{text}");
    assert!(
        built.value["error"]
            .as_str()
            .is_some_and(|error| error.contains("odd.ts") && !error.contains("good.ts")),
        "names the file: {text}"
    );
    let retry = built.value["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["tool"] == "graphBuild")
        .unwrap_or_else(|| panic!("no exclude retry: {text}"));
    let query = &retry["input"]["queries"][0];
    assert_eq!(query["exclude"], json!(["/odd.ts"]), "{retry}");
    crate::contracts::validate_built_query("graphBuild", query.clone())
        .unwrap_or_else(|error| panic!("the retry is not runnable: {error:?}"));
    let stats = ask(&dir.0, "stats", None, |_| {});
    assert_ne!(stats.class, ExitClass::Success, "{}", stats.value);
}

/// On Unix `\` is a file-name byte: `f\g.ts` and `f/g.ts` are two files,
/// two graph nodes.
#[cfg(unix)]
#[test]
fn a_backslash_file_name_is_its_own_node() {
    let dir = git_app(&[
        ("f/g.ts", "export function slash() { return 1; }\n"),
        ("f\\g.ts", "export function back() { return 2; }\n"),
    ]);
    let built = build(&dir.0, RegistryView::installed());
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let mut files = ids(&ask(&dir.0, "find", None, |options| {
        options.kind = Some("file".to_owned());
    }));
    files.sort();
    assert_eq!(files, ["f/g.ts", "f\\g.ts"], "two nodes");
}

/// Runs the graphBuild retry a failed build leads to (its `related` row) on
/// `dir`'s `app` scope, with that row's `exclude`.
fn run_retry(dir: &Path, failed: &GraphOutput) -> GraphOutput {
    let retry = failed.value["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["tool"] == "graphBuild")
        .unwrap_or_else(|| panic!("no exclude retry: {}", failed.value));
    let query = retry["input"]["queries"][0].clone();
    crate::contracts::validate_built_query("graphBuild", query.clone())
        .unwrap_or_else(|error| panic!("the retry is not runnable: {error:?}"));
    let exclude = query["exclude"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|glob| glob.as_str().map(str::to_owned))
        .collect();
    super::super::ingest::ingest_with_registry(
        &IngestOptions {
            path: dir.join("app"),
            workspace: Some(dir.to_path_buf()),
            exclude,
            ..Default::default()
        },
        &workspace_policy(dir),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        RegistryView::installed(),
        &crate::tools::ast_graph::limits::LIMITS,
    )
}

/// A file whose name holds glob characters is excluded by its retry as that
/// one file: the retry builds every other file.
#[test]
fn a_redaction_retry_excludes_a_glob_named_file_alone() {
    let marker = "redactglob7c21";
    super::super::ingest::inject_redact_fault(marker, super::super::ingest::Fault::Unwind);
    let odd = format!("export function {marker}() {{ return 2; }}\n");
    let dir = git_app(&[
        ("good.ts", "export function healthy() { return 1; }\n"),
        ("a1x.ts", "export function near() { return 3; }\n"),
        ("a[1]{x}!.ts", &odd),
    ]);
    let failed = build(&dir.0, RegistryView::installed());
    assert_ne!(failed.class, ExitClass::Success, "{}", failed.value);
    let retried = run_retry(&dir.0, &failed);
    assert_eq!(retried.class, ExitClass::Success, "{}", retried.value);
    assert_eq!(
        retried.value["counts"]["nodesByKind"]["file"], 2,
        "good.ts and a1x.ts, not the excluded file: {}",
        retried.value
    );
}

/// A link panic leads to the same retry: the torn file excluded.
#[test]
fn a_link_panic_leads_to_a_retry_without_the_file() {
    let marker = "linkretry5d0e";
    crate::tools::ast_graph::graph::inject_link_fault(marker);
    let torn = format!("{marker}.ts");
    let dir = git_app(&[
        ("good.ts", "export function healthy() { return 1; }\n"),
        (&torn, "export function torn() { return 3; }\n"),
    ]);
    let failed = build(&dir.0, RegistryView::installed());
    assert_ne!(failed.class, ExitClass::Success, "{}", failed.value);
    let retried = run_retry(&dir.0, &failed);
    assert_eq!(retried.class, ExitClass::Success, "{}", retried.value);
    assert_eq!(
        retried.value["counts"]["nodesByKind"]["file"], 1,
        "good.ts only: {}",
        retried.value
    );
}

/// A panic while linking leaves the graph torn: the build fails, naming the
/// file and phase but not the panic's message, and publishes nothing.
#[test]
fn a_link_panic_fails_the_build_naming_the_file() {
    let marker = "linkfault9e1a";
    crate::tools::ast_graph::graph::inject_link_fault(marker);
    let torn = format!("{marker}.ts");
    let dir = git_app(&[
        ("good.ts", "export function healthy() { return 1; }\n"),
        (&torn, "export function torn() { return 3; }\n"),
    ]);
    let built = build(&dir.0, RegistryView::installed());
    assert_ne!(built.class, ExitClass::Success, "{}", built.value);
    let text = built.value.to_string();
    assert!(
        text.contains(&torn) && text.contains("link phase panicked"),
        "{text}"
    );
    assert!(!text.contains("injected link panic"), "{text}");
    let stats = ask(&dir.0, "stats", None, |_| {});
    assert_ne!(
        stats.class,
        ExitClass::Success,
        "no snapshot was published: {}",
        stats.value
    );
}

/// The torn-file key carries the ingest's exclude retry only: it never
/// reaches the build's error response.
#[test]
fn a_link_failure_response_never_carries_the_torn_file_key() {
    let marker = "linkkey3c7b";
    crate::tools::ast_graph::graph::inject_link_fault(marker);
    let torn = format!("{marker}.ts");
    let dir = git_app(&[
        ("good.ts", "export function healthy() { return 1; }\n"),
        (&torn, "export function torn() { return 3; }\n"),
    ]);
    let built = build(&dir.0, RegistryView::installed());
    assert_ne!(built.class, ExitClass::Success, "{}", built.value);
    let text = built.value.to_string();
    assert!(text.contains(&torn), "the failure names the file: {text}");
    assert!(
        !text.contains(crate::tools::ast_graph::graph::TORN_FILE),
        "{text}"
    );
}

/// A file whose source correspondence no build could hold is named, the
/// other files are kept, and the answers are partial, like after any other
/// skipped file.
#[test]
fn a_budget_declined_file_leaves_answers_partial() {
    let dense = crate::tools::ast_graph::graph::peaks::dense_source("f", 2_000);
    // It enters the build's memory; its correspondence does not fit, and
    // never would beside the small file either.
    let build_bytes =
        crate::tools::ast_graph::graph::peaks::file_peak(&dense, "dense.ts").in_flight() * 3 / 4;
    let dir = git_app(&[
        ("small.ts", "export const small = () => 1;\n"),
        ("dense.ts", &dense),
    ]);
    let built = super::super::ingest::ingest_with_registry(
        &IngestOptions {
            path: dir.0.join("app"),
            workspace: Some(dir.0.to_path_buf()),
            ..Default::default()
        },
        &workspace_policy(&dir.0),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        RegistryView::installed(),
        &crate::tools::ast_graph::limits::Limits {
            build_bytes,
            ..*crate::tools::ast_graph::limits::LIMITS
        },
    );
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    assert_eq!(built.value["scan"]["filesSkipped"], 1, "{}", built.value);
    let declined = diagnostic_of(&dir.0, "dense.ts");
    assert!(
        declined.contains("graph.scan.sourceBudgetExceeded")
            && declined.contains("build memory budget")
            && declined.contains(&build_bytes.to_string()),
        "{declined}"
    );
    assert!(
        found(&dir.0, "small") >= 1,
        "the other file is in the graph"
    );
    assert_partial(&dir.0);
}

/// The faulty build prints nothing: rerun in a child process with the test
/// harness's capture off, so a panic hook's output would reach stderr.
#[test]
fn faulty_extraction_prints_nothing_to_stderr() {
    let exe = std::env::current_exe().expect("test binary");
    let out = std::process::Command::new(exe)
        .args([
            "--exact",
            "tools::ast_graph::store::tests::extraction_faults::panicking_and_failing_extraction_is_named_by_file_and_phase",
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .expect("run the child");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("1 passed"),
        "the child ran the test"
    );
    assert!(
        !stderr.contains("panicked") && !stderr.contains("injected"),
        "{stderr}"
    );
}
