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

/// The `#[cfg(test)]` line belongs to the item it gates: `only` keeps a hit
/// on it and `exclude` leaves it out, as it does the item's own lines.
#[test]
fn the_cfg_test_attribute_line_belongs_to_its_item() {
    const GATED_MODULE: [(&str, &str); 1] = [(
        "src/lib.rs",
        "pub fn run() {}\n#[cfg(test)]\nmod tests {}\n",
    )];
    let search = |test_code: &str| {
        search_fixture(
            &GATED_MODULE,
            ls_query(
                serde_json::json!({"matchString":["cfg"],"regexEngine":"rust","testCode":test_code,"view":"content"}),
                None,
            ),
        )
    };
    let only = search("only");
    assert_eq!(hits(&only), ["src/lib.rs:2"], "{only}");
    let exclude = search("exclude");
    assert!(hits(&exclude).is_empty(), "{exclude}");
    assert_eq!(exclude["testCodeExcluded"]["lines"], 1, "{exclude}");
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

/// A production module's `tests.rs` (or `x_test.rs`) is production code
/// whatever its name, however deep its production parent lies: `exclude`
/// keeps its hits and `only` leaves them out.
#[test]
fn a_production_module_named_like_a_test_is_kept_by_exclude() {
    const NESTED: [(&str, &str); 4] = [
        ("src/lib.rs", "mod foo;\n"),
        ("src/foo.rs", "mod tests;\nmod x_test;\n"),
        ("src/foo/tests.rs", "fn a() { needle(); }\n"),
        ("src/foo/x_test.rs", "fn b() { needle(); }\n"),
    ];
    let search = |test_code: &str| {
        search_fixture(
            &NESTED,
            ls_query(
                serde_json::json!({"matchString":["needle"],"testCode":test_code,"view":"content"}),
                None,
            ),
        )
    };
    let exclude = search("exclude");
    assert_eq!(
        hits(&exclude),
        ["src/foo/tests.rs:1", "src/foo/x_test.rs:1"],
        "{exclude}"
    );
    let only = search("only");
    assert!(hits(&only).is_empty(), "{only}");
}

/// A spelled-test file that only a production `#[path]` mounts is production
/// code: `exclude` searches it, though a verdict by module roots cannot see
/// the declarer.
#[test]
fn exclude_searches_a_spelled_file_a_production_path_mounts() {
    const DIAMOND: [(&str, &str); 3] = [
        ("src/lib.rs", "mod a;\n"),
        ("src/a.rs", "#[path = \"shared/tests.rs\"]\nmod c;\n"),
        ("src/shared/tests.rs", "fn s() { needle(); }\n"),
    ];
    let search = |test_code: &str| {
        search_fixture(
            &DIAMOND,
            ls_query(
                serde_json::json!({"matchString":["needle"],"testCode":test_code,"view":"content"}),
                None,
            ),
        )
    };
    let exclude = search("exclude");
    assert_eq!(hits(&exclude), ["src/shared/tests.rs:1"], "{exclude}");
    let only = search("only");
    assert!(hits(&only).is_empty(), "{only}");
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
                manifest::get(&snapshot, &policy.identity()).is_some(),
                "{test_code} {scope}: what the verdicts read proves them"
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

/// A new crate root that mounts the searched test module by `#[path]` in
/// production moves it across the line, though no file the search read
/// changed: the pages go stale. (A new file nothing mounts, or one mounting it
/// only under test, places nothing: production by any parent wins.)
#[test]
fn a_scoped_search_follows_a_file_that_appears_and_mounts_it() {
    let root = tempfile::tempdir().expect("fixture directory");
    module_beside_its_parent(root.path(), GATED);
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().join("src/sub/a.rs").to_string_lossy(), "matchString": ["needle"], "testCode": "only"}),
        None,
    );
    let snapshot = search_in(&request, &policy, &security, Some(6_000))
        .expect("page 1")
        .source_snapshot
        .expect("snapshot");
    let next = ls_query(
        serde_json::json!({"snapshot": snapshot, "page": 2}),
        Some(&request),
    );
    search_in(&next, &policy, &security, Some(6_000)).expect("page 2");
    fs::write(
        root.path().join("src/main.rs"),
        "#[path = \"sub/a.rs\"]\nmod renamed;\n",
    )
    .expect("added");
    let failed = search_in(&next, &policy, &security, Some(6_000)).expect_err("a new mount");
    assert_eq!(
        failed.code,
        octocode_contract_types::ErrorCode::StaleSnapshot,
        "{}",
        failed.message
    );
}

