//! Empty searches: their hints, the leads that verify absence, and their repairs.
use super::*;
use octocode_test_support::json_fixture;

/// An empty code search for `row` against `a/b`: no index hit, a live repo.
async fn run_empty(row: serde_json::Value) -> octocode_runtime_base::response::result::ToolData {
    run_empty_in(row, false).await
}

/// [`run_empty`] against a repository that is `archived` or not.
async fn run_empty_in(
    row: serde_json::Value,
    archived: bool,
) -> octocode_runtime_base::response::result::ToolData {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{"total_count": 0, "incomplete_results": false, "items": []}"#,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "full_name": "a/b",
            "default_branch": "main",
            "archived": archived
        })))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(row).expect("query");
    let context = default_context();
    execute(
        &provider,
        &query,
        &context,
        &Passthrough,
        &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
    )
    .await
    .expect("empty search")
}

/// An empty repo-scoped search is unproven absence. A miss never leads to a
/// clone of the whole repository (no `paths` to bound it), whatever the
/// filters; only a `path` scope leads to its listing.
#[tokio::test]
async fn an_empty_repo_search_never_leads_to_an_unscoped_clone() {
    for row in [
        json_fixture(r#"{"owner": "a", "repo": "b", "keywords": ["zzqq"]}"#),
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["zzqq"],
                "ref": "v1"
            }"#,
        ),
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["zzqq"],
                "extensions": ["ts"]
            }"#,
        ),
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["zzqq"],
                "path": "src"
            }"#,
        ),
    ] {
        let out = run_empty(row.clone()).await;
        assert!(
            octocode_runtime_base::test_support::checks::find_related_call(
                &out.data,
                "ghCloneRepo",
                |_| { true }
            )
            .is_none(),
            "{row}: {}",
            out.data
        );
        assert_eq!(
            octocode_runtime_base::test_support::checks::find_related_call(
                &out.data,
                "ghStructure",
                |input| { input["queries"][0]["operation"] == "tree" }
            )
            .is_some(),
            row.get("path").is_some(),
            "{row}: {}",
            out.data
        );
    }
}

/// An empty repo-wide search lists nothing: the tool description routes
/// unindexed files and other refs to ghStructure, so a root listing would
/// restate it. A ref the index does not cover is named in the hint; a
/// search with a narrowing to drop leads to `broadenSearch`.
#[tokio::test]
async fn an_empty_repo_wide_search_offers_no_root_listing() {
    let out = run_empty(json_fixture(
        r#"{
            "owner": "a",
            "repo": "b",
            "keywords": ["zzqq"],
            "ref": "v1"
        }"#,
    ))
    .await;
    assert!(out.data.get("related").is_none(), "{}", out.data);
    assert_eq!(
        out.data["hints"],
        json_fixture(
            r#"["Code search indexes only the default branch `main`; `v1` was not searched."]"#
        ),
        "{}",
        out.data
    );
    let out = run_empty(json_fixture(
        r#"{"owner": "a", "repo": "b", "keywords": ["zzqq", "yyrr"]}"#,
    ))
    .await;
    assert!(
        octocode_runtime_base::test_support::checks::find_related_call(
            &out.data,
            "ghStructure",
            |input| { input["queries"][0]["operation"] == "tree" }
        )
        .is_none(),
        "{}",
        out.data
    );
    assert!(
        octocode_runtime_base::test_support::checks::find_related_call(
            &out.data,
            "ghSearchCode",
            |_| true
        )
        .is_some(),
        "{}",
        out.data
    );
}

