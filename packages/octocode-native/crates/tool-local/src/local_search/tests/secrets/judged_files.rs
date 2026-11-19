//! A search judges only the files its matches could be in: a file whose own
//! bytes hold no match serves a judged text without one, so it is not
//! judged. A hidden value still makes no hit, and a placeholder is still
//! found where a read shows it.
use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

/// The files `request` lists under `root`, and the files the judge read
/// there.
fn listed(root: &std::path::Path, request: serde_json::Value) -> (Vec<String>, Vec<String>) {
    let names = |paths: Vec<std::path::PathBuf>| {
        let mut names = paths
            .iter()
            .filter(|path| path.starts_with(root))
            .map(|path| {
                path.file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    let judged = || {
        crate::local_search::executor::JUDGED_PATHS
            .lock()
            .expect("judged log")
            .clone()
    };
    let before = judged().len();
    let (policy, _) = policy_for(root);
    let security = ContentSecurity::new();
    let request = ls_query(
        json!({"path": root.to_string_lossy().into_owned(), "view": "files"}),
        Some(&ls_query(request, None)),
    );
    let rows = search_in(&request, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("serialize");
    let files = rows["files"]
        .as_array()
        .map(|files| {
            files
                .iter()
                .filter_map(|file| file["path"].as_str())
                .map(|path| path.rsplit('/').next().unwrap_or(path).to_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    (files, names(judged()[before..].to_vec()))
}

#[test]
fn only_files_whose_bytes_match_are_judged() {
    let root = tempfile::tempdir().expect("fixture directory");
    let root = root.path().canonicalize().expect("root");
    let token = format!("ghp_{}", "a1B2c3D4".repeat(5));
    fs::write(root.join("plain.txt"), "fn alpha() {}\n").expect("fixture");
    fs::write(
        root.join("tok.rs"),
        format!("const T: &str = \"{token}\"; // needle\n"),
    )
    .expect("fixture");
    fs::write(root.join("blob.bin"), b"\0\x01\x02 needle\n").expect("fixture");

    // No file's bytes hold it: none is judged.
    let (files, judged) = listed(&root, json_fixture(r#"{"matchString": ["defects-queue"]}"#));
    assert!(files.is_empty(), "{files:?}");
    assert!(judged.is_empty(), "{judged:?}");

    // Only the file whose bytes match is judged, and it is listed.
    let (files, judged) = listed(&root, json_fixture(r#"{"matchString": ["needle"]}"#));
    assert_eq!(files, ["tok.rs"]);
    assert_eq!(judged, ["tok.rs"]);

    // The hidden value, or a part of it, is no hit, though its bytes hold it.
    for needle in [token.as_str(), &token[4..14]] {
        let (files, judged) = listed(&root, json!({"matchString": [needle]}));
        assert!(files.is_empty(), "{needle}: {files:?}");
        assert!(judged.iter().any(|name| name == "tok.rs"), "{judged:?}");
        assert!(!judged.iter().any(|name| name == "plain.txt"), "{judged:?}");
    }

    // A placeholder is found where a read shows it, though no byte holds it:
    // a pattern that can match written text judges every text file.
    for needle in [
        "[REDACTED-GITHUBTOKENS]",
        "REDACTED-GITHUBTOKENS",
        "githubtokens",
    ] {
        let (files, judged) = listed(&root, json!({"matchString": [needle]}));
        assert_eq!(files, ["tok.rs"], "{needle}");
        assert!(
            judged.iter().any(|name| name == "plain.txt"),
            "{needle}: {judged:?}"
        );
    }
}

#[test]
fn the_written_text_a_match_could_take_is_every_placeholder() {
    use octocode_runtime_base::security::meets_written;
    for literal in [
        "[",
        "x]",
        "REDACTED",
        "redacted",
        "GITHUBTOKENS",
        "PRIVATEKEYFRAGMENT",
        "SANITIZER-FAILURE",
        "\u{212A}EYFRAGMENT",
        " indented",
        "\tx",
        "+added",
        "-removed",
        "a\nb",
        "a\rb",
        "bad\u{FFFD}",
    ] {
        assert!(meets_written(literal, true), "{literal:?}");
    }
    for literal in ["defects-queue", "fn alpha", "needle", "ghp_a1B2", "x = 1"] {
        assert!(!meets_written(literal, true), "{literal:?}");
    }
    // Compared with case, a lowercase name is no placeholder's.
    assert!(!meets_written("redacted", false));
    assert!(meets_written("REDACTED", false));
}

/// Several terms searched without case run as one alternation of
/// case-folded groups: a file whose bytes hold none of them is not judged,
/// as for one term, and a file whose bytes hold one in another case is
/// judged and found.
#[test]
fn several_terms_without_case_judge_only_files_whose_bytes_match() {
    let root = tempfile::tempdir().expect("fixture directory");
    let root = root.path().canonicalize().expect("root");
    fs::write(root.join("plain.txt"), "fn alpha() {}\n").expect("fixture");
    fs::write(root.join("other.rs"), "let walk = skip(ledger);\n").expect("fixture");
    fs::write(root.join("held.rs"), "struct WALK_SKIP_LEDGER;\n").expect("fixture");
    let terms = json!(["walkSkipLedger", "walk_skip_ledger", "skip ledger"]);
    let (files, judged) = listed(
        &root,
        json!({"matchString": terms, "caseMode": "insensitive", "path": "plain.txt"}),
    );
    let _ = files;
    let _ = judged;
    let (files, judged) = listed(
        &root,
        json!({"matchString": terms, "caseMode": "insensitive"}),
    );
    assert_eq!(files, ["held.rs"]);
    assert_eq!(judged, ["held.rs"], "{judged:?}");
    // Not one file's bytes hold any of them: none is judged.
    fs::remove_file(root.join("held.rs")).expect("remove");
    let (files, judged) = listed(
        &root,
        json!({"matchString": terms, "caseMode": "insensitive"}),
    );
    assert!(files.is_empty(), "{files:?}");
    assert!(judged.is_empty(), "{judged:?}");
}
