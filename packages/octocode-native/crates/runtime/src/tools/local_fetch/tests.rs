use super::*;
use crate::security::scan::ContentScan;
use crate::tools::cancel::{CancellationCheck, NeverCancel};
use sha2::Digest;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
static TEMP_ID: AtomicUsize = AtomicUsize::new(0);
pub(super) struct Temp(pub PathBuf);
impl Temp {
    pub(super) fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "local-fetch-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test fixture operation should succeed")
                .as_nanos(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).expect("test fixture operation should succeed");
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub(super) struct Paths(pub PathBuf);
impl PathAccess for Paths {
    fn validate_read(&self, p: &Path) -> Result<ValidatedRead, PathFailure> {
        let p = if p.is_absolute() {
            p.into()
        } else {
            self.0.join(p)
        };
        p.canonicalize()
            .map(|canonical| ValidatedRead {
                canonical,
                display: p.to_string_lossy().into_owned(),
            })
            .map_err(|error| PathFailure {
                code: "fileAccessFailed".into(),
                message: error.to_string(),
                safe_path: None,
                resource_missing: false,
                sparse_checkout: false,
                directory: false,
                nearest_dir: None,
            })
    }
}
struct Safe;
impl ContentScan for Safe {
    fn sanitize(&self, text: &str, _: &Path) -> Result<(String, Vec<String>), (String, String)> {
        Ok((
            text.replace("SECRET", "[REDACTED]"),
            if text.contains("SECRET") {
                vec!["Secrets detected and redacted: synthetic".into()]
            } else {
                vec![]
            },
        ))
    }
}
/// Read `req` under `paths` with the redacting test scan, no cancellation,
/// default regex and no response window.
fn fetch(req: &LocalFetchQuery, paths: &impl PathAccess) -> LocalFetchResult {
    execute_local_fetch(
        req,
        paths,
        &Safe,
        &NeverCancel,
        &LocalFetchRegex::default(),
        None,
    )
}
pub(super) fn q(path: &Path) -> LocalFetchQuery {
    LocalFetchQuery {
        path: path.to_string_lossy().parse().expect("path"),
        ..LocalFetchQuery::test_default()
    }
}
#[test]
fn exact_line_and_continuation_union() {
    let t = Temp::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "one\ntwo 😀\nthree\n").expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.unit = Some(WindowUnit::Lines);
    req.length = wire_positive(1);
    let mut joined = String::new();
    loop {
        let r = fetch(&req, &paths);
        assert_eq!(r.status, "success");
        joined.push_str(
            r.content
                .as_deref()
                .expect("test fixture operation should succeed"),
        );
        let Some(next) = r.next.and_then(|n| n.r#continue) else {
            break;
        };
        req = next.query
    }
    assert_eq!(joined, "one\ntwo 😀\nthree\n")
}
#[test]
fn regex_line_anchors_match_every_line() {
    let t = Temp::new();
    let p = t.0.join("Cargo.toml");
    fs::write(&p, "[package]\nversion = \"1\"\n[dep]\nversion = \"2\"\n")
        .expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some("^version.*\"$".parse().expect("match string"));
    req.regex = Some(crate::contracts::tool_types::ReadRegex::Rust);
    req.context_lines = Some(0);
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
    assert_eq!(wire["matchedLines"], serde_json::json!([2, 4]));
}
#[test]
fn wire_form_omits_metadata_derivable_from_emitted_fields() {
    let t = Temp::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "zero\none\nneedle\nthree\nfour\n")
        .expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some("needle".parse().expect("match string"));
    req.context_lines = Some(1);
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
    assert_eq!(wire["content"], "one\nneedle\nthree\n");
    assert_eq!(
        wire["sourceLineRanges"],
        serde_json::json!([{"line": 2, "endLine": 4}])
    );
    assert_eq!(wire["matchedLines"], serde_json::json!([3]));
    assert_eq!(wire["totalLines"], 5);
    assert_eq!(wire["sourceBytes"], 27);
    assert_eq!(wire["returnedBytes"], 17);
    for redundant in [
        "contentView",
        "startLine",
        "endLine",
        "matchRanges",
        "selectedMatchCount",
        "resolvedPath",
        "pagination",
    ] {
        assert!(wire.get(redundant).is_none(), "{redundant}: {wire}");
    }
    // Explanation counters are built for every read; the contract classes
    // them verbose, so a response shows them only under `debug: true`.
    let verbose = crate::tools::id::ToolId::LocalFetch.verbose_paths();
    for counter in [
        "modified",
        "sourceChars",
        "sourceBytes",
        "returnedChars",
        "returnedBytes",
        "returnedLines",
    ] {
        let path = format!("results[].data.{counter}");
        assert!(verbose.contains(&path.as_str()), "{counter} is not verbose");
    }

    let mut paged = q(&p);
    paged.length = wire_positive(2);
    let wire = serde_json::to_value(fetch(&paged, &paths)).expect("serializable");
    assert_eq!(
        wire["pagination"],
        serde_json::json!({"unit":"lines","offset":0,"length":2,"hasMore":true,"nextOffset":2})
    );
    assert!(wire["next"]["continue"].is_object());
    paged.offset = Some(4);
    let last = serde_json::to_value(fetch(&paged, &paths)).expect("serializable");
    assert_eq!(
        last["pagination"],
        serde_json::json!({"unit":"lines","offset":4,"length":1,"hasMore":false})
    );

    let wide = t.0.join("wide.txt");
    fs::write(&wide, "a😀\n").expect("test fixture operation should succeed");
    let wire = serde_json::to_value(fetch(&q(&wide), &paths)).expect("serializable");
    assert_eq!(wire["sourceChars"], 4);
    assert_eq!(wire["sourceBytes"], 6);
    assert_eq!(wire["returnedChars"], 4);
}
fn numbered(n: usize) -> String {
    (1..=n).map(|i| format!("l{i}\n")).collect()
}
/// `a.txt` of `lines` numbered lines where each line in `hits` reads
/// `hit<n>`, and a query matching `hit` with one context line.
fn hit_lines(t: &Temp, lines: usize, hits: &[usize]) -> (Paths, LocalFetchQuery) {
    let p = t.0.join("a.txt");
    let text = hits.iter().fold(numbered(lines), |text, hit| {
        text.replace(&format!("l{hit}\n"), &format!("hit{hit}\n"))
    });
    fs::write(&p, text).expect("test fixture operation should succeed");
    let mut req = q(&p);
    req.match_string = Some("hit".parse().expect("match string"));
    req.context_lines = Some(1);
    (Paths(t.0.clone()), req)
}
#[test]
fn match_windows_are_separated_by_omission_marker() {
    let t = Temp::new();
    let (paths, req) = hit_lines(&t, 30, &[3, 25]);
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
    assert_eq!(
        wire["content"],
        "l2\nhit3\nl4\n... [lines 5-23 not requested] ...\nl24\nhit25\nl26\n"
    );
    assert_eq!(
        wire["sourceLineRanges"],
        serde_json::json!([{"line": 2, "endLine": 4},{"line": 24, "endLine": 26}])
    );
    assert_eq!(wire["matchedLines"], serde_json::json!([3, 25]));
}
#[test]
fn adjacent_match_windows_have_no_marker() {
    let t = Temp::new();
    let (paths, req) = hit_lines(&t, 10, &[3, 5]);
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
    assert_eq!(wire["content"], "l2\nhit3\nl4\nhit5\nl6\n");
    assert_eq!(
        wire["sourceLineRanges"],
        serde_json::json!([{"line": 2, "endLine": 6}])
    );
}
#[test]
fn omission_marker_survives_redaction() {
    let t = Temp::new();
    let p = t.0.join("a.txt");
    fs::write(
        &p,
        numbered(10)
            .replace("l2\n", "hit\n")
            .replace("l9\n", "hit SECRET\n"),
    )
    .expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some("hit".parse().expect("match string"));
    req.context_lines = Some(0);
    let r = fetch(&req, &paths);
    assert_eq!(
        r.content.as_deref(),
        Some("hit\n... [lines 3-8 not requested] ...\nhit [REDACTED]\n")
    );
    // Redaction keeps line counts, so the anchors stay; the warning says
    // the text is not verbatim.
    assert_eq!(
        r.source_line_ranges,
        vec![
            LineRange { start: 2, end: 2 },
            LineRange { start: 9, end: 9 }
        ]
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("not verbatim")),
        "{:?}",
        r.warnings
    );
}
#[test]
fn paged_match_windows_map_source_ranges_around_marker() {
    let t = Temp::new();
    let (paths, mut req) = hit_lines(&t, 30, &[3, 25]);
    req.unit = Some(WindowUnit::Lines);
    req.length = wire_positive(4);
    let first = fetch(&req, &paths);
    assert_eq!(
        first.content.as_deref(),
        Some("l2\nhit3\nl4\n... [lines 5-23 not requested] ...\n")
    );
    assert_eq!(
        first.source_line_ranges,
        vec![LineRange { start: 2, end: 4 }]
    );
    let next = first
        .next
        .and_then(|n| n.r#continue)
        .expect("second page")
        .query;
    let second = fetch(&next, &paths);
    assert_eq!(second.content.as_deref(), Some("l24\nhit25\nl26\n"));
    assert_eq!(
        second.source_line_ranges,
        vec![LineRange { start: 24, end: 26 }]
    );
}
#[test]
fn matches_on_minified_lines_default_to_byte_windows() {
    let t = Temp::new();
    let p = t.0.join("bundle.min.js");
    let filler = "x".repeat(3_000);
    let source = format!("{filler}needle{filler}needle{filler}\nshort needle\n");
    fs::write(&p, &source).expect("fixture");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some("needle".parse().expect("match string"));
    let r = fetch(&req, &paths);
    let content = r.content.expect("content");
    assert_eq!(content.matches("needle").count(), 3, "{content:?}");
    assert!(content.len() < 2_000, "{} bytes", content.len());
    for part in content.split('\n').filter(|part| !part.is_empty()) {
        assert!(
            part.starts_with("... [") || source.contains(part),
            "{part:?}"
        );
    }
    // The cut windows are not the whole lines: one read reaches them.
    let next = r.next.clone().expect("next");
    assert_eq!(
        next.whole_lines
            .as_ref()
            .map(|read| read.query.context_lines),
        Some(Some(0)),
        "{next:?}"
    );
    assert!(next.r#continue.is_none(), "{next:?}");
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 1, end: 2 }],
        "anchor kept"
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("contextLines")),
        "{:?}",
        r.warnings
    );
    // An explicit contextLines keeps whole lines.
    req.context_lines = Some(0);
    let r = fetch(&req, &paths);
    assert!(
        r.warnings.iter().all(|w| !w.contains("contextLines")),
        "{:?}",
        r.warnings
    );
}
#[test]
fn missing_files_in_a_sparse_checkout_say_so() {
    let t = Temp::new();
    let git = t.0.join("clone/.git");
    fs::create_dir_all(git.join("info")).expect("fixture");
    fs::write(git.join("info/sparse-checkout"), "/*\n!/*/\n/src/\n").expect("fixture");
    fs::write(git.join("config"), "[core]\n\tsparseCheckout = true\n").expect("fixture");
    let policy = crate::policy::path::PathPolicy::new(crate::policy::path::PathPolicyConfig {
        workspace_root: Some(t.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let mut req = LocalFetchQuery::test_default();
    req.path = "clone/docs/guide.md".parse().expect("path");
    let r = fetch(&req, &policy);
    assert!(r.resource_missing, "{r:?}");
    let error = r.error.as_deref().unwrap_or_default();
    assert!(error.contains("clone/docs/guide.md"), "{error}");
    assert!(error.contains("sparse checkout"), "{error}");

    assert_eq!(r.error_code.as_deref(), Some("pathNotFound"), "{r:?}");

    // An ordinary repository keeps the plain not-found message, under
    // the not-found code the other local tools use; the recovery advice
    // is the row's hint, not a second copy in the message.
    fs::write(git.join("config"), "[core]\n\tbare = false\n").expect("fixture");
    let r = fetch(&req, &policy);
    assert_eq!(
        r.error.as_deref(),
        Some("Path does not exist: clone/docs/guide.md"),
        "{r:?}"
    );
    assert_eq!(r.error_code.as_deref(), Some("pathNotFound"), "{r:?}");
    assert!(r.resource_missing, "{r:?}");
}
#[test]
fn redacted_chunk_and_range_reads_keep_their_source_anchor() {
    let t = Temp::new();
    let p = t.0.join("a.txt");
    fs::write(&p, numbered(30).replace("l12\n", "l12 SECRET\n")).expect("fixture");
    let paths = Paths(t.0.clone());
    let mut chunk = q(&p);
    chunk.unit = Some(WindowUnit::Lines);
    chunk.offset = Some(wire_count(10));
    chunk.length = wire_positive(5);
    let mut range = q(&p);
    range.set_line_span(11, 13);
    for (req, expected) in [(chunk, (11, 15)), (range, (11, 13))] {
        let r = fetch(&req, &paths);
        assert!(
            r.content
                .as_deref()
                .is_some_and(|c| c.contains("[REDACTED]")),
            "{r:?}"
        );
        assert_eq!(
            r.source_line_ranges,
            vec![LineRange {
                start: expected.0,
                end: expected.1
            }]
        );
        assert!(
            r.warnings.iter().any(|w| w.contains("not verbatim")),
            "{:?}",
            r.warnings
        );
    }
}
#[test]
fn context_bytes_overlapping_windows_do_not_duplicate_source() {
    let t = Temp::new();
    let p = t.0.join("a.txt");
    let source = "aaa needle bbb\nline2\nline3\nline4\nccc needle ddd\nneedle x needle\n";
    fs::write(&p, source).expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some("needle".parse().expect("match string"));
    req.context_bytes = Some(4);
    let r = fetch(&req, &paths);
    let content = r.content.expect("content");
    for part in content.split("\n").filter(|part| !part.is_empty()) {
        assert!(
            part.starts_with("... [") || source.contains(part),
            "fabricated text {part:?} in {content:?}"
        );
    }
    assert!(content.contains("bytes omitted"), "{content:?}");
}
#[test]
fn multiline_matches_preserve_every_matched_source_line() {
    let t = Temp::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "before\nfirst\nsecond\nafter\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for (pattern, is_regex) in [("first\nsecond", false), (r"first\nsecond", true)] {
        let mut req = q(&p);
        req.match_string = Some(pattern.parse().expect("match string"));
        req.regex = is_regex.then_some(crate::contracts::tool_types::ReadRegex::Rust);
        req.context_lines = Some(0);
        let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
        assert_eq!(wire["content"], "first\nsecond\n", "{pattern:?}: {wire}");
        assert_eq!(wire["matchedLines"], serde_json::json!([2, 3]));
        assert_eq!(
            wire["sourceLineRanges"],
            serde_json::json!([{"line": 2, "endLine": 3}])
        );
    }
}

/// GF2/X8: an omitted caseMode is smart case, like localSearch: a
/// lowercase pattern ignores case, an uppercase letter makes it exact.
#[test]
fn match_case_defaults_to_smart_case() {
    use crate::contracts::tool_types::ReadCaseMode;
    let t = Temp::new();
    let p = t.0.join("a.py");
    fs::write(&p, "MERGE_SETTINGS = 1\nmerge_settings()\nMerge_Settings\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for (pattern, mode, lines) in [
        ("merge_settings", None, serde_json::json!([1, 2, 3])),
        ("MERGE_SETTINGS", None, serde_json::json!([1])),
        (
            "MERGE_SETTINGS",
            Some(ReadCaseMode::Insensitive),
            serde_json::json!([1, 2, 3]),
        ),
        (
            "merge_settings",
            Some(ReadCaseMode::Sensitive),
            serde_json::json!([2]),
        ),
    ] {
        let mut req = q(&p);
        req.match_string = Some(pattern.parse().expect("match string"));
        req.case_mode = mode;
        req.context_lines = Some(0);
        let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
        assert_eq!(wire["matchedLines"], lines, "{pattern} {mode:?}: {wire}");
    }
}

/// `regex:"pcre2"` (localSearch's engine for lookaround/backreferences)
/// matches in original byte coordinates, and a multiline match keeps every
/// source line it touches.
#[test]
fn pcre2_matches_lookaround_backreferences_and_multiline_spans() {
    use crate::contracts::tool_types::ReadRegex;
    let t = Temp::new();
    let p = t.0.join("a.rs");
    fs::write(&p, "before\nfn alpha() {\n  beta(); beta();\n}\nafter\n").expect("fixture");
    let paths = Paths(t.0.clone());
    let read = |pattern: &str| {
        let mut req = q(&p);
        req.match_string = Some(pattern.parse().expect("match string"));
        req.regex = Some(ReadRegex::Pcre2);
        req.context_lines = Some(0);
        serde_json::to_value(fetch(&req, &paths)).expect("serializable")
    };
    let lookbehind = read("(?<=fn )alpha");
    assert_eq!(
        lookbehind["matchedLines"],
        serde_json::json!([2]),
        "{lookbehind}"
    );
    let backreference = read(r"(beta\(\);) \1");
    assert_eq!(
        backreference["matchedLines"],
        serde_json::json!([3]),
        "{backreference}"
    );
    for (engine, pattern) in [
        (ReadRegex::Pcre2, r"alpha\(\) \{\n\s+beta"),
        (ReadRegex::Rust, r"alpha\(\) \{\n\s+beta"),
    ] {
        let mut req = q(&p);
        req.match_string = Some(pattern.parse().expect("match string"));
        req.regex = Some(engine);
        req.context_lines = Some(0);
        let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
        assert_eq!(
            wire["content"], "fn alpha() {\n  beta(); beta();\n",
            "{engine}: {wire}"
        );
        assert_eq!(
            wire["matchedLines"],
            serde_json::json!([2, 3]),
            "{engine}: {wire}"
        );
        assert_eq!(
            wire["sourceLineRanges"],
            serde_json::json!([{"line": 2, "endLine": 3}]),
            "{engine}: {wire}"
        );
    }
    // Uppercase in a regex is smart case too.
    assert_eq!(read("ALPHA")["content"], "", "no line has uppercase ALPHA");
    assert_eq!(read("alph[a]")["matchedLines"], serde_json::json!([2]));
    let invalid = read("(");
    assert_eq!(invalid["errorCode"], "invalidPattern", "{invalid}");
}

#[test]
fn context_bytes_keep_original_offsets_when_unicode_lowercase_expands() {
    let t = Temp::new();
    let p = t.0.join("a.txt");
    let paths = Paths(t.0.clone());
    for (source, pattern, expected) in [
        ("İneedle tail\n", "needle", "needle"),
        ("İNEEDLE\n", "needle", "NEEDLE"),
        ("prefix İ tail\n", "İ", "İ"),
        ("İİneedle", "needle", "needle"),
    ] {
        fs::write(&p, source).expect("fixture");
        let mut req = q(&p);
        req.match_string = Some(pattern.parse().expect("match string"));
        req.context_bytes = Some(0);
        let result = fetch(&req, &paths);
        let content = result.content.unwrap_or_default();
        // A window that starts or ends mid-line says so with an omission
        // marker; the matched bytes between them are the original span.
        let span = content
            .lines()
            .filter(|line| !line.starts_with("... ["))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(span, expected, "{source:?}: {content:?}");
        let cut_before = source.find(expected).is_some_and(|at| at > 0);
        assert_eq!(
            content.starts_with("... ["),
            cut_before,
            "{source:?}: {content:?}"
        );
    }
}

#[test]
fn byte_pages_preserve_utf8_and_use_byte_offsets() {
    let t = Temp::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "a😀b").expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.unit = Some(WindowUnit::Bytes);
    req.length = wire_positive(2);
    let a = fetch(&req, &paths);
    assert_eq!(a.content.as_deref(), Some("a😀"));
    req = a
        .next
        .expect("test fixture operation should succeed")
        .r#continue
        .expect("test fixture operation should succeed")
        .query;
    let b = fetch(&req, &paths);
    assert_eq!(b.content.as_deref(), Some("b"));
    assert_eq!(b.returned_chars, Some(1))
}
#[test]
fn ranges_matches_redaction_and_binary() {
    let t = Temp::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "zero\nneedle SECRET\nlast\n").expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some("needle".parse().expect("match string"));
    req.context_lines = Some(0);
    let r = fetch(&req, &paths);
    let expected_hash = hex::encode(sha2::Sha256::digest(b"zero\nneedle SECRET\nlast\n"));
    assert_eq!(r.source_sha256.as_deref(), Some(expected_hash.as_str()));
    assert_eq!(r.content.as_deref(), Some("needle [REDACTED]\n"));
    assert_eq!(r.match_ranges, vec![LineRange { start: 2, end: 2 }]);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 2, end: 2 }],
        "line-preserving redaction keeps the source anchor"
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("not verbatim")),
        "{:?}",
        r.warnings
    );
    let bin = t.0.join("b.bin");
    fs::write(&bin, [0, 1, 2]).expect("test fixture operation should succeed");
    assert_eq!(
        fetch(&q(&bin), &paths).error_code.as_deref(),
        Some("binaryFileUnsupported")
    )
}
struct Cancel(AtomicUsize);
impl CancellationCheck for Cancel {
    fn check(&self) -> Result<(), String> {
        if self.0.fetch_add(1, Ordering::SeqCst) > 0 {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    }
}
#[test]
fn cancellation_is_checked_around_io() {
    let t = Temp::new();
    let p = t.0.join("a");
    fs::write(&p, "text").expect("test fixture operation should succeed");
    let r = execute_local_fetch(
        &q(&p),
        &Paths(t.0.clone()),
        &Safe,
        &Cancel(AtomicUsize::new(0)),
        &LocalFetchRegex::default(),
        None,
    );
    assert_eq!(r.error_code.as_deref(), Some("cancelled"))
}
#[test]
fn schema_rejects_unknown_fields_and_invalid_combinations() {
    assert!(
        serde_json::from_value::<LocalFetchQuery>(serde_json::json!({"path":"x","madeUp":true}))
            .is_err()
    );
    // The contract is the one validator of selector combinations.
    let combined = serde_json::json!({"path":"x","fullContent":true,"matchString":"x"});
    assert!(crate::contracts::validate_query("localFetch", combined).is_err());
}

#[test]
fn latin1_text_decodes_with_a_flag_and_long_lines_fall_back_to_bytes() {
    let t = Temp::new();
    // "café\nnaïve" in ISO-8859-1: text, but not valid UTF-8.
    let latin1 = t.0.join("latin1.txt");
    fs::write(&latin1, b"caf\xe9\nna\xefve\n").expect("latin-1 fixture should be written");
    let paths = Paths(t.0.clone());
    let decoded = fetch(&q(&latin1), &paths);
    assert_eq!(decoded.error_code, None, "{decoded:?}");
    assert_eq!(decoded.content.as_deref(), Some("café\nnaïve\n"));
    assert!(
        decoded.warnings.iter().any(|w| w.contains("Latin-1")),
        "{:?}",
        decoded.warnings
    );
    // Mostly UTF-8 with one stray byte stays UTF-8 (lossy), also flagged.
    let stray = t.0.join("stray.txt");
    fs::write(
        &stray,
        "é \u{2014} ok\n"
            .bytes()
            .chain([0xff, b'\n'])
            .collect::<Vec<_>>(),
    )
    .expect("stray fixture should be written");
    let lossy = fetch(&q(&stray), &paths);
    assert_eq!(lossy.content.as_deref(), Some("é \u{2014} ok\n\u{fffd}\n"));
    assert!(
        lossy.warnings.iter().any(|w| w.contains("UTF-8")),
        "{:?}",
        lossy.warnings
    );

    let long = t.0.join("long.txt");
    fs::write(&long, "x".repeat(18_000)).expect("long fixture should be written");
    let long_result = fetch(&q(&long), &paths);
    let pagination = long_result.pagination.expect("long result should paginate");
    assert_eq!(pagination.unit, WindowUnit::Bytes);
    assert_eq!(pagination.length, 16_384);
    assert!(pagination.has_more);
}

#[test]
fn full_content_continuation_pages_by_the_byte_budget_not_100_lines() {
    let t = Temp::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("short-lines.rs");
    fs::write(&file, "let x = 1;\n".repeat(6_000)).expect("fixture should be written");
    let mut full = q(&file);
    full.full_content = Some(true);
    let limited = execute_local_fetch(
        &full,
        &paths,
        &Safe,
        &NeverCancel,
        &LocalFetchRegex::default(),
        Some(50_000),
    );
    assert_eq!(limited.partial_reasons, vec![PartialReason::FullContent]);
    assert!(limited.pagination.as_ref().expect("first page").length > 1_000);
    let next = limited
        .next
        .and_then(|next| next.r#continue)
        .expect("continuation");
    let page = execute_local_fetch(
        &next.query,
        &paths,
        &Safe,
        &NeverCancel,
        &LocalFetchRegex::default(),
        Some(50_000),
    );
    let pagination = page.pagination.expect("paged");
    // 11-byte lines: a 16 KiB page holds ~1,489 lines, not 100.
    assert!(pagination.length > 1_000, "{pagination:?}");
    assert!(page.content.as_deref().unwrap_or_default().len() <= 16_384);
}

/// A whole-file read is cut at the configured response window, not at a
/// fixed size: the same file pages under a small window and returns whole
/// under a large one.
#[test]
fn full_content_is_cut_at_the_configured_response_window() {
    let t = Temp::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("mid.txt");
    fs::write(&file, "let value = 1;\n".repeat(1_500)).expect("fixture");
    let mut full = q(&file);
    full.full_content = Some(true);
    let read = |window| {
        execute_local_fetch(
            &full,
            &paths,
            &Safe,
            &NeverCancel,
            &LocalFetchRegex::default(),
            window,
        )
    };
    let small = read(Some(10_000));
    assert_eq!(small.partial_reasons, vec![PartialReason::FullContent]);
    assert!(small.next.and_then(|next| next.r#continue).is_some());
    let large = read(Some(50_000));
    assert!(
        large.partial_reasons.is_empty(),
        "{:?}",
        large.partial_reasons
    );
    assert!(large.next.is_none());
    assert_eq!(large.returned_lines, Some(1_500));
}

#[test]
fn full_content_limits_return_executable_continuations() {
    let t = Temp::new();
    let paths = Paths(t.0.clone());
    let view_limited = t.0.join("view.txt");
    fs::write(&view_limited, "x".repeat(60_000)).expect("view fixture should be written");
    let mut compact = q(&view_limited);
    compact.full_content = Some(true);
    compact.minify = Some(MinifyMode::Standard);
    let result = execute_local_fetch(
        &compact,
        &paths,
        &Safe,
        &NeverCancel,
        &LocalFetchRegex::default(),
        Some(50_000),
    );
    // Page 1 of the view is returned inline with the continuation.
    assert_eq!(result.error_code, None);
    assert!(
        result
            .content
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(result.partial_reasons, vec![PartialReason::FullContent]);
    assert!(result.next.and_then(|next| next.r#continue).is_some());

    let source_temp = Temp::new();
    let source_limited = source_temp.0.join("source.txt");
    fs::write(&source_limited, "y".repeat(110 * 1024)).expect("source fixture should be written");
    let mut full = q(&source_limited);
    full.full_content = Some(true);
    let result = execute_local_fetch(
        &full,
        &Paths(source_temp.0.clone()),
        &Safe,
        &NeverCancel,
        &LocalFetchRegex::default(),
        Some(50_000),
    );
    assert_eq!(result.error_code, None);
    assert!(
        result
            .content
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(result.partial_reasons, vec![PartialReason::FullContent]);
    assert!(result.next.and_then(|next| next.r#continue).is_some());
}

#[test]
fn security_limit_is_terminal_but_offers_a_bounded_line_read() {
    struct Limited;
    impl ContentScan for Limited {
        fn sanitize(&self, _: &str, _: &Path) -> Result<(String, Vec<String>), (String, String)> {
            Err(("contentSecurityLimit".into(), "limited".into()))
        }
    }
    let t = Temp::new();
    let p = t.0.join("large.txt");
    fs::write(&p, "one\ntwo\n").expect("security fixture should be written");
    let result = execute_local_fetch(
        &q(&p),
        &Paths(t.0.clone()),
        &Limited,
        &NeverCancel,
        &LocalFetchRegex::default(),
        None,
    );
    assert_eq!(result.error_code.as_deref(), Some("contentSecurityLimit"));
    assert_eq!(result.terminal_limit, Some(true));
    assert!(
        result
            .next
            .and_then(|next| next.read_bounded_lines)
            .is_some()
    );
}

/// `minify:"standard"` keeps the citation gutter: every kept line opens
/// with its own source line number and a tab; comments and blank lines
/// the view drops leave gaps in the numbers, never a wrong number.
#[test]
fn standard_view_lines_cite_their_source_lines() {
    let t = Temp::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("a.js");
    fs::write(
            &p,
            "// header comment\n\nimport { a } from 'a';\n\n/* block\n   comment */\nexport function run(x) {\n  return a(x); // trailing\n}\n",
        )
        .expect("fixture");
    let read = |fields: serde_json::Value| fetch(&qj(&p, fields), &paths).content.expect("content");
    let content = read(serde_json::json!({"minify":"standard"}));
    let source = fs::read_to_string(&p).expect("source");
    let squash = |text: &str| {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    let mut numbers = Vec::new();
    for line in content.lines() {
        let (number, text) = line
            .split_once('\t')
            .unwrap_or_else(|| panic!("every line numbered: {content:?}"));
        let number = number.parse::<usize>().expect("number");
        let original = source.lines().nth(number - 1).expect("source line");
        assert!(
            squash(original).contains(&squash(text)),
            "{number}: {text:?} vs {original:?}"
        );
        numbers.push(number);
    }
    assert_eq!(numbers.first(), Some(&3), "{content:?}");
    assert!(numbers.contains(&8), "{content:?}");
    // A line window keeps the window's own numbers.
    let window = read(serde_json::json!({"minify":"standard","ranges": ["7-9"]}));
    assert!(window.starts_with("7\t"), "{window:?}");
    assert!(
        window.lines().last().is_some_and(|l| l.starts_with("9\t")),
        "{window:?}"
    );
}

/// `minify:"symbols"` uses one gutter, `N\t`, in every language and on
/// every line, closing-brace lines included.
#[test]
fn symbols_view_uses_a_tab_gutter_everywhere() {
    let t = Temp::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("b.ts");
    fs::write(
            &p,
            "import { a } from 'a';\nimport { b } from 'b';\n\nexport class Box {\n  open(): void {\n    a();\n  }\n}\n\nexport function make(): Box {\n  return new Box();\n}\n",
        )
        .expect("fixture");
    let content = fetch(&qj(&p, serde_json::json!({"minify":"symbols"})), &paths)
        .content
        .expect("content");
    assert!(!content.is_empty());
    for line in content.lines() {
        let (number, _) = line
            .split_once('\t')
            .unwrap_or_else(|| panic!("tab gutter: {line:?} in {content:?}"));
        // A declaration head reads `start-end`; every other line one number.
        let mut bounds = number.splitn(2, '-');
        assert!(
            bounds.all(|bound| bound.parse::<u64>().is_ok()),
            "{line:?} in {content:?}"
        );
    }
}

/// A query built from wire JSON (fields the test does not name default).
fn qj(path: &Path, fields: serde_json::Value) -> LocalFetchQuery {
    let mut query = serde_json::json!({"path": path.to_string_lossy(), "mainGoal": "test", "reasoning": "test"});
    for (key, value) in fields.as_object().expect("object") {
        query[key] = value.clone();
    }
    serde_json::from_value(query).expect("localFetch query")
}

/// Each page names only the matched windows and lines it shows: a later
/// page never restates what an earlier page sent.
#[test]
fn match_ranges_cover_only_the_page_that_shows_them() {
    let t = Temp::new();
    let p = t.0.join("a.txt");
    let mut text = numbered(300);
    text = text
        .replace("\nl10\n", "\nl10 needle\n")
        .replace("\nl250\n", "\nl250 needle\n");
    fs::write(&p, &text).expect("fixture");
    let paths = Paths(t.0.clone());
    let req = qj(
        &p,
        serde_json::json!({"matchString": "needle", "contextLines": 2, "unit": "lines", "length": 5}),
    );
    let first = fetch(&req, &paths);
    assert_eq!(first.error, None, "{first:?}");
    let wire = serde_json::to_value(&first).expect("serialize");
    // The page's windows are its source lines: nothing to restate.
    assert!(wire.get("matchRanges").is_none(), "{wire}");
    assert_eq!(wire["matchedLines"], serde_json::json!([10]), "{wire}");
    let next = first
        .next
        .and_then(|next| next.r#continue)
        .expect("second page")
        .query;
    let second = serde_json::to_value(fetch(&next, &paths)).expect("serialize");
    assert!(
        !second.to_string().contains("\"start\":8"),
        "page 2 never restates page 1's window: {second}"
    );
    assert!(second.get("matchRanges").is_none(), "{second}");
    assert_eq!(second["matchedLines"], serde_json::json!([250]), "{second}");
}

#[test]
fn ranges_read_several_windows_in_one_row() {
    let t = Temp::new();
    let p = t.0.join("a.txt");
    fs::write(&p, numbered(30)).expect("fixture");
    let paths = Paths(t.0.clone());
    // Unordered, overlapping, and past-the-end ranges merge and clamp.
    let req = qj(
        &p,
        serde_json::json!({"ranges": ["20-21", "2-3", "3-4", "29-40"]}),
    );
    let r = fetch(&req, &paths);
    assert_eq!(r.error, None, "{r:?}");
    assert_eq!(
        r.content.as_deref(),
        Some(
            "l2\nl3\nl4\n... [lines 5-19 not requested] ...\nl20\nl21\n... [lines 22-28 not requested] ...\nl29\nl30\n"
        )
    );
    assert_eq!(
        r.source_line_ranges,
        vec![
            LineRange { start: 2, end: 4 },
            LineRange { start: 20, end: 21 },
            LineRange { start: 29, end: 30 }
        ]
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("29-40")),
        "{:?}",
        r.warnings
    );
    // One range reads that span.
    let old = qj(&p, serde_json::json!({"ranges": ["2-3"]}));
    assert_eq!(fetch(&old, &paths).content.as_deref(), Some("l2\nl3\n"));
    // Ranges wholly past the end are an error, not an empty success.
    let past = qj(&p, serde_json::json!({"ranges": ["40-41"]}));
    assert!(fetch(&past, &paths).error.is_some());
}

#[test]
fn block_reads_through_the_enclosing_declaration() {
    let t = Temp::new();
    let p = t.0.join("m.py");
    fs::write(
            &p,
            "import os\n\ndef first(a):\n    x = 1\n    return a\n\n\ndef second(b):\n    if b:\n        return 1\n    return 2\n",
        )
        .expect("fixture");
    let paths = Paths(t.0.clone());
    let ranged = qj(&p, serde_json::json!({"ranges": ["8-9"], "block": true}));
    let r = fetch(&ranged, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 8, end: 11 }],
        "{r:?}"
    );
    let matched = qj(
        &p,
        serde_json::json!({"matchString": ["return a", "return 1"], "contextLines": 0, "block": true}),
    );
    let r = fetch(&matched, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![
            LineRange { start: 3, end: 5 },
            LineRange { start: 8, end: 11 }
        ],
        "{r:?}"
    );
    assert_eq!(r.matched_lines, vec![5, 10]);
}

/// A match window that stops inside its declaration shows a short rest
/// inline (cheaper than a lead) and offers a long one as `readBlock`: the
/// whole declaration, since the window holds its head.
#[test]
fn a_cut_declaration_inlines_a_short_rest_and_leads_to_a_long_one() {
    let t = Temp::new();
    let p = t.0.join("m.py");
    let small: String = (2..=10).map(|i| format!("    s{i} = {i}\n")).collect();
    let big: String = (13..=52).map(|i| format!("    b{i} = {i}\n")).collect();
    // small(): lines 1-10, needle at 3; big(): lines 12-52, needle at 14.
    fs::write(
        &p,
        format!(
            "def small():\n{}\ndef big():\n{}",
            small.replace("s3 = 3", "needle_small = 3"),
            big.replace("b14 = 14", "needle_big = 14")
        ),
    )
    .expect("fixture");
    let paths = Paths(t.0.clone());
    let short = qj(
        &p,
        serde_json::json!({"matchString": "needle_small", "contextLines": 2}),
    );
    let r = fetch(&short, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 1, end: 10 }],
        "{r:?}"
    );
    assert_eq!(r.matched_lines, vec![3]);
    assert!(
        r.next
            .as_ref()
            .and_then(|next| next.read_block.as_ref())
            .is_none(),
        "{r:?}"
    );
    let long = qj(
        &p,
        serde_json::json!({"matchString": "needle_big", "contextLines": 2}),
    );
    let r = fetch(&long, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 12, end: 16 }],
        "{r:?}"
    );
    // An optional lead leaves nothing of this result unread.
    assert_eq!(r.is_partial, None, "{r:?}");
    let rest = r
        .next
        .and_then(|next| next.read_block)
        .expect("next.readBlock");
    assert_eq!(
        rest.query
            .ranges
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["12-52".to_owned()],
        "{rest:?}"
    );
    let r = fetch(&rest.query, &paths);
    assert_eq!(r.source_line_ranges, vec![LineRange { start: 12, end: 52 }]);
    assert!(
        r.next.is_none(),
        "the whole declaration offers no more: {r:?}"
    );
    // Exact-line reads ask for no more.
    let exact = qj(
        &p,
        serde_json::json!({"matchString": "needle_small", "contextLines": 0}),
    );
    let r = fetch(&exact, &paths);
    assert_eq!(r.source_line_ranges, vec![LineRange { start: 3, end: 3 }]);
}

/// Hits in several cut declarations lead to one read of every cut
/// declaration (each whole, as its window holds its head), within the
/// lead's byte cap: no hit's declaration is left unreachable.
#[test]
fn read_block_reads_every_cut_declaration() {
    let t = Temp::new();
    let p = t.0.join("m.py");
    let body = |name: &str, from: usize| -> String {
        (from..from + 40)
            .map(|i| format!("    {name}{i} = {i}\n"))
            .collect()
    };
    // one(): lines 1-41, hit at 3; two(): lines 43-83, hit at 45.
    fs::write(
        &p,
        format!(
            "def one():\n{}\ndef two():\n{}",
            body("a", 2).replace("a3 = 3", "hit_one = 3"),
            body("b", 44).replace("b45 = 45", "hit_two = 45")
        ),
    )
    .expect("fixture");
    let paths = Paths(t.0.clone());
    let req = qj(
        &p,
        serde_json::json!({"matchString": ["hit_one", "hit_two"], "contextLines": 2}),
    );
    let r = fetch(&req, &paths);
    assert_eq!(r.matched_lines, vec![3, 45], "{r:?}");
    let lead = r
        .next
        .and_then(|next| next.read_block)
        .expect("next.readBlock");
    assert_eq!(
        lead.query
            .ranges
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["1-41".to_owned(), "43-83".to_owned()],
        "{lead:?}"
    );
}

/// A `ranges` read that stops inside a declaration offers it whole when
/// the read holds one end of it (the rest alone would cut it again from the
/// other side), and only its unseen lines when the read sits inside it.
#[test]
fn a_cut_ranges_read_offers_the_rest_of_its_declaration() {
    let t = Temp::new();
    let p = t.0.join("m.py");
    let body: String = (2..=40).map(|i| format!("    x{i} = {i}\n")).collect();
    fs::write(&p, format!("def big():\n{body}    return 0\n")).expect("fixture");
    let paths = Paths(t.0.clone());
    let cut = qj(&p, serde_json::json!({"ranges": ["1-10"]}));
    let r = fetch(&cut, &paths);
    let lead = r
        .next
        .and_then(|next| next.read_block)
        .expect("next.readBlock");
    assert_eq!(
        lead.query
            .ranges
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["1-41".to_owned()],
        "{lead:?}"
    );
    let inside = qj(&p, serde_json::json!({"ranges": ["10-20"]}));
    let lead = fetch(&inside, &paths)
        .next
        .and_then(|next| next.read_block)
        .expect("next.readBlock");
    assert_eq!(
        lead.query
            .ranges
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["1-9".to_owned(), "21-41".to_owned()],
        "{lead:?}"
    );
    // Lines between a multi-range read's own windows are skipped on purpose.
    let r = fetch(&lead.query, &paths);
    assert!(r.next.is_none(), "{r:?}");
    // A read that covers its declaration offers nothing more.
    let whole = qj(&p, serde_json::json!({"ranges": ["1-41"]}));
    let r = fetch(&whole, &paths);
    assert!(r.next.is_none(), "{r:?}");
}

/// Following `readBlock` leads always ends, and every hop reads lines the
/// walk has not seen: whatever window a read takes into a declaration (its
/// head, tail, middle, or across its end), the lead's own read covers that
/// declaration, so it offers no further lead.
#[test]
fn read_block_leads_walk_to_termination() {
    let t = Temp::new();
    let p = t.0.join("m.py");
    let body = |name: &str, lines: std::ops::RangeInclusive<usize>| -> String {
        lines.map(|i| format!("    {name}{i} = {i}\n")).collect()
    };
    // outer(): 1-40, with inner(): 10-20 nested; after(): 42-60.
    fs::write(
        &p,
        format!(
            "def outer():\n{}    def inner():\n{}{}\ndef after():\n{}",
            body("a", 2..=9),
            body("    b", 11..=20),
            body("c", 21..=40),
            body("d", 43..=60)
        ),
    )
    .expect("fixture");
    let paths = Paths(t.0.clone());
    let lines = |r: &LocalFetchResult| -> Vec<usize> {
        r.source_line_ranges
            .iter()
            .flat_map(|range| range.start..=range.end)
            .collect()
    };
    let starts = [
        serde_json::json!({"ranges": ["1-6"]}),
        serde_json::json!({"ranges": ["35-40"]}),
        serde_json::json!({"ranges": ["25-30"]}),
        serde_json::json!({"ranges": ["12-14"]}),
        serde_json::json!({"ranges": ["38-45"]}),
        serde_json::json!({"ranges": ["3-5", "30-33"]}),
        serde_json::json!({"matchString": "a3 = 3", "contextLines": 1}),
        serde_json::json!({"matchString": "c38 = 38", "contextLines": 1}),
        serde_json::json!({"matchString": "c25 = 25", "contextLines": 1}),
        serde_json::json!({"matchString": "b15 = 15", "contextLines": 1}),
    ];
    for start in starts {
        let mut query = qj(&p, start.clone());
        let mut seen = std::collections::BTreeSet::new();
        for hop in 0.. {
            assert!(hop < 2, "{start}: a readBlock walk must end in one hop");
            let r = fetch(&query, &paths);
            let read = lines(&r);
            assert!(
                hop == 0 || read.iter().any(|line| !seen.contains(line)),
                "{start}: hop {hop} read nothing new: {read:?}"
            );
            seen.extend(read);
            match r.next.and_then(|next| next.read_block) {
                Some(lead) => query = lead.query,
                None => break,
            }
        }
    }
}

#[test]
fn a_match_list_is_a_grep_map_of_every_literal() {
    let t = Temp::new();
    let p = t.0.join("a.txt");
    fs::write(
        &p,
        numbered(12)
            .replace("l4\n", "def a|b\n")
            .replace("l9\n", "x.y\n"),
    )
    .expect("fixture");
    let paths = Paths(t.0.clone());
    // Literal entries are literal: `|` and `.` are not regex syntax.
    let req = qj(
        &p,
        serde_json::json!({"matchString": ["a|b", "x.y", "absent"], "contextLines": 0}),
    );
    let r = fetch(&req, &paths);
    assert_eq!(
        r.content.as_deref(),
        Some("def a|b\n... [lines 5-8 not requested] ...\nx.y\n")
    );
    assert_eq!(r.matched_lines, vec![4, 9]);
    let one = qj(
        &p,
        serde_json::json!({"matchString": "x.y", "contextLines": 0}),
    );
    assert_eq!(fetch(&one, &paths).matched_lines, vec![9]);
}

#[test]
fn an_oversized_block_continues_through_the_declaration_end() {
    let t = Temp::new();
    let p = t.0.join("m.py");
    let body: String = (0..450).map(|i| format!("    x{i} = {i}\n")).collect();
    // def at line 1, 450 body lines (2-451), return at 452.
    fs::write(&p, format!("def big():\n{body}    return 0\n")).expect("fixture");
    let paths = Paths(t.0.clone());
    let ranged = qj(&p, serde_json::json!({"ranges": ["10-12"], "block": true}));
    let r = fetch(&ranged, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange {
            start: 10,
            end: 409
        }]
    );
    let rest = r
        .next
        .and_then(|next| next.continue_block)
        .expect("next.continueBlock");
    assert_eq!(
        rest.query
            .ranges
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["410-452".to_owned()],
        "{rest:?}"
    );
    assert!(
        !rest.query.block(),
        "a block re-read would widen back: {rest:?}"
    );
    let r = fetch(&rest.query, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange {
            start: 410,
            end: 452
        }]
    );
    // A match inside an oversized declaration keeps its context window
    // and offers the whole declaration.
    let matched = qj(
        &p,
        serde_json::json!({"matchString": "x200 = 200", "contextLines": 1, "block": true}),
    );
    let r = fetch(&matched, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange {
            start: 201,
            end: 203
        }]
    );
    let whole = r
        .next
        .and_then(|next| next.read_block)
        .expect("next.readBlock");
    let ranges: Vec<&str> = whole
        .query
        .ranges
        .iter()
        .map(|range| range.as_str())
        .collect();
    assert_eq!(ranges, ["1-200", "204-452"], "{whole:?}");
    assert!(whole.query.match_string.is_none() && !whole.query.block());
}

#[test]
fn a_long_matched_line_marks_both_cuts_and_offers_the_whole_line() {
    let t = Temp::new();
    let p = t.0.join("min.js");
    let line = format!("{}needleFn(){}", "a".repeat(3000), "b".repeat(3000));
    fs::write(&p, format!("head\n{line}\ntail\n")).expect("fixture");
    let paths = Paths(t.0.clone());
    let req = qj(&p, serde_json::json!({"matchString": "needleFn"}));
    let r = fetch(&req, &paths);
    let content = r.content.as_deref().expect("content");
    assert!(
        content.starts_with("... [2800 bytes omitted] ...\n"),
        "{content:.80}"
    );
    assert!(
        content.ends_with("\n... [2802 bytes omitted] ...\n"),
        "{content:.80}"
    );
    let wire = serde_json::to_value(&r).expect("serializable");
    crate::contracts::validate_output(
        "localFetch",
        &serde_json::json!({"results":[{"index":0,"data":wire}]}),
    )
    .expect("a row with next.wholeLines satisfies the output contract");
    let whole = r
        .next
        .and_then(|next| next.whole_lines)
        .expect("next.wholeLines");
    assert_eq!(whole.query.context_lines, Some(0), "{whole:?}");
    let r = fetch(&whole.query, &paths);
    assert_eq!(r.content, Some(format!("{line}\n")), "{:?}", r.warnings);
}

#[test]
fn ecma_lookbehind_uses_the_isolated_worker_when_available() {
    let worker =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/octocode-regex-worker");
    if !worker.exists() {
        return;
    }
    // On macOS a freshly compiled binary triggers a Gatekeeper security scan on first
    // launch that can easily exceed the 1-second deadline. Spawn the worker once with
    // no stdin so macOS completes the scan before the timed engine is created.
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new(&worker)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|mut c| c.wait());
    }
    let t = Temp::new();
    let p = t.0.join("regex.txt");
    fs::write(&p, "needle one\nneedle two\n").expect("regex fixture should be written");
    let mut request = q(&p);
    request.match_string = Some("(?<=needle )one".parse().expect("match string"));
    request.regex = Some(crate::contracts::tool_types::ReadRegex::Rust);
    request.context_lines = Some(0);
    let engine = std::sync::Arc::new(crate::regex::IsolatedRegexEngine::new(
        worker,
        crate::regex::IsolatedRegexLimits::default(),
    ));
    let regex = LocalFetchRegex::new(Some(engine));
    let result = execute_local_fetch(
        &request,
        &Paths(t.0.clone()),
        &Safe,
        &NeverCancel,
        &regex,
        None,
    );
    assert_eq!(
        result.content.as_deref(),
        Some("needle one\n"),
        "{:?}",
        result.error
    );
}