/// The other side of that rule: a new file mounting a production module only
/// under test moves nothing, so the pages stay current.
#[test]
fn a_new_test_only_mount_leaves_a_production_module_current() {
    let root = tempfile::tempdir().expect("fixture directory");
    module_beside_its_parent(root.path(), OPEN);
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().join("src/sub/a.rs").to_string_lossy(), "matchString": ["needle"], "testCode": "exclude"}),
        None,
    );
    let snapshot = search_in(&request, &policy, &security, Some(6_000))
        .expect("page 1")
        .source_snapshot
        .expect("snapshot");
    let next = ls_query(
        serde_json::json!({"snapshot": snapshot, "page": 2}),
        Some(&request),
    );
    search_in(&next, &policy, &security, Some(6_000)).expect("page 2");
    fs::write(
        root.path().join("src/sub/added.rs"),
        "#[cfg(test)]\n#[path = \"a.rs\"]\nmod renamed;\n",
    )
    .expect("added");
    search_in(&next, &policy, &security, Some(6_000)).expect("production still wins");
}

/// Each side of a file past the parse limit stays on its side, and nothing is
/// left to warn about.
#[test]
fn a_file_too_large_to_parse_is_split_at_its_test_items() {
    let padding = "// padding\n".repeat(crate::security::SourceLimits::REAL.whole_source / 11 + 1);
    let big = format!(
        "pub fn needle_run() {{}}\n{padding}#[cfg(test)]\nmod tests {{\n    fn needle_test() {{}}\n}}\n"
    );
    let test_line = padding.lines().count() + 4;
    for (test_code, line) in [("only", test_line), ("exclude", 1)] {
        let body = search_fixture(
            &[("src/big.rs", big.as_str())],
            ls_query(
                serde_json::json!({"matchString":["needle"],"testCode":test_code}),
                None,
            ),
        );
        assert_eq!(hits(&body), [format!("src/big.rs:{line}")], "{test_code}");
        assert_eq!(body["testCodeExcluded"]["lines"], 1, "{test_code}");
        assert!(body.get("isPartial").is_none(), "{test_code}");
    }
}

/// A parent no read can admit leaves its modules unproven, and the row says so.
#[test]
fn a_parent_that_cannot_be_read_leaves_the_row_partial() {
    let root = tempfile::tempdir().expect("fixture directory");
    let parent = module_beside_its_parent(root.path(), OPEN);
    let over = u64::try_from(octocode_engine::memory::MEMORY.limit()).expect("limit") + 1;
    fs::File::options()
        .write(true)
        .open(&parent)
        .and_then(|file| file.set_len(over))
        .expect("sparse parent");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().join("src/sub").to_string_lossy(), "matchString": ["needle"], "testCode": "exclude", "view": "files"}),
        None,
    );
    let body = search_in(&request, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("serialize");
    assert_eq!(body["isPartial"], true, "{body}");
    assert!(
        body.to_string().contains("testCode verdict"),
        "names why: {body}"
    );
}