/// An empty search of a file's path lists the file's parent directory and
/// reads the file itself for the keywords: the index may not hold it.
#[tokio::test]
async fn a_file_path_scope_leads_to_filename_and_the_parent_listing() {
    let out = run_empty(json_fixture(
        r#"{
            "owner": "a",
            "repo": "b",
            "keywords": ["ppr"],
            "path": "packages/next/src/config-shared.ts"
        }"#,
    ))
    .await;
    let data = &out.data;

    let tree =
        &octocode_runtime_base::test_support::checks::related_call(data, "ghStructure", |input| {
            input["queries"][0]["operation"] == "tree"
        })["input"]["queries"][0];
    assert_eq!(tree["path"], "packages/next/src", "{data}");
    let read = &octocode_runtime_base::test_support::checks::related_call(
        data,
        "ghGetFileContent",
        |_| true,
    )["input"]["queries"][0];
    assert_eq!(read["path"], "packages/next/src/config-shared.ts", "{data}");
    assert_eq!(read["matchString"], json_fixture(r#"["ppr"]"#), "{data}");
    // A bare file name has the repository root as its parent.
    let out = run_empty(json_fixture(
        r#"{
            "owner": "a",
            "repo": "b",
            "keywords": ["x"],
            "path": "Cargo.toml"
        }"#,
    ))
    .await;
    assert_eq!(
        octocode_runtime_base::test_support::checks::related_call(
            &out.data,
            "ghStructure",
            |input| input["queries"][0]["operation"] == "tree"
        )["input"]["queries"][0]["path"],
        "",
        "{}",
        out.data
    );
    // A dotted directory is not a file.
    let out = run_empty(json_fixture(
        r#"{
            "owner": "a",
            "repo": "b",
            "keywords": ["x"],
            "path": "docs/v1.2"
        }"#,
    ))
    .await;
    assert!(out.data["next"].get("read").is_none(), "{}", out.data);
    let out = run_empty(json_fixture(
        r#"{
            "owner": "a",
            "repo": "b",
            "keywords": ["x"],
            "path": ".github"
        }"#,
    ))
    .await;
    assert!(out.data["next"].get("read").is_none(), "{}", out.data);
}

/// `path` names a file or a folder, so an empty search of either has no
/// repair hint, with or without a repository; the leads verify it.
#[tokio::test]
async fn an_empty_path_search_has_no_repair_hint() {
    for row in [
        json_fixture(
            r#"{
                "owner": "a",
                "keywords": ["ppr"],
                "path": "packages/next/src/config-shared.ts"
            }"#,
        ),
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["MIT"],
                "path": "LICENSE"
            }"#,
        ),
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["x"],
                "path": "packages/next/src"
            }"#,
        ),
    ] {
        let out = run_empty(row.clone()).await;
        let hints = out.data["hints"].to_string();
        assert!(
            !hints.contains("nameContains") && !hints.contains("directory"),
            "{row}: {}",
            out.data
        );
    }
}

/// A path-scoped empty states why it proves nothing in the default
/// hint, not only under `extras:["diagnostics"]`.
#[tokio::test]
async fn a_path_scoped_empty_says_why_it_proves_nothing() {
    let out = run_empty(json_fixture(
        r#"{
            "owner": "a",
            "repo": "b",
            "keywords": ["zzqq"],
            "path": "packages/next/src"
        }"#,
    ))
    .await;
    octocode_runtime_base::test_support::checks::assert_leads_valid(&out.data);
    assert!(
        octocode_runtime_base::test_support::checks::find_related_call(
            &out.data,
            "ghStructure",
            |_| true
        )
        .is_some()
    );
    // It names only tools every surface runs; the CLI also gets the
    // executable `cloneRepo` lead.

    // A requested ref is still named: the index covers only the default branch.
    let out = run_empty(json_fixture(
        r#"{
            "owner": "a",
            "repo": "b",
            "keywords": ["zzqq"],
            "ref": "v1",
            "path": "src"
        }"#,
    ))
    .await;
    octocode_runtime_base::test_support::checks::assert_leads_valid(&out.data);
    assert!(
        octocode_runtime_base::test_support::checks::find_related_call(
            &out.data,
            "ghStructure",
            |_| true
        )
        .is_some()
    );
}

