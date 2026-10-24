//! Partial and failed scopes: invalid globs, unreadable paths, NULs, binary
//! prefixes and text-cut files.

use super::*;

/// Make `dir` unreadable; returns false when the process can still read it
/// (e.g. running as root), so the caller skips.
#[cfg(unix)]
fn lock_dir(dir: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o000)).expect("chmod 000");
    if fs::read_dir(dir).is_ok() {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).expect("restore");
        return false;
    }
    true
}

#[cfg(unix)]
fn unlock_dir(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o755));
}

/// `root/hidden/secret.txt` holding the only `needle-only-here`, with
/// `hidden` made unreadable; `None` when the process can still read it.
#[cfg(unix)]
fn locked_secret(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let hidden = root.join("includeHidden");
    fs::create_dir(&hidden).expect("dir");
    fs::write(hidden.join("secret.txt"), "needle-only-here\n").expect("fixture");
    lock_dir(&hidden).then_some(hidden)
}

#[test]
fn binary_prefix_matches_mark_a_search_partial_and_terminal() {
    // Matches before the NUL do not prove the rest of the file was
    // searched: the cut is a coverage limit no continuation can lift.
    let body = search_fixture(
        &[("bin.dat", "foo\u{0}foo\n"), ("a.txt", "foo\n")],
        ls_query(
            serde_json::json!({"matchString": ["foo".to_string()]}),
            None,
        ),
    );
    assert_eq!(body["isPartial"], true, "{body}");
    assert_eq!(body["terminalLimit"], true, "{body}");
    assert!(body["next"].get("nextPage").is_none(), "{body}");
    assert_eq!(
        body["partialReasons"],
        serde_json::json!(["binaryQuit"]),
        "{body}"
    );
    // localFetch rejects binary files, so a binary file is never the read:
    // the read takes the shown text hit.
    let read = &crate::tools::test_support::checks::related_call(&body, "localFetch", |_| true);
    assert_eq!(read["tool"], "localFetch", "{body}");
    assert_eq!(
        read["input"]["queries"][0]["path"]
            .as_str()
            .map(|path| path.ends_with("a.txt")),
        Some(true),
        "{body}"
    );
    assert!(!body.to_string().contains("bin.dat\",\"ranges"), "{body}");
}

/// A malformed `include` glob is caller input, coded by the engine's typed
/// kind: a pattern that spells "glob" is not.
#[test]
fn a_bad_include_glob_is_invalid_glob() {
    let t = tempfile::tempdir().expect("tempdir");
    fs::write(t.path().join("a.rs"), "glob\n").expect("fixture");
    let (policy, security) = policy_for(t.path());
    let mut fields = serde_json::json!({"matchString": ["glob"], "include": ["src/{a"]});
    fields["path"] = t.path().to_string_lossy().into_owned().into();
    let error = execute_local_search(
        &ls_query(fields, None),
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect_err("bad glob");
    assert_eq!(error.code, ErrorCode::InvalidGlob, "{error:?}");
}

#[cfg(unix)]
#[test]
fn a_scope_where_every_path_is_unreadable_is_an_access_error_not_empty() {
    let root = tempfile::tempdir().expect("fixture directory");
    let Some(hidden) = locked_secret(root.path()) else {
        return;
    };
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": ["needle-only-here".to_string()], "regexEngine": "literal", "includeIgnored": true}),
        None,
    );
    let outcome =
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None);
    unlock_dir(&hidden);
    let error = outcome.expect_err("every candidate failed");
    assert_eq!(error.code, ErrorCode::FileAccessFailed);
    assert!(
        error.message.contains("Permission denied"),
        "{}",
        error.message
    );
    // The message names the count and first failure; no advice rides along.
    assert!(error.hints.is_empty(), "{:?}", error.hints);
}

#[cfg(unix)]
#[test]
fn unreadable_paths_beside_readable_misses_make_the_result_partial() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("open.txt"), "nothing here\n").expect("fixture");
    let Some(hidden) = locked_secret(root.path()) else {
        return;
    };
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": ["needle-only-here".to_string()], "regexEngine": "literal", "includeIgnored": true}),
        None,
    );
    let outcome =
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None);
    unlock_dir(&hidden);
    let result = outcome.expect("readable file searched");
    assert_eq!(result.status, SearchStatus::Partial);
    let body = serde_json::to_value(&result).expect("serialize");
    assert_eq!(body["isPartial"], true, "{body}");
    assert_eq!(body["terminalLimit"], true, "{body}");
    assert_eq!(body["fileErrors"]["count"], 1, "{body}");
    // The count is stated once, as a warning; no advice.
    assert!(
        body["warnings"].to_string().contains("1 path unreadable"),
        "{body}"
    );
    crate::tools::test_support::checks::assert_absent(&body["hints"], "unreadable");
}

#[test]
fn a_nul_keeps_the_matches_before_it_and_warns() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(
        root.path().join("mixed.txt"),
        b"alpha before\0alpha after\nalpha before\n",
    )
    .expect("fixture");
    let result = search_root(
        root.path(),
        serde_json::json!({"matchString": ["alpha".to_string()], "regexEngine": "literal"}),
    );
    assert_ne!(result.status, SearchStatus::Empty);
    let body = serde_json::to_value(&result).expect("serialize");
    assert_eq!(body["files"][0]["matches"][0]["line"], 1, "{body}");
    assert_eq!(body["matchCount"], 1, "{body}");
    // The warning names the file it could not search past the NUL.
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.starts_with("binaryFileSkipped: mixed.txt was searched")),
        "{:?}",
        result.warnings
    );
}

