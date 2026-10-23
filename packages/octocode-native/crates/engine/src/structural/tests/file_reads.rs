//! How a directory search reads sources: text admission, walk errors and skips.

use super::*;

fn files_options(root: &TmpDir) -> StructuralSearchFilesOptions {
    StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(10),
        skip_files: None,
        max_file_bytes: None,
    }
}

/// Every reader admits a source by the one rule ([`crate::text::admit`]):
/// NUL-bearing UTF-8 is binary (not parsed), a leading UTF-8 BOM is no text,
/// BOM-marked UTF-16 is decoded and parsed; the digest names the file bytes.
#[test]
fn search_files_admits_sources_by_the_one_text_rule() {
    let root = TmpDir::new();
    fs::write(root.join("nul.ts"), b"target(x);\0").expect("nul");
    fs::write(root.join("bom.ts"), b"\xef\xbb\xbftarget(x);\n").expect("bom");
    let utf16: Vec<u8> = [0xff, 0xfe]
        .into_iter()
        .chain("target(y);\n".encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    fs::write(root.join("wide.ts"), &utf16).expect("utf16");

    let result = search_files_detailed(files_options(&root)).expect("search files");
    let row = |name: &str| {
        result
            .files
            .iter()
            .find(|file| file.path.ends_with(name))
            .expect("file row")
    };
    assert_eq!(result.skipped_unreadable, 1);
    assert_eq!(row("nul.ts").skipped_reason.as_deref(), Some("read"));
    assert_eq!(
        row("nul.ts").diagnostics[0].message,
        "Could not read file content as text: binary content."
    );
    assert_eq!(result.total_matches, 2);
    let (bom, wide) = (row("bom.ts"), row("wide.ts"));
    assert_eq!(bom.status, "ok");
    assert_eq!(wide.status, "ok");
    assert_eq!(
        bom.source_sha256.as_deref(),
        Some(crate::digest::sha256(b"\xef\xbb\xbftarget(x);\n").as_str())
    );
    assert_eq!(
        wide.source_sha256.as_deref(),
        Some(crate::digest::sha256(&utf16).as_str())
    );
    // The rebuild for a page sees the same text as the scan did.
    let rebuilt = file_matches(
        &root.join("wide.ts"),
        "ts",
        Some("target($X)"),
        None,
        wide.source_sha256.as_deref().expect("digest"),
        1 << 20,
    )
    .expect("rebuilt");
    assert!(matches!(rebuilt, Rebuilt::Matches(matches) if matches.len() == 1));
}

/// A directory the walk cannot read is counted and sampled, never dropped
/// silently: absence below it is unproven, so the scan is `partial`.
#[cfg(unix)]
#[test]
fn search_files_discloses_the_walk_errors_it_met() {
    use std::os::unix::fs::PermissionsExt;
    let root = TmpDir::new();
    fs::write(root.join("a.ts"), "target(x);\n").expect("hit");
    let locked = root.join("locked");
    fs::create_dir(&locked).expect("dir");
    fs::write(locked.join("b.ts"), "target(y);\n").expect("hidden hit");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
    // A privileged user reads it anyway: nothing to prove then.
    let readable = fs::read_dir(&locked).is_ok();

    let result = search_files_detailed(files_options(&root));
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("restore");
    let result = result.expect("search files");
    if readable {
        assert_eq!(result.walk_errors, WalkErrors::default());
        return;
    }
    assert_eq!(result.total_matches, 1);
    assert_eq!(
        (
            result.walk_errors.skipped,
            result.walk_errors.permission_denied
        ),
        (1, 1)
    );
    assert!(
        result.walk_errors.sample[0].contains("locked"),
        "{:?}",
        result.walk_errors.sample
    );
    assert_eq!(result.status, "partial");

    let clean = TmpDir::new();
    fs::write(clean.join("a.ts"), "target(x);\n").expect("hit");
    let clean = search_files_detailed(files_options(&clean)).expect("search files");
    assert_eq!(clean.walk_errors, WalkErrors::default());
    assert_eq!(clean.status, "ok");
}

/// The anchor prefilter reads a candidate once: a hit's bytes feed the
/// parse, and a file over the parse cap is streamed, never held. Each skip
/// keeps the reason and text the separate prefilter and parse reads gave.
#[test]
fn search_files_prefilter_read_feeds_the_parse_and_keeps_skip_reasons() {
    let root = TmpDir::new();
    fs::write(root.join("a.ts"), "target(value);\n").expect("hit");
    // Over the cap: the anchor sits past aho-corasick's stream buffer.
    let filler = "// filler line for the stream search\n".repeat(8_000);
    fs::write(root.join("big_hit.ts"), format!("{filler}target(value);\n")).expect("big hit");
    fs::write(root.join("big_miss.ts"), &filler).expect("big miss");
    fs::write(root.join("bad.ts"), b"target(\xff);\n").expect("not utf8");
    let options = |max_file_bytes| StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(10),
        skip_files: None,
        max_file_bytes,
    };
    let result = search_files_detailed(options(Some(1_000))).expect("search files");
    assert_eq!(result.total_matches, 1);
    assert_eq!(matched_paths(&result).len(), 1);
    assert_eq!(
        result.skipped_by_pre_filter, 1,
        "big_miss.ts lacks the anchor"
    );
    assert_eq!(result.skipped_large, 1, "big_hit.ts holds it past the cap");
    assert_eq!(result.skipped_unreadable, 1, "bad.ts is not UTF-8");
    let row = |name: &str| {
        result
            .files
            .iter()
            .find(|file| file.path.ends_with(name))
            .expect("file row")
    };
    let big = fs::metadata(root.join("big_hit.ts")).expect("meta").len();
    assert_eq!(
        row("big_hit.ts").skipped_reason.as_deref(),
        Some("maxFileBytes")
    );
    assert!(
        row("big_hit.ts").diagnostics[0]
            .message
            .starts_with(&format!("File is {big} bytes")),
        "{}",
        row("big_hit.ts").diagnostics[0].message
    );
    assert_eq!(
        row("big_miss.ts").skipped_reason.as_deref(),
        Some("preFilter")
    );
    assert_eq!(row("bad.ts").skipped_reason.as_deref(), Some("read"));
    assert_eq!(
        row("bad.ts").diagnostics[0].message,
        "Could not read file content as text: not valid UTF-8 or BOM-marked UTF-16 text."
    );
    // Under the default cap both big files parse; only big_hit.ts matches.
    let result = search_files_detailed(options(None)).expect("search files");
    assert_eq!(result.total_matches, 2);
    assert_eq!(result.skipped_large, 0);
    assert_eq!(result.skipped_by_pre_filter, 1);
}

