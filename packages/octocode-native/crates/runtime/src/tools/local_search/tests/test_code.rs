//! `testCode` on localSearch: hits on the kept side of the test-code line,
//! whole files by path left unread, and what was left out counted.
use super::*;

/// One fixture per convention, every file but `plain.rs` with a hit: Rust
/// by path, mount, `#![cfg(test)]` and an inline `#[cfg(test)]` module
/// beside production code; TypeScript `*.test.ts` and `__tests__`; Python
/// `test_*.py`.
const MIXED: [(&str, &str); 10] = [
    (
        "src/lib.rs",
        "#[cfg(test)]\nmod helpers;\npub fn run() { needle(); }\n#[cfg(test)]\nmod tests {\n    fn t() { needle(); }\n}\n",
    ),
    ("src/helpers.rs", "pub fn h() { needle(); }\n"),
    ("src/gated.rs", "#![cfg(test)]\nfn g() { needle(); }\n"),
    ("src/plain.rs", "pub fn p() {}\n"),
    ("tests/it.rs", "fn it() { needle(); }\n"),
    ("web/a.ts", "needle\n"),
    ("web/a.test.ts", "needle\n"),
    ("web/__tests__/b.ts", "needle\n"),
    ("py/api.py", "needle\n"),
    ("py/test_api.py", "needle\n"),
];

fn scoped(test_code: &str, view: &str) -> serde_json::Value {
    search_fixture(
        &MIXED,
        ls_query(
            serde_json::json!({"matchString":["needle"],"testCode":test_code,"view":view}),
            None,
        ),
    )
}

/// `path:line` of every hit, sorted.
fn hits(body: &serde_json::Value) -> Vec<String> {
    let mut hits = body["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| {
            let path = file["path"].as_str().expect("path").to_owned();
            file["matches"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |hit| format!("{path}:{}", hit["line"]))
        })
        .collect::<Vec<_>>();
    hits.sort();
    hits
}

#[test]
fn exclude_keeps_production_hits_and_counts_the_test_ones() {
    let body = scoped("exclude", "content");
    assert_eq!(
        hits(&body),
        ["py/api.py:1", "src/lib.rs:3", "web/a.ts:1"],
        "{body}"
    );
    // Whole test files are left out as files: test paths and the mounted
    // module unread, `#![cfg(test)]` read; only the inline test module's
    // hit is a left-out line.
    assert_eq!(
        body["testCodeExcluded"],
        serde_json::json!({"files": 6, "lines": 1}),
        "{body}"
    );
    let counted = scoped("exclude", "countMatches");
    assert_eq!(counted["matchCount"], 3, "{counted}");
    assert_eq!(
        counted["testCodeExcluded"], body["testCodeExcluded"],
        "{counted}"
    );
}

#[test]
fn only_keeps_test_hits_and_counts_the_production_ones() {
    let body = scoped("only", "content");
    assert_eq!(
        hits(&body),
        [
            "py/test_api.py:1",
            "src/gated.rs:2",
            "src/helpers.rs:1",
            "src/lib.rs:6",
            "tests/it.rs:1",
            "web/__tests__/b.ts:1",
            "web/a.test.ts:1",
        ],
        "{body}"
    );
    assert_eq!(
        body["testCodeExcluded"],
        serde_json::json!({"files": 2, "lines": 1}),
        "{body}"
    );
}