/// A file only a `#[path]` mounts under test costs a walk nothing until its
/// verdict shows: read, then left out by its hits, and left out of a listing of
/// files without any. One with no hits is not counted among those left out.
#[test]
fn a_path_mounted_file_is_decided_when_its_verdict_shows() {
    const FILES: [(&str, &str); 5] = [
        ("src/lib.rs", "pub fn open() {}\n"),
        (
            "src/fixtures.rs",
            "#[cfg(test)]\n#[path = \"hit_x.rs\"]\nmod hit;\n#[cfg(test)]\n#[path = \"quiet_x.rs\"]\nmod quiet;\n",
        ),
        ("src/hit_x.rs", "fn needle() {}\n"),
        ("src/quiet_x.rs", "fn other() {}\n"),
        ("src/prod.rs", "fn needle() {}\n"),
    ];
    let search = |test_code: &str, view: &str| {
        search_fixture(
            &FILES,
            ls_query(
                serde_json::json!({"matchString":["needle"],"testCode":test_code,"view":view}),
                None,
            ),
        )
    };
    let exclude = search("exclude", "files");
    assert_eq!(listed(&exclude), ["src/prod.rs"], "{exclude}");
    assert_eq!(exclude["testCodeExcluded"]["files"], 1, "{exclude}");
    let only = search("only", "files");
    assert_eq!(listed(&only), ["src/hit_x.rs"], "{only}");
    let without = search("exclude", "filesWithout");
    assert_eq!(
        listed(&without),
        ["src/fixtures.rs", "src/lib.rs"],
        "{without}"
    );
}

/// A declaration hit's lspSearch lead runs with the search's `testCode`:
/// a search that left tests out asks for callers outside tests too. A
/// fixture server stands for the file's language server.
#[cfg(unix)]
#[test]
fn the_lsp_lead_keeps_the_searchs_test_code() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().expect("home");
    let command = home.path().join("fixture-lsp");
    std::fs::write(&command, "#!/bin/sh\nexit 0\n").expect("server");
    std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755)).expect("executable");
    std::fs::write(
        home.path().join("lsp-servers.json"),
        serde_json::json!({"languageServers":{".rs":{"command":command,"args":[],"languageId":"rust"}}})
            .to_string(),
    )
    .expect("config");
    let options = octocode_engine::lsp::config::LspDiscoveryOptions {
        octocode_home: Some(home.path().to_path_buf()),
        ..Default::default()
    };
    let search = |test_code: serde_json::Value| {
        crate::tools::leads::lsp::with_lead_discovery(options.clone(), || {
            search_fixture(
                &[(
                    "src/lib.rs",
                    "pub fn helper() {}\npub fn run() { helper(); }\n",
                )],
                ls_query(
                    serde_json::json!({"matchString":["helper"],"testCode":test_code}),
                    None,
                ),
            )
        })
    };
    let lead_of = |body: &serde_json::Value| {
        crate::tools::test_support::checks::related_call(body, "lspSearch", |_| true)["input"]
            ["queries"][0]
            .clone()
    };
    let excluded = search(serde_json::json!("exclude"));
    let lead = lead_of(&excluded);
    assert_eq!(lead["operation"], "callers", "{excluded}");
    assert_eq!(lead["testCode"], "exclude", "{excluded}");
    let unset = search(serde_json::Value::Null);
    let lead = lead_of(&unset);
    assert_eq!(lead["operation"], "callers", "{unset}");
    assert!(lead.get("testCode").is_none(), "{unset}");
}