/// A whole-file read through one oversized (minified) line byte-chunks only
/// that line: once it ends, paging resumes by lines and every later line
/// keeps its number. The walk stays lossless.
#[test]
fn full_content_resumes_numbered_lines_after_one_oversized_line() {
    let t = Temp::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("strings.ts");
    let mut source = String::new();
    for i in 1..=1_000 {
        source.push_str(&format!("export const before_{i} = {i};\n"));
    }
    source.push_str(&format!("const table = \"{}\";\n", "x".repeat(27_500)));
    for i in 1..=300 {
        source.push_str(&format!("export const after_{i} = {i};\n"));
    }
    fs::write(&file, &source).expect("fixture");
    let mut query = q(&file);
    query.full_content = Some(true);
    // The standard view: every line carries its source number (`N\t`).
    query.minify = Some(MinifyMode::Standard);
    let mut pages = Vec::new();
    loop {
        let page = execute_local_fetch(
            &query,
            &paths,
            &Safe,
            &NeverCancel,
            &LocalFetchRegex::default(),
            Some(20_000),
        );
        assert_eq!(page.error_code, None, "{page:?}");
        let next = page.next.clone().and_then(|next| next.r#continue);
        pages.push(page);
        match next {
            Some(next) => query = next.query,
            None => break,
        }
        assert!(pages.len() < 40, "runaway paging");
    }
    // Every page after the long line is a numbered line page.
    let long_line = 1_001;
    let mut numbered_after = Vec::new();
    let mut text = String::new();
    for page in &pages {
        let content = page.content.clone().unwrap_or_default();
        let unit = page.pagination.as_ref().map(|p| p.unit);
        if unit == Some(WindowUnit::Lines) {
            for line in content.lines() {
                let (number, _) = line.split_once('\t').expect("numbered line");
                let number: usize = number.parse().expect("number");
                if number > long_line {
                    numbered_after.push(number);
                }
            }
        }
        text.push_str(&content);
    }
    assert_eq!(numbered_after, (1_002..=1_301).collect::<Vec<_>>());
    // Pages join into the numbered view exactly once: no gap, no repeat.
    let view = source
        .lines()
        .enumerate()
        .map(|(index, line)| format!("{}\t{line}\n", index + 1))
        .collect::<String>();
    // The numbered view ends without the source's final newline.
    let (text, view) = (text.trim_end_matches('\n'), view.trim_end_matches('\n'));
    let at = text
        .bytes()
        .zip(view.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(text.len().min(view.len()));
    assert!(
        text == view,
        "lossless walk diverges at byte {at} ({} vs {} bytes): got {:?} want {:?}",
        text.len(),
        view.len(),
        &text[at.saturating_sub(40)..(at + 80).min(text.len())],
        &view[at.saturating_sub(40)..(at + 80).min(view.len())]
    );
}

fn workspace_policy(root: &Path) -> crate::policy::path::PathPolicy {
    crate::policy::path::PathPolicy::new(crate::policy::path::PathPolicyConfig {
        workspace_root: Some(root.to_path_buf()),
        ..Default::default()
    })
    .expect("policy")
}

/// LF4: a directory is `notAFile` and leads to its tree.
#[test]
fn directory_path_is_not_a_file_with_tree_lead() {
    let t = Temp::new();
    fs::create_dir_all(t.0.join("src")).expect("dir");
    let mut req = LocalFetchQuery::test_default();
    req.path = "src".parse().expect("path");
    for result in [
        fetch(&req, &workspace_policy(&t.0)),
        fetch(&q(&t.0.join("src")), &Paths(t.0.clone())),
    ] {
        assert_eq!(result.error_code.as_deref(), Some("notAFile"), "{result:?}");
        let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
        assert_eq!(next["viewTree"]["tool"], "structureSearch", "{next}");
        assert_eq!(
            next["viewTree"]["query"]["queries"][0]["operation"], "tree",
            "{next}"
        );
    }
}

/// LF4: a missing file leads to a listing by its stem under the closest
/// existing directory.
#[test]
fn missing_file_leads_to_files_by_stem() {
    let t = Temp::new();
    fs::create_dir_all(t.0.join("src/b")).expect("dirs");
    fs::write(t.0.join("src/b/blok.ts"), "x\n").expect("moved");
    let mut req = LocalFetchQuery::test_default();
    req.path = "src/a/blok.rs".parse().expect("path");
    let result = fetch(&req, &workspace_policy(&t.0));
    assert_eq!(
        result.error_code.as_deref(),
        Some("pathNotFound"),
        "{result:?}"
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let listing = &next["findFile"]["query"]["queries"][0];
    assert_eq!(listing["operation"], "files", "{next}");
    assert_eq!(listing["path"], "src", "{next}");
    assert_eq!(listing["include"], serde_json::json!(["blok"]), "{next}");
}

/// LF4: a matchString miss leads to a localSearch of the file's directory.
#[test]
fn match_miss_leads_to_directory_search() {
    let t = Temp::new();
    let p = t.0.join("a.rs");
    fs::write(&p, "fn a() {}\n").expect("fixture");
    let result = fetch(
        &qj(&p, serde_json::json!({"matchString": "nowhere_here"})),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let search = &next["textSearch"];
    assert_eq!(search["tool"], "localSearch", "{next}");
    assert_eq!(
        search["query"]["queries"][0]["matchString"], "nowhere_here",
        "{next}"
    );
    assert_eq!(
        search["query"]["queries"][0]["path"],
        t.0.to_string_lossy().as_ref(),
        "{next}"
    );
}

const PY_TWO: &str = "import os\n\ndef first(a):\n    x = 1\n    return a\n\n\ndef second(b):\n    if b:\n        return first(1)\n    return 2\n";

/// LF3: a symbols outline names each multi-line declaration's line span.
#[test]
fn symbols_view_heads_carry_line_ranges() {
    let t = Temp::new();
    let p = t.0.join("m.py");
    fs::write(&p, PY_TWO).expect("fixture");
    let content = fetch(
        &qj(&p, serde_json::json!({"minify": "symbols"})),
        &Paths(t.0.clone()),
    )
    .content
    .expect("content");
    assert!(content.contains("3-5\tdef first(a):"), "{content}");
    assert!(content.contains("8-11\tdef second(b):"), "{content}");
    assert!(content.contains("1\timport os"), "{content}");
}

/// LF1: with a declaration hit present, a call-site hit keeps its window
/// instead of widening to its caller; the widened declaration is named.
#[test]
fn block_reports_widened_declarations() {
    let t = Temp::new();
    let p = t.0.join("m.py");
    fs::write(&p, PY_TWO).expect("fixture");
    let result = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": "first", "block": true, "contextLines": 0}),
        ),
        &Paths(t.0.clone()),
    );
    let body = serde_json::to_value(&result).expect("json");
    assert_eq!(
        body["blocks"],
        serde_json::json!([{"symbolName": "first", "line": 3, "endLine": 5}]),
        "{body}"
    );
    let content = result.content.expect("content");
    // The call on line 10 shows alone, not `second` (8-11) whole.
    assert!(content.contains("def first(a):"), "{content}");
    assert!(!content.contains("def second(b):"), "{content}");
    assert!(content.contains("return first(1)"), "{content}");
}

/// N3: a long-line source's default window is bounded by bytes, with the
/// full window one continuation away.
#[test]
fn minified_default_window_is_byte_bounded() {
    let t = Temp::new();
    let p = t.0.join("lodash.min.js");
    let text: String = (1..=140)
        .map(|n| {
            let tag = if n == 126 { "HIT" } else { "abc" };
            format!("var v{n:03}={tag};{}\n", "x".repeat(505))
        })
        .collect();
    fs::write(&p, &text).expect("fixture");
    let result = fetch(
        &qj(&p, serde_json::json!({"matchString": "HIT"})),
        &Paths(t.0.clone()),
    );
    let content = result.content.clone().expect("content");
    assert!(content.len() <= 2600, "{} bytes", content.len());
    assert!(content.contains("var v126=HIT;"), "{content}");
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    assert_eq!(
        next["expandContext"]["query"]["queries"][0]["contextLines"], 10,
        "{next}"
    );
    // Short-line sources keep the full ±10 window and no lead.
    let short = t.0.join("a.txt");
    fs::write(&short, numbered(40).replace("l20\n", "l20 HIT\n")).expect("fixture");
    let plain = fetch(
        &qj(&short, serde_json::json!({"matchString": "HIT"})),
        &Paths(t.0.clone()),
    );
    assert!(
        plain
            .next
            .as_ref()
            .is_none_or(|next| next.expand_context.is_none()),
        "{plain:?}"
    );
    assert_eq!(plain.content.expect("content").lines().count(), 21);
}

/// N3: a cut-declaration lead never points at a minified bundle's IIFE.
#[test]
fn read_block_lead_is_byte_capped() {
    let t = Temp::new();
    let p = t.0.join("bundle.js");
    let body: String = (1..=140)
        .map(|n| {
            let tag = if n == 70 { "HIT" } else { "abc" };
            format!("  var v{n:03}={tag};{}\n", "x".repeat(505))
        })
        .collect();
    fs::write(&p, format!("(function () {{\n{body}}})();\n")).expect("fixture");
    let result = fetch(
        &qj(&p, serde_json::json!({"matchString": "HIT"})),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(&result.next).expect("json");
    let ranges = next["readBlock"]["query"]["queries"][0]["ranges"].clone();
    let lines: usize = ranges
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(|range| {
                    let (a, b) = range.as_str()?.split_once('-')?;
                    Some(b.parse::<usize>().ok()? + 1 - a.parse::<usize>().ok()?)
                })
                .sum()
        })
        .unwrap_or(0);
    assert!(lines * 520 <= 32 * 1024, "{next}");
}

/// GF3: a plain read's readBlock covers every cut declaration, not just
/// the first hit's.
#[test]
fn read_block_covers_every_cut_declaration() {
    let t = Temp::new();
    let p = t.0.join("two.js");
    let fill = |name: &str| {
        (0..30)
            .map(|i| format!("  var {name}{i} = {i};\n"))
            .collect::<String>()
    };
    let text = format!(
        "function a(options) {{\n{}  var opts = options || {{}};\n{}}}\n\nfunction b(options) {{\n{}  var opts = options || {{}};\n{}}}\n",
        fill("a"),
        fill("p"),
        fill("b"),
        fill("q")
    );
    fs::write(&p, &text).expect("fixture");
    let result = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": "var opts = options || {}"}),
        ),
        &Paths(t.0.clone()),
    );
    let next = serde_json::to_value(&result.next).expect("json");
    let ranges = next["readBlock"]["query"]["queries"][0]["ranges"]
        .as_array()
        .unwrap_or_else(|| panic!("readBlock ranges: {next}"))
        .iter()
        .filter_map(|range| range.as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    // Lines of `a` (1-63) and `b` (65-127) outside the ±10 windows.
    let covers = |line: usize| {
        ranges.iter().any(|range| {
            range
                .split_once('-')
                .and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)))
                .is_some_and(|(a, b)| a <= line && line <= b)
        })
    };
    assert!(covers(2) && covers(62), "{ranges:?}");
    assert!(covers(66) && covers(126), "{ranges:?}");
}