// ── prefilter vs unsupported conflation (evidence: proof vs unevaluated) ─
//
// A `.txt` file that textually contains the anchor is not "anchor-absent"
// (proof of no match) — it's "unsupported extension" (not evaluated).
// `search_files` must report them on separate counters so the warning text
// can't collapse a proof-skip into an unevaluated-skip, the exact
// anti-pattern the evidence-grade contract forbids.

#[test]
fn search_files_separates_unsupported_from_prefilter_skips() {
    let root = TmpDir::new();
    // `match.ts` carries the anchor and matches the pattern.
    fs::write(root.join("match.ts"), "target(value);\n").expect("match");
    // `hasanchor.txt` textually contains the anchor but .txt has no
    // grammar — it must read as unsupported, NOT as a prefilter skip.
    fs::write(root.join("hasanchor.txt"), "target(value);\n").expect("txt");
    // `noanchor.ts` lacks the anchor — a genuine prefilter (proof) skip.
    fs::write(root.join("noanchor.ts"), "other(value);\n").expect("noanchor");

    let result = search_files_detailed(StructuralSearchFilesOptions {
        path: root.to_string_lossy().to_string(),
        pattern: Some("target($X)".to_owned()),
        rule: None,
        include: None,
        exclude_dir: None,
        default_exclude_dir: None,
        exclude: None,
        ignore: crate::search::walk::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(10),
        skip_files: None,
        max_file_bytes: None,
    })
    .expect("search files");

    assert_eq!(result.total_matches, 1);
    assert_eq!(result.parsed_files, 1);
    assert_eq!(
        result.skipped_by_pre_filter, 1,
        "only noanchor.ts is a proof-skip"
    );
    assert_eq!(
        result.skipped_unsupported, 1,
        "hasanchor.txt is unsupported, not prefilter"
    );
    // The warning text must name unsupported files distinctly — the lumped
    // "Pre-filter skipped parsing N file(s)" line is the imprecision we fix.
    let prefilter_warning = result
        .warnings
        .iter()
        .find(|w| w.starts_with("Pre-filter skipped parsing"));
    let unsupported_warning = result
        .warnings
        .iter()
        .find(|w| w.starts_with("Skipped") && w.contains("unsupported"));
    assert!(
        prefilter_warning.is_some(),
        "prefilter warning still present for the genuine proof-skip"
    );
    assert!(
        unsupported_warning.is_some(),
        "unsupported files need their own warning line, not lumped into prefilter: {:?}",
        result.warnings
    );
}