/// Relevance ranks test code after source by the `testCode` verdict, not by
/// spelling alone: a module mounted only under `#[cfg(test)]` that declares
/// the searched name follows the production declaration, and the symbol's
/// lspSearch lead names the production one.
#[test]
fn relevance_ranks_a_test_only_mount_after_the_source_declaration() {
    let body = search_fixture(
        &[
            (
                "src/lib.rs",
                "#[cfg(test)]\nmod a_support;\nmod b_source;\n",
            ),
            ("src/a_support.rs", "fn needle() {}\n"),
            ("src/b_source.rs", "pub fn needle() {}\n"),
        ],
        ls_query(serde_json::json!({"matchString":["needle"]}), None),
    );
    let order = body["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|file| file["path"].as_str().expect("path").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(order, ["src/b_source.rs", "src/a_support.rs"], "{body}");
    let lsp = body["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["tool"] == "lspSearch")
        .expect("an lspSearch lead");
    assert!(
        lsp["input"]["queries"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("src/b_source.rs")),
        "{lsp}"
    );
}

/// Inline test declarations and calls keep their source evidence, but do not
/// outrank a production declaration simply because their file has more hits.
#[test]
fn relevance_inline_test_only_hits_follow_production_without_losing_matches() {
    let files = [
        (
            "src/a_inline.rs",
            "pub fn other() {}\n#[cfg(test)]\nmod checks {\n    fn needle() {}\n    fn check() {\n        needle();\n        needle();\n    }\n}\n",
        ),
        ("src/z_source.rs", "pub fn needle() {}\n"),
    ];
    for view in ["content", "matchOnly", "files"] {
        let body = search_fixture(
            &files,
            ls_query(
                serde_json::json!({"matchString": ["needle"], "view": view}),
                None,
            ),
        );
        let paths = body["files"]
            .as_array()
            .expect("files")
            .iter()
            .map(|file| file["path"].as_str().expect("path"))
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            ["src/z_source.rs", "src/a_inline.rs"],
            "{view}: {body}"
        );
        if view != "files" {
            assert_eq!(
                hits(&body),
                [
                    "src/a_inline.rs:4",
                    "src/a_inline.rs:6",
                    "src/a_inline.rs:7",
                    "src/z_source.rs:1"
                ],
                "{view}: {body}"
            );
            assert!(
                body.get("testCodeExcluded").is_none(),
                "ranking excludes nothing: {body}"
            );
        }
    }
    let body = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": ["needle"], "sort": {"by": "path", "order": "asc"}}),
            None,
        ),
    );
    assert_eq!(
        body["files"][0]["path"], "src/a_inline.rs",
        "path sort stays literal"
    );
}

/// A first clipped match page contains production evidence even when all
/// earlier hits lie inside an inline test module. Source order within the
/// selected page and the test declaration's identity remain intact.
#[test]
fn relevance_inline_production_hit_reaches_the_first_clipped_page() {
    let root = tempfile::tempdir().expect("fixture");
    let test_hits = local_search_query::default_matches_per_file() + 2;
    let mut text = "#[cfg(test)]\nmod checks {\n    fn needle() {}\n    fn check() {\n".to_owned();
    for _ in 0..test_hits {
        text.push_str("        needle();\n");
    }
    text.push_str("    }\n}\n// needle in production prose\n");
    let production_line = text.lines().count();
    fs::write(root.path().join("mixed.rs"), &text).expect("source");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({
            "path": root.path().to_string_lossy(), "matchString": ["needle"], "limit": 1
        }),
        None,
    );
    let result = search_in(&request, &policy, &security, Some(1_200)).expect("clipped search");
    let body = result.to_row().expect("row");
    let hits = body["files"][0]["matches"].as_array().expect("matches");
    assert!(
        hits.iter()
            .any(|hit| hit["line"].as_u64() == Some(production_line as u64)),
        "production hit must reach the first page: {body}"
    );
    assert!(
        hits.len() < usize::try_from(test_hits + 2).expect("count"),
        "actually clipped: {body}"
    );
    let whole = search_fixture(
        &[("mixed.rs", text.as_str())],
        ls_query(serde_json::json!({"matchString":["needle"]}), None),
    );
    let declaration = whole["files"][0]["matches"]
        .as_array()
        .expect("whole matches")
        .iter()
        .find(|hit| hit["line"] == 3)
        .expect("test declaration");
    assert_eq!(
        declaration["declaration"], true,
        "a test declaration is still a declaration"
    );
}
