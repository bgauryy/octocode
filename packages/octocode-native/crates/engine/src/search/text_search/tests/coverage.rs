//! Coverage gaps: walk and read errors, oversize files, binary cuts and opaque binaries.

use super::*;

#[test]
fn collection_errors_do_not_report_complete_absence() {
    let t = TmpDir::new();
    let missing = t.0.join("missing").to_string_lossy().into_owned();
    let result = search(opts(missing, "needle")).expect("partial search");
    assert!(result.files.is_empty());
    assert_eq!(result.stats.error_count, Some(1));
    assert!(
        result
            .stats
            .first_error
            .as_ref()
            .is_some_and(|e| !e.is_empty())
    );
    assert_eq!(result.stats.capped, Some(false));
}

#[cfg(unix)]
#[test]
fn collection_errors_preserve_successful_files() {
    use std::os::unix::fs::PermissionsExt;
    let t = TmpDir::new();
    t.write("good.txt", "needle\n");
    t.write("denied.txt", "needle\n");
    let denied = t.0.join("denied.txt");
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)).unwrap();
    // Privileged test runners can read mode-000 files; the missing-root test
    // remains deterministic on those hosts.
    let inaccessible = fs::read(&denied).is_err();
    let result = search(opts(t.path(), "needle"));
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o600)).unwrap();
    if !inaccessible {
        return;
    }
    let result = result.expect("partial search");
    assert_eq!(result.files.len(), 1);
    assert!(result.files[0].path.ends_with("good.txt"));
    assert_eq!(result.stats.error_count, Some(1));
    assert!(
        result
            .stats
            .first_error
            .as_ref()
            .is_some_and(|e| e.contains("denied.txt"))
    );
}

#[test]
fn oversize_file_is_skipped_with_diagnostic_and_normal_file_still_matches() {
    // A file above the byte ceiling is skipped before it is searched and
    // surfaced as a `maxFileSize` diagnostic; a normal file still matches.
    let t = TmpDir::new();
    t.write("small.txt", "needle\n"); // 7 bytes, under the ceiling
    t.write("big.txt", "needle ".repeat(50)); // 350 bytes, over the ceiling
    let mut o = opts(t.path(), "needle");
    o.max_file_bytes = Some(10);
    let r = search(o).expect("ok");
    assert_eq!(r.files.len(), 1);
    assert!(r.files[0].path.ends_with("small.txt"));
    assert_eq!(r.stats.capped, Some(true));
    assert!(r.stats.cap_reasons.contains(&CapReason::MaxFileSize));
    // The skipped file is not counted as covered (no overstated filesSearched).
    assert_eq!(r.stats.files_searched, Some(1));
}

#[test]
fn binary_quit_file_is_flagged_not_silently_absent() {
    // A file quit as binary (NUL byte) is reflected in a diagnostic, and
    // an ordinary text file still matches.
    let t = TmpDir::new();
    t.write("data.bin", "needle before\u{0}needle after\n");
    t.write("text.txt", "needle plain\n");
    let r = search(opts(t.path(), "needle")).expect("ok");
    assert!(r.files.iter().any(|f| f.path.ends_with("text.txt")));
    assert_eq!(r.stats.capped, Some(true));
    assert!(r.stats.cap_reasons.contains(&CapReason::BinaryQuit));
}

#[test]
fn files_without_match_omits_binary_files_quit_before_matching() {
    let t = TmpDir::new();
    t.write("data.bin", "\u{0}needle\n");
    t.write("plain.txt", "nothing here\n");
    t.write("hit.txt", "needle\n");
    let mut o = opts(t.path(), "needle");
    o.output = SearchOutput::FilesWithoutMatch;
    let r = search(o).expect("ok");
    let paths: Vec<_> = r.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths.len(), 1, "{paths:?}");
    assert!(paths[0].ends_with("plain.txt"));
}