/// repo-sweep: a `unit:"lines"` walk over a file holding one line longer
/// than a page reaches every source byte exactly once. Line pages continue
/// at the next line; the oversized line is served as byte pages that each
/// cite it as `sourceLineRanges: [{line, endLine}]` (the wire names, X1),
/// and line paging resumes on the following line.
#[test]
fn line_walk_over_an_oversized_line_covers_every_byte_once() {
    let t = Temp::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("bundle.js");
    let mut source = String::new();
    for i in 1..=1_164 {
        source.push_str(&format!("var line_{i} = {i};\n"));
    }
    let long_line = 1_165;
    source.push_str(&format!("    CSS: '{}',\n", "x".repeat(42_830)));
    for i in 1..=11 {
        source.push_str(&format!("tail_{i}();\n"));
    }
    let total_lines = long_line + 11;
    fs::write(&file, &source).expect("fixture");
    let mut query = qj(&file, serde_json::json!({"unit": "lines", "length": 400}));
    let (mut text, mut last_line, mut byte_pages, mut pages) = (String::new(), 0, 0, 0);
    loop {
        let page = fetch(&query, &paths);
        assert_eq!(page.error_code, None, "{page:?}");
        pages += 1;
        assert!(pages < 40, "runaway paging");
        let wire = serde_json::to_value(&page).expect("serializable");
        let content = page.content.clone().unwrap_or_default();
        let unit = page.pagination.as_ref().map(|p| p.unit);
        if unit == Some(WindowUnit::Bytes) {
            byte_pages += 1;
            assert_eq!(
                wire["sourceLineRanges"],
                serde_json::json!([{"line": long_line, "endLine": long_line}]),
                "a byte page cites the oversized line it serves"
            );
            last_line = long_line;
        } else {
            let ranges = &page.source_line_ranges;
            let first = ranges.first().map_or(0, |r| r.start);
            assert_eq!(
                first,
                last_line + 1,
                "line page {pages} must continue gap-free"
            );
            assert!(
                ranges.windows(2).all(|w| w[1].start == w[0].end + 1),
                "{ranges:?}"
            );
            last_line = ranges.last().map_or(last_line, |r| r.end);
        }
        text.push_str(&content);
        match page.next.and_then(|next| next.r#continue) {
            Some(next) => query = next.query,
            None => break,
        }
    }
    assert!(byte_pages >= 3, "the 42 KB line spans several byte pages");
    assert_eq!(last_line, total_lines);
    assert!(text == source, "pages join into the source exactly once");
}