/// A path-mode search whose keywords read as code leads to the same search
/// over file contents; that targeted lead is not pushed out of the two lead
/// slots by the generic clone and root listing of an empty repo search.
#[tokio::test]
async fn a_path_mode_search_of_code_leads_to_the_content_search_alone() {
    let out = run_empty(json_fixture(
        r#"{
            "owner": "a",
            "repo": "b",
            "keywords": ["res.send("],
            "matchIn": ["path"]
        }"#,
    ))
    .await;
    let next = octocode_runtime_base::test_support::checks::related_call(
        &out.data,
        "ghSearchCode",
        |input| {
            octocode_runtime_base::test_support::opened(
                octocode_runtime_base::id::ToolId::GhSearchCode,
                &input["queries"][0],
            )["matchIn"]
                == json_fixture(r#"["content"]"#)
        },
    );
    octocode_runtime_base::contracts::validate("ghSearchCode", next["input"].clone())
        .expect("a related content search must accept an authored input");
    assert_eq!(next["tool"], "ghSearchCode", "{}", out.data);
    assert_eq!(
        octocode_runtime_base::test_support::opened(
            octocode_runtime_base::id::ToolId::GhSearchCode,
            &next["input"]["queries"][0]
        )["matchIn"],
        json_fixture(r#"["content"]"#),
        "{}",
        out.data
    );
    // An empty path search leads to the content search alone.
    for other in ["ghCloneRepo", "ghStructure"] {
        assert!(
            octocode_runtime_base::test_support::checks::find_related_call(
                &out.data,
                other,
                |_| { true }
            )
            .is_none(),
            "{other}: {}",
            out.data
        );
    }
}

/// Keywords are ANDed literals: an item holding a standalone `OR` ran as one
/// phrase. An empty result says so once and leads to one row per
/// alternative; an `OR` inside a word or in lower case is ordinary text.
#[tokio::test]
async fn an_empty_search_of_an_or_phrase_splits_it_into_rows() {
    let out = run_empty(json_fixture(
        r#"{
            "owner": "a",
            "repo": "b",
            "keywords": ["SearcherBuilder OR Sink", "pub"]
        }"#,
    ))
    .await;
    let data = &out.data;
    assert!(
        data["hints"]
            .as_array()
            .is_some_and(|hints| hints.iter().any(|hint| hint
                .as_str()
                .is_some_and(|hint| hint.starts_with("keywords hold OR")))),
        "{data}"
    );
    let split =
        octocode_runtime_base::test_support::checks::related_call(data, "ghSearchCode", |input| {
            input["queries"].as_array().is_some_and(|rows| {
                rows.len() == 2
                    && rows[0]["keywords"] == json_fixture(r#"["SearcherBuilder", "pub"]"#)
            })
        });
    octocode_runtime_base::contracts::validate("ghSearchCode", split["input"].clone())
        .expect("each related alternative must accept an authored input");
    let rows = &split["input"]["queries"];
    assert_eq!(
        rows[0]["keywords"],
        json_fixture(r#"["SearcherBuilder", "pub"]"#),
        "{data}"
    );
    assert_eq!(
        rows[1]["keywords"],
        json_fixture(r#"["Sink", "pub"]"#),
        "{data}"
    );
    assert_eq!(rows[0]["repo"], "b", "{data}");
    for literal in ["ORDER BY", "a or b"] {
        let out = run_empty(json!({"owner":"a","repo":"b","keywords":[literal]})).await;
        assert!(out.data["next"].get("repair").is_none(), "{}", out.data);
    }
}

/// An empty search scoped to a missing repository reads its metadata once:
/// the rename check's 404 is the search's not-found failure, not read again
/// by the empty page's own check.
#[tokio::test]
async fn an_empty_search_of_a_missing_repository_reads_it_once() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{"total_count": 0, "incomplete_results": false, "items": []}"#,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/missing"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(json_fixture(
        r#"{"owner": "a", "repo": "missing", "keywords": ["x"]}"#,
    ))
    .expect("query");
    let error = execute(
        &provider,
        &query,
        &default_context(),
        &Passthrough,
        &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
    )
    .await
    .expect_err("a missing repository");
    assert_eq!(error.kind, octocode_github::ProviderErrorKind::NotFound);
    assert_eq!(
        error.reason,
        Some(octocode_github::ProviderErrorReason::RepositoryNotFound)
    );
}

/// A missing repository's name search returns repositories named like it
/// under any owner: the hint says to confirm the owner, and the owner's own
/// listing sits beside it.
#[tokio::test]
async fn a_missing_repository_labels_its_name_search_and_lists_the_owner() {
    let server = MockServer::builder().start().await;
    // The owner's login exists: only the repository is missing.
    wiremock::Mock::given(wiremock::matchers::path_regex("/users/[^/]+$"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"login": "owner"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{"total_count": 0, "incomplete_results": false, "items": []}"#,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/missing"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(json_fixture(
        r#"{"owner": "a", "repo": "missing", "keywords": ["x"]}"#,
    ))
    .expect("query");
    let context = default_context();
    let failure = crate::gh_search_code::run(
        &provider,
        &query,
        Ok(&context),
        &Passthrough,
        &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
        None,
    )
    .await
    .expect_err("a missing repository");
    let calls = failure.calls.as_deref().expect("recovery calls");
    assert_eq!(
        calls["findRepository"]["input"]["queries"][0],
        json!({"operation":"search","keywords":["missing"],
            "limit":octocode_tools_shared::gh_shared::RECOVERY_LIMIT}),
        "{calls}"
    );
    assert_eq!(
        calls["findRepository2"]["input"]["queries"][0],
        json!({"operation":"list","owner":"a","limit":octocode_tools_shared::gh_shared::RECOVERY_LIMIT}),
        "{calls}"
    );
    assert_eq!(
        failure.hints,
        [octocode_tools_shared::gh_shared::REPOSITORY_SEARCH_HINT.to_owned()]
    );
}

/// A scoped empty search does not restate the tool description ("Empty
/// never rules out an unindexed file: broaden, browse ghStructure, or
/// matchString"): neither its hints nor the diagnostics the response stage
/// appends say "unproven absence"; its related[] listing is the next step.
#[tokio::test]
async fn a_scoped_empty_search_does_not_restate_the_description() {
    let out = run_empty(json_fixture(
        r#"{
            "owner": "a",
            "repo": "b",
            "keywords": ["zzqq"],
            "path": "src"
        }"#,
    ))
    .await;
    let mut hints = out.data["hints"]
        .as_array()
        .map(|hints| {
            hints
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    hints.extend(out.diagnostics.hints.iter().map(String::as_str));
    assert!(
        !hints.iter().any(|hint| hint.contains("absence")),
        "{hints:?}"
    );
    assert!(
        octocode_runtime_base::test_support::checks::find_related_call(
            &out.data,
            "ghStructure",
            |input| { input["queries"][0]["operation"] == "tree" }
        )
        .is_some(),
        "{}",
        out.data
    );
}

/// GitHub's index holds nothing of an archived repository: its empty search
/// is partial coverage whose reason names the cause, never `complete`, and
/// its recovery is the root listing, with no hint restating either (nor the
/// default-branch note of a ref). A live repository's empty search says what
/// `complete` covers.
#[tokio::test]
async fn an_empty_search_of_an_archived_repo_proves_no_absence() {
    let row = json_fixture(r#"{"owner": "a", "repo": "b", "keywords": ["zzqq"]}"#);
    let mut at_ref = row.clone();
    at_ref["ref"] = serde_json::json!("v1");
    for asked in [row.clone(), at_ref] {
        let archived = run_empty_in(asked.clone(), true).await;
        assert_eq!(
            archived.data["coverage"],
            json_fixture(
                r#"{
                    "state": "partial",
                    "reason": "archived: GitHub's code search does not index archived repositories"
                }"#
            ),
            "{}",
            archived.data
        );
        assert!(archived.data.get("hints").is_none(), "{}", archived.data);
        assert!(archived.diagnostics.hints.is_empty(), "{}", archived.data);
        let listing = &octocode_runtime_base::test_support::checks::related_call(
            &archived.data,
            "ghStructure",
            |input| input["queries"][0]["operation"] == "tree",
        )["input"]["queries"][0];
        assert_eq!(listing["path"], "", "{}", archived.data);
        assert_eq!(listing["ref"], asked["ref"], "{}", archived.data);
    }
    let live = run_empty(row).await;
    assert_eq!(live.data["coverage"]["state"], "complete", "{}", live.data);
    assert!(
        live.data["coverage"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("does not index")),
        "{}",
        live.data
    );
}

/// A keyword spelled as a qualifier matches text; the empty answer names the
/// field that carries it.
#[tokio::test]
async fn an_empty_search_names_the_field_for_a_qualifier_keyword() {
    let output = run_empty(json_fixture(
        r#"{"owner": "a", "repo": "b", "keywords": ["language:rust", "parse"]}"#,
    ))
    .await;
    let hints = output.data["hints"].to_string();
    assert!(hints.contains("`language:rust`"), "{hints}");
    assert!(hints.contains("`extensions`"), "{hints}");
    assert!(!hints.contains("`parse`"), "{hints}");
}