/// A file whose first NUL comes before any text (image, object file, archive
/// headers) is opaque binary: skipped, counted, and not a
/// coverage gap. Only a NUL after text leaves searchable text unread.
#[test]
fn opaque_binary_files_are_skipped_not_a_coverage_gap() {
    let t = TmpDir::new();
    fs::write(
        t.0.join("logo.png"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR needle",
    )
    .expect("png");
    fs::write(t.0.join("addon.node"), b"\x7fELF\x02\x01\x01\0needle").expect("elf");
    fs::write(t.0.join("font.ttf"), b"\0\x01\0\0needle").expect("ttf");
    t.write("text.txt", "needle plain\n");
    let r = search(opts(t.path(), "needle")).expect("ok");
    assert_eq!(r.files.len(), 1, "{:?}", r.files);
    assert!(r.files[0].path.ends_with("text.txt"));
    assert_eq!(r.stats.cap_reasons, [], "{:?}", r.stats);
    assert_ne!(r.stats.capped, Some(true), "{:?}", r.stats);
    assert_eq!(r.stats.binary_files, None, "{:?}", r.stats);
    assert_eq!(r.stats.skipped_binary_count, Some(3), "{:?}", r.stats);
    assert_eq!(
        extension_groups(&r.stats),
        [("node", 1), ("png", 1), ("ttf", 1)],
        "{:?}",
        r.stats
    );
    assert_eq!(r.stats.files_searched, Some(4), "{:?}", r.stats);

    // files-without-match still never lists a file it could not read as text.
    let mut o = opts(t.path(), "needle");
    o.output = SearchOutput::FilesWithoutMatch;
    assert!(search(o).expect("ok").files.is_empty());
}

/// Bytes of a binary header that happen to match are not text hits: the
/// file is skipped and counted, never returned with rows a text read fails on.
#[test]
fn a_match_inside_an_opaque_binary_header_is_not_a_hit() {
    let t = TmpDir::new();
    fs::write(t.0.join("logo.png"), b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").expect("png");
    let r = search(opts(t.path(), "PNG")).expect("ok");
    assert!(r.files.is_empty(), "{:?}", r.files);
    assert_eq!(r.stats.skipped_binary_count, Some(1), "{:?}", r.stats);
    assert_eq!(r.stats.files_matched, Some(0), "{:?}", r.stats);
}

#[test]
fn binary_quit_keeps_matches_before_the_nul() {
    let t = TmpDir::new();
    fs::write(
        t.0.join("mixed.txt"),
        b"alpha before\0alpha after\nalpha before\n",
    )
    .expect("fixture");
    let r = search(opts(t.path(), "alpha")).expect("ok");
    assert_eq!(r.files.len(), 1, "{:?}", r.stats);
    let matches = &r.files[0].matches;
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].line, 1);
    assert_eq!(matches[0].value, "alpha before");
    assert_eq!(r.stats.match_count, Some(1));
    assert_eq!(r.stats.cap_reasons, [CapReason::BinaryQuit]);
}

fn extension_groups(stats: &TextSearchStats) -> Vec<(&str, u32)> {
    stats
        .skipped_binary_extensions
        .iter()
        .flatten()
        .map(|group| (group.extension.as_str(), group.count))
        .collect()
}

/// A format whose magic is printable (woff2, OpenType, SQLite) still puts
/// its NUL inside the leading bytes, before any complete line: it is binary
/// like an image, not text cut short. Skipped, counted by extension.
#[test]
fn a_printable_magic_before_a_leading_nul_is_opaque_binary() {
    let t = TmpDir::new();
    fs::write(t.0.join("a.woff2"), b"wOF2\0\x01\0\0needle").expect("woff2");
    fs::write(t.0.join("b.woff2"), b"wOF2\0\x01\0\0needle").expect("woff2");
    fs::write(t.0.join("c.OTF"), b"OTTO\0\x0b\0\x80needle").expect("otf");
    fs::write(t.0.join("store"), b"SQLite format 3\0needle").expect("sqlite");
    t.write("text.txt", "needle plain\n");
    let r = search(opts(t.path(), "needle")).expect("ok");
    assert_eq!(r.files.len(), 1, "{:?}", r.files);
    assert_eq!(r.stats.cap_reasons, [], "{:?}", r.stats);
    assert_eq!(r.stats.binary_files, None, "{:?}", r.stats);
    assert_eq!(r.stats.skipped_binary_count, Some(4), "{:?}", r.stats);
    // Most files first; the extension is lowercased like structureSearch
    // matches it, and an extensionless file groups under "".
    assert_eq!(
        extension_groups(&r.stats),
        [("woff2", 2), ("", 1), ("otf", 1)],
        "{:?}",
        r.stats
    );
    // Only the extensionless group names its files.
    let names = |extension: &str| {
        r.stats
            .skipped_binary_extensions
            .iter()
            .flatten()
            .find(|group| group.extension == extension)
            .map(|group| group.names.clone())
    };
    assert_eq!(names(""), Some(vec!["store".to_owned()]));
    assert_eq!(names("woff2"), Some(Vec::new()));
}