/// H1: a regex read with more matches than the old 10,000 in-process cap
/// selects every matching line (never a silent prefix of them).
#[test]
fn regex_read_selects_every_match_past_ten_thousand() {
    use crate::contracts::tool_types::ReadRegex;
    let t = Temp::new();
    let p = t.0.join("many.txt");
    let source: String = (1..=12_000).map(|i| format!("hit {i}\n")).collect();
    fs::write(&p, &source).expect("fixture");
    for engine in [ReadRegex::Rust, ReadRegex::Pcre2] {
        let mut request = q(&p);
        request.match_string = Some("hit [0-9]+".parse().expect("match string"));
        request.regex = Some(engine);
        request.context_lines = Some(0);
        let result = fetch(&request, &Paths(t.0.clone()));
        assert_eq!(result.error, None, "{engine:?}");
        assert_eq!(result.selected_match_count, Some(12_000), "{engine:?}");
        assert!(
            result.warnings.iter().all(|w| !w.contains("match limit")),
            "{engine:?}: {:?}",
            result.warnings
        );
    }
}

/// H1: a regex read that reaches the in-process match limit discloses it:
/// isPartial, a warning naming the limit, and a localSearch lead that pages
/// every match of the same pattern in the same file.
#[test]
fn regex_read_past_the_match_limit_is_partial_with_a_search_lead() {
    use crate::contracts::tool_types::ReadRegex;
    let t = Temp::new();
    let p = t.0.join("dense.txt");
    let mut source = String::new();
    for _ in 0..(extraction::MAX_REGEX_MATCHES / 1_000 + 1) {
        source.push_str(&"x".repeat(1_000));
        source.push('\n');
    }
    fs::write(&p, &source).expect("fixture");
    for engine in [ReadRegex::Rust, ReadRegex::Pcre2] {
        let mut request = q(&p);
        request.match_string = Some("x".parse().expect("match string"));
        request.regex = Some(engine);
        request.context_lines = Some(0);
        let result = fetch(&request, &Paths(t.0.clone()));
        assert_eq!(result.error, None, "{engine:?}");
        assert_eq!(result.is_partial, Some(true), "{engine:?}");
        assert!(
            result.warnings.iter().any(|w| w.contains("match limit")),
            "{engine:?}: {:?}",
            result.warnings
        );
        let lead = result
            .next
            .as_ref()
            .and_then(|next| next.text_search.clone())
            .expect("textSearch lead");
        assert_eq!(lead["tool"], "localSearch", "{lead}");
        let row = &lead["query"]["queries"][0];
        assert_eq!(row["matchString"], "x", "{lead}");
        assert_eq!(row["path"], p.to_string_lossy().as_ref(), "{lead}");
    }
}