/// The listed paths of a files or files-without view, sorted.
fn listed(body: &serde_json::Value) -> Vec<String> {
    let mut paths = body["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|file| file["path"].as_str().expect("path").to_owned())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

/// One file-level classifier for every view: a file that is test code
/// whole (by path, by a `#[cfg(test)]` mount, by `#![cfg(test)]`) is never
/// listed as production, by the files view or the files-without view; both
/// disclose the same left-out files, and their lists partition
/// `filesScanned`.
#[test]
fn files_and_files_without_classify_whole_test_files_alike() {
    let with = scoped("exclude", "files");
    let without = scoped("exclude", "filesWithout");
    assert_eq!(
        listed(&with),
        ["py/api.py", "src/lib.rs", "web/a.ts"],
        "{with}"
    );
    assert_eq!(listed(&without), ["src/plain.rs"], "{without}");
    for body in [&with, &without] {
        assert_eq!(body["testCodeExcluded"]["files"], 6, "{body}");
        assert_eq!(body["filesScanned"], 4, "{body}");
    }
}

#[test]
fn include_and_unscoped_search_everything_and_disclose_nothing() {
    for test_code in [None, Some("include")] {
        let mut fields = serde_json::json!({"matchString":["needle"]});
        if let Some(test_code) = test_code {
            fields["testCode"] = serde_json::json!(test_code);
        }
        let body = search_fixture(&MIXED, ls_query(fields, None));
        assert_eq!(hits(&body).len(), 10, "{body}");
        assert!(body.get("testCodeExcluded").is_none(), "{body}");
    }
}

const OPEN: &str = "pub mod sub;\n";
const GATED: &str = "#[cfg(test)]\npub mod sub;\n";

/// `src/sub`, a module of `src/lib.rs` (outside the search's root) that says
/// `declares` it, with enough settled hits for several pages; the path of
/// `lib.rs`.
fn module_beside_its_parent(root: &std::path::Path, declares: &str) -> std::path::PathBuf {
    let parent = root.join("src/lib.rs");
    fs::create_dir_all(root.join("src/sub")).expect("fixture dir");
    fs::write(&parent, declares).expect("parent");
    fs::write(root.join("src/sub/mod.rs"), "pub mod a;\n").expect("module");
    let hits = (0..400).fold(String::new(), |hits, line| {
        hits + &format!("pub fn needle_{line}() {{}}\n")
    });
    fs::write(root.join("src/sub/a.rs"), hits).expect("hits");
    settle();
    parent
}

/// A search under a `testCode` scope follows a parent module outside its
/// root: moving the module across the line leaves the pages stale, for a
/// directory and for one file.
#[test]
fn a_scoped_search_follows_a_parent_outside_its_root() {
    for (test_code, before, after) in [("exclude", OPEN, GATED), ("only", GATED, OPEN)] {
        for scope in ["src/sub", "src/sub/a.rs"] {
            let root = tempfile::tempdir().expect("fixture directory");
            let parent = module_beside_its_parent(root.path(), before);
            let (policy, security) = policy_for(root.path());
            let request = ls_query(
                serde_json::json!({"path": root.path().join(scope).to_string_lossy(), "matchString": ["needle"], "testCode": test_code}),
                None,
            );
            let snapshot = search_in(&request, &policy, &security, Some(6_000))
                .expect("page 1")
                .source_snapshot
                .expect("snapshot");
            assert!(
                manifest::get(&snapshot, &policy.identity()).is_none(),
                "{test_code} {scope}: the verdicts are not proved"
            );
            let next = ls_query(
                serde_json::json!({"snapshot": snapshot, "page": 2}),
                Some(&request),
            );
            search_in(&next, &policy, &security, Some(6_000)).expect("page 2");
            fs::write(&parent, after).expect("edit");
            let failed = search_in(&next, &policy, &security, Some(6_000))
                .expect_err("the module crossed the line");
            assert_eq!(
                failed.code,
                octocode_contract_types::ErrorCode::StaleSnapshot,
                "{}",
                failed.message
            );
        }
    }
}

/// Without a filtering scope nothing depends on a parent: the scan stays
/// stored and serves its later pages, whatever a parent says.
#[test]
fn a_search_with_no_test_code_filter_keeps_its_scan() {
    for test_code in [None, Some("include")] {
        let root = tempfile::tempdir().expect("fixture directory");
        let parent = module_beside_its_parent(root.path(), OPEN);
        let (policy, security) = policy_for(root.path());
        let mut fields = serde_json::json!({"path": root.path().join("src/sub").to_string_lossy(), "matchString": ["needle"]});
        if let Some(test_code) = test_code {
            fields["testCode"] = serde_json::json!(test_code);
        }
        let request = ls_query(fields, None);
        let snapshot = search_in(&request, &policy, &security, Some(6_000))
            .expect("page 1")
            .source_snapshot
            .expect("snapshot");
        fs::write(&parent, GATED).expect("gate");
        assert!(
            manifest::get(&snapshot, &policy.identity()).is_some(),
            "{test_code:?}"
        );
        let next = ls_query(
            serde_json::json!({"snapshot": snapshot, "page": 2}),
            Some(&request),
        );
        search_in(&next, &policy, &security, Some(6_000)).expect("page 2");
    }
}