/// Text lines before a NUL are searchable text the cut leaves unread: a
/// coverage gap even when nothing before the NUL matched.
#[test]
fn text_lines_before_a_nul_stay_a_coverage_gap() {
    let t = TmpDir::new();
    fs::write(t.0.join("log.txt"), b"first line\nsecond line\n\0needle\n").expect("log");
    let r = search(opts(t.path(), "needle")).expect("ok");
    assert!(r.files.is_empty(), "{:?}", r.files);
    assert_eq!(r.stats.cap_reasons, [CapReason::BinaryQuit]);
    assert_eq!(r.stats.skipped_binary_count, None, "{:?}", r.stats);
    assert_eq!(r.stats.binary_file_count, Some(1));
    let named = r.stats.binary_files.unwrap_or_default();
    assert!(
        named.len() == 1 && named[0].ends_with("log.txt"),
        "{named:?}"
    );
}

/// A match before the NUL is evidence from a file the search could not
/// finish, so even a short single-line prefix keeps the file a named gap.
#[test]
fn a_match_before_a_leading_nul_keeps_the_file_a_gap() {
    let t = TmpDir::new();
    fs::write(t.0.join("a.woff2"), b"wOF2\0\x01\0\0").expect("woff2");
    let r = search(opts(t.path(), "wOF2")).expect("ok");
    assert_eq!(r.files.len(), 1, "{:?}", r.files);
    assert_eq!(r.stats.cap_reasons, [CapReason::BinaryQuit]);
    assert_eq!(r.stats.skipped_binary_count, None, "{:?}", r.stats);
}

/// A long run of text before a NUL is not a format header, line break or
/// not: it stays a coverage gap.
#[test]
fn a_long_text_run_before_a_nul_stays_a_gap() {
    let t = TmpDir::new();
    let mut body = "x".repeat(4096).into_bytes();
    body.extend_from_slice(b"\0needle\n");
    fs::write(t.0.join("bundle.js"), body).expect("bundle");
    let r = search(opts(t.path(), "needle")).expect("ok");
    assert_eq!(r.stats.cap_reasons, [CapReason::BinaryQuit]);
    assert_eq!(r.stats.skipped_binary_count, None, "{:?}", r.stats);
}

#[test]
fn binary_prefix_counts_matches_across_earlier_lines() {
    let t = TmpDir::new();
    fs::write(
        t.0.join("mixed.txt"),
        b"alpha one\nbeta\nalpha two\0alpha three\n",
    )
    .expect("fixture");
    let r = search(opts(t.path(), "alpha")).expect("ok");
    let lines: Vec<u32> = r.files[0].matches.iter().map(|m| m.line).collect();
    assert_eq!(lines, vec![1, 3]);
}

#[test]
fn open_regular_refuses_a_symlink_swapped_in_after_the_walk() {
    let t = TmpDir::new();
    t.write("real.txt", "needle\n");
    let (mut file, meta) = open_regular(&t.0.join("real.txt")).expect("a regular file opens");
    assert!(meta.is_file());
    assert_eq!(meta.len(), 7);
    let mut text = String::new();
    std::io::Read::read_to_string(&mut file, &mut text).expect("read");
    assert_eq!(text, "needle\n");
    #[cfg(unix)]
    {
        let not_regular = |path: &Path| {
            let error = open_regular(path).expect_err("refused");
            assert!(
                error.to_string().contains("not a regular file when opened"),
                "{}: {error}",
                path.display()
            );
        };
        std::os::unix::fs::symlink(t.0.join("real.txt"), t.0.join("link.txt")).expect("symlink");
        // O_NOFOLLOW: the link itself is never opened.
        let link = open_regular(&t.0.join("link.txt")).expect_err("symlink refused");
        assert_eq!(link.raw_os_error(), Some(libc::ELOOP), "{link}");
        fs::create_dir(t.0.join("dir")).expect("dir");
        not_regular(&t.0.join("dir"));
        let fifo = t.0.join("pipe");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .is_ok_and(|status| status.success());
        if made {
            // Non-blocking open: a FIFO with no writer must not hang.
            not_regular(&fifo);
        }
    }
}