/// H1 when the read pages: the tool emits the textSearch lead beside the
/// page continuation (the response moves it to `hints.textSearch`).
#[test]
fn paged_regex_read_past_the_match_limit_keeps_its_search_lead() {
    use crate::contracts::tool_types::ReadRegex;
    let t = Temp::new();
    let p = t.0.join("hits.txt");
    let source: String = (1..=extraction::MAX_REGEX_MATCHES + 5)
        .map(|n| format!("hit {n}\n"))
        .collect();
    fs::write(&p, &source).expect("fixture");
    let mut request = q(&p);
    request.match_string = Some("hit".parse().expect("match string"));
    request.regex = Some(ReadRegex::Rust);
    let result = execute_local_fetch(
        &request,
        &Paths(t.0.clone()),
        &Safe,
        &NeverCancel,
        &LocalFetchRegex::default(),
        None,
    );
    assert!(
        result.warnings.iter().any(|w| w.contains("match limit")),
        "{:?}",
        result.warnings
    );
    let next = result.next.as_ref().expect("next");
    assert!(next.r#continue.is_some(), "the read pages: {next:?}");
    let lead = next
        .text_search
        .as_ref()
        .expect("textSearch lead on a paged read");
    assert_eq!(lead["tool"], "localSearch", "{lead}");
}