/// Every file cut at a NUL after real text is a coverage gap: the
/// warning names each one, files sharing a directory grouped under it,
/// and none summarized as a count.
#[test]
fn every_text_cut_file_is_named_grouped_by_directory() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir_all(root.path().join("deep/dir")).expect("fixture dir");
    for index in 0..8 {
        fs::write(
            root.path().join(format!("deep/dir/mixed{index}.txt")),
            b"alpha before\0alpha after\n",
        )
        .expect("fixture");
    }
    fs::write(root.path().join("top.txt"), b"alpha before\0alpha after\n").expect("fixture");
    let result = search_root(
        root.path(),
        serde_json::json!({"matchString": ["alpha".to_string()], "regexEngine": "literal"}),
    );
    assert!(result.is_partial);
    let warning = result
        .warnings
        .iter()
        .find(|w| w.starts_with("binaryFileSkipped:"))
        .expect("binary warning");
    let names = (0..8)
        .map(|index| format!("mixed{index}.txt"))
        .collect::<Vec<_>>()
        .join(",");
    assert!(
        warning.starts_with(&format!(
            "binaryFileSkipped: top.txt, deep/dir/{{{names}}} were searched"
        )),
        "{warning}"
    );
    assert!(!warning.contains("more"), "{warning}");
}

/// Past a handful, text-cut binary files are a count by extension with a
/// listing that names every one (`binarySkipped`), not an unbounded
/// inline warning: a build tree of `.rlib`s stays a short row.
#[test]
fn many_text_cut_files_are_a_count_with_a_lossless_listing() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir_all(root.path().join("deps")).expect("fixture dir");
    for index in 0..300 {
        fs::write(
            root.path()
                .join(format!("deps/libcrate_with_a_long_name_{index:03}.rlib")),
            b"alpha before\0alpha after\n",
        )
        .expect("fixture");
    }
    let request = ls_query(
        serde_json::json!({"path": root.path(), "matchString": ["alpha"], "regexEngine": "literal", "view": "files", "defaultExcludes": false}),
        None,
    );
    let (policy, security) = policy_for(root.path());
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let warning = result
        .warnings
        .iter()
        .find(|w| w.starts_with("binaryFileSkipped:"))
        .expect("binary warning");
    assert!(
        warning.starts_with("binaryFileSkipped: 300 files (.rlib 300) were"),
        "{warning}"
    );
    assert!(!warning.contains("libcrate"), "{warning}");
    let listing = result
        .next
        .as_ref()
        .and_then(|next| next.get("binarySkipped"))
        .expect("listing lead");
    assert_eq!(
        listing["input"]["queries"][0]["extensions"],
        serde_json::json!(["rlib"])
    );
    assert!(warning.len() < 200, "{warning}");
}

/// Files binary from their leading bytes (a font's magic, then a NUL)
/// are outside a text search: no partial result, a
/// count grouped by extension instead of every path, and an exact
/// structureSearch continuation that lists them all.
#[test]
fn leading_nul_binaries_are_counted_with_a_listing_continuation() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir_all(root.path().join("fonts/a")).expect("fixture dir");
    for index in 0..30 {
        fs::write(
            root.path()
                .join(format!("fonts/a/Font-{index}-0123456789abcdef.woff2")),
            b"wOF2\0\x01\0\0alpha",
        )
        .expect("fixture");
    }
    fs::write(
        root.path().join("logo.png"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .expect("fixture");
    fs::write(root.path().join("a.txt"), "alpha\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": ["alpha".to_string()], "regexEngine": "literal"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let body = serde_json::to_value(&result).expect("serialize");
    assert!(!result.is_partial, "{body}");
    assert!(!result.terminal_limit, "{body}");
    assert_eq!(result.status, SearchStatus::Success, "{body}");
    assert!(result.stats.cap_reasons.is_empty(), "{body}");
    // A row with hits states the count in stats, not as a warning.
    assert!(result.warnings.is_empty(), "{body}");
    assert_eq!(body["binarySkipped"], 31, "{body}");
    let text = body.to_string();
    assert!(!text.contains("Font-0"), "{text}");

    let listing =
        &crate::tools::test_support::checks::related_call(&body, "structureSearch", |_| true);
    assert_eq!(listing["tool"], "structureSearch", "{body}");
    let query = listing["input"]["queries"][0].clone();
    assert_eq!(query["operation"], "files", "{listing}");
    assert_eq!(
        query["extensions"],
        serde_json::json!(["woff2", "png"]),
        "{listing}"
    );
    let listed = list_structure(query, &policy, &security);
    for index in 0..30 {
        assert!(
            listed.contains(&format!("Font-{index}-0123456789abcdef.woff2")),
            "{listed}"
        );
    }
    assert!(listed.contains("logo.png"), "{listed}");
    assert!(!listed.contains("a.txt"), "{listed}");
}
