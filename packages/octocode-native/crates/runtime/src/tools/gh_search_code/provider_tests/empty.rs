//! Empty searches: their hints, the leads that verify absence, and their repairs.
use super::*;

/// The default empty-result hint fits the query: it names only tools every
/// surface runs (the CLI also gets the executable `cloneRepo` lead), a
/// repository it can list only when the search had one, and fewer keywords
/// only when there is one to drop.
#[test]
fn the_default_empty_hint_fits_the_query() {
    use crate::response::output::ToolOutput;
    let tools_named = |hint: &str| {
        let mut named = hint
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|word| word.starts_with("gh") && word.len() > 2)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        named.sort();
        named.dedup();
        named
    };
    for (query, tools, fewer) in [
        (
            json!({"owner":"a","keywords":["qzxNotARealSymbol"]}),
            vec!["ghSearchRepo", "ghStructure"],
            false,
        ),
        (
            json!({"owner":"a","repo":"r","keywords":["qzxNotARealSymbol"]}),
            vec!["ghGetFileContent", "ghStructure"],
            false,
        ),
        (
            json!({"owner":"a","keywords":["alpha","beta"]}),
            vec!["ghGetFileContent"],
            true,
        ),
    ] {
        let hint = super::super::Output.fallback_hint(&query);
        assert!(
            hint.starts_with("An empty index result is unproven absence"),
            "{hint}"
        );
        assert_eq!(tools_named(hint), tools, "{query}: {hint}");
        assert_eq!(hint.contains("fewer keywords"), fewer, "{query}: {hint}");
    }
}

/// An empty code search for `row` against `a/b`: no index hit, a live repo.
async fn run_empty(row: serde_json::Value) -> crate::response::result::ToolData {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count":0,"incomplete_results":false,"items":[]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"full_name":"a/b","default_branch":"main","archived":false})),
        )
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(row).expect("query");
    let context = default_context();
    execute(&provider, &query, &context, &Passthrough)
        .await
        .expect("empty search")
}

/// An empty repo-scoped search is unproven absence; the default hint says
/// so in plain words and `next.cloneRepo` is the runnable way to verify it,
/// scoped to the same ref as the search.
#[tokio::test]
async fn an_empty_repo_search_leads_to_a_runnable_clone() {
    let out = run_empty(json!({"owner":"a","repo":"b","keywords":["zzqq"]})).await;
    let clone = &out.data["next"]["cloneRepo"];
    assert_eq!(clone["tool"], "ghCloneRepo", "{}", out.data);
    assert_eq!(
        clone["query"]["queries"][0],
        json!({"owner":"a","repo":"b"}),
        "{}",
        out.data
    );
    let out = run_empty(json!({"owner":"a","repo":"b","keywords":["zzqq"],"ref":"v1"})).await;
    assert_eq!(
        out.data["next"]["cloneRepo"]["query"]["queries"][0],
        json!({"owner":"a","repo":"b","ref":"v1"}),
        "{}",
        out.data
    );
    // A path scope leads to its listing instead.
    let out = run_empty(json!({"owner":"a","repo":"b","keywords":["zzqq"],"path":"src"})).await;
    assert!(out.data["next"].get("cloneRepo").is_none(), "{}", out.data);
    assert!(out.data["next"].get("viewTree").is_some(), "{}", out.data);
}

/// `cloneRepo` runs only where clone is available, so an empty repo-wide
/// search also lists the root at the searched ref, a route every surface
/// runs. A search with a narrowing to drop leads to `broadenSearch`
/// instead.
#[tokio::test]
async fn an_empty_repo_search_also_lists_the_root_on_every_surface() {
    let out = run_empty(json!({"owner":"a","repo":"b","keywords":["zzqq"],"ref":"v1"})).await;
    let tree = &out.data["next"]["viewTree"];
    assert_eq!(tree["tool"], "ghStructure", "{}", out.data);
    assert_eq!(
        tree["query"]["queries"][0],
        json!({"owner":"a","repo":"b","ref":"v1"}),
        "{}",
        out.data
    );
    assert!(out.data["next"].get("cloneRepo").is_some(), "{}", out.data);
    let out = run_empty(json!({"owner":"a","repo":"b","keywords":["zzqq","yyrr"]})).await;
    assert!(out.data["next"].get("viewTree").is_none(), "{}", out.data);
    assert!(
        out.data["next"].get("broadenSearch").is_some(),
        "{}",
        out.data
    );
}

/// `path` is a directory prefix, so a file path matches nothing. The
/// empty names the mistake, lists the file's parent directory, and reads the
/// file itself for the keywords.
#[tokio::test]
async fn a_file_path_scope_leads_to_filename_and_the_parent_listing() {
    let out = run_empty(json!({
        "owner":"a","repo":"b","keywords":["ppr"],"path":"packages/next/src/config-shared.ts"
    }))
    .await;
    let data = &out.data;
    assert_eq!(
        data["hints"],
        json!([
            "`path` is a directory prefix and `packages/next/src/config-shared.ts` looks like a file: search it by `filename:\"config-shared.ts\"` with `path:\"packages/next/src\"`, or read it with matchString.",
            "broadenSearch rows: queries[0] drops path; queries[1] drops keywords"
        ]),
        "{data}"
    );
    let tree = &data["next"]["viewTree"]["query"]["queries"][0];
    assert_eq!(tree["path"], "packages/next/src", "{data}");
    let read = &data["next"]["read"]["query"]["queries"][0];
    assert_eq!(read["path"], "packages/next/src/config-shared.ts", "{data}");
    assert_eq!(read["matchString"], "ppr", "{data}");
    // A bare file name has the repository root as its parent.
    let out = run_empty(json!({"owner":"a","repo":"b","keywords":["x"],"path":"Cargo.toml"})).await;
    assert_eq!(
        out.data["next"]["viewTree"]["query"]["queries"][0]["path"], "",
        "{}",
        out.data
    );
    // A dotted directory is not a file.
    let out = run_empty(json!({"owner":"a","repo":"b","keywords":["x"],"path":"docs/v1.2"})).await;
    assert!(out.data["next"].get("read").is_none(), "{}", out.data);
    let out = run_empty(json!({"owner":"a","repo":"b","keywords":["x"],"path":".github"})).await;
    assert!(out.data["next"].get("read").is_none(), "{}", out.data);
}

/// A file `path` names its mistake whether or not a repository narrows the
/// search, and a name no extension rule recognises (`LICENSE`) still learns
/// that `path` is a directory prefix.
#[tokio::test]
async fn a_file_path_scope_is_named_without_a_repo_and_for_extensionless_names() {
    let owner_wide = run_empty(json!({
        "owner":"a","keywords":["ppr"],"path":"packages/next/src/config-shared.ts"
    }))
    .await;
    let hints = owner_wide.data["hints"].as_array().expect("hints");
    assert!(
        hints[0].as_str().is_some_and(|hint| hint.contains(
            "search it by `filename:\"config-shared.ts\"` with `path:\"packages/next/src\"`"
        )),
        "{}",
        owner_wide.data
    );
    let extensionless = run_empty(json!({
        "owner":"a","repo":"b","keywords":["MIT"],"path":"LICENSE"
    }))
    .await;
    assert!(
        extensionless.data["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("`filename`") && hint.contains("`path`")),
        "{}",
        extensionless.data
    );
}

/// A path-scoped empty states why it proves nothing in the default
/// hint, not only under `extras:["diagnostics"]`.
#[tokio::test]
async fn a_path_scoped_empty_says_why_it_proves_nothing() {
    let out = run_empty(json!({
        "owner":"a","repo":"b","keywords":["zzqq"],"path":"packages/next/src"
    }))
    .await;
    // It names only tools every surface runs; the CLI also gets the
    // executable `cloneRepo` lead.
    assert_eq!(
        out.data["hints"],
        json!([
            "No indexed match under `packages/next/src`: unproven absence; a file is `filename` + its parent as `path`; ghStructure lists the path.",
            "broadenSearch rows: queries[0] drops path; queries[1] drops keywords"
        ]),
        "{}",
        out.data
    );
    // A requested ref is still named: the index covers only the default branch.
    let out = run_empty(json!({
        "owner":"a","repo":"b","keywords":["zzqq"],"path":"src","ref":"v1"
    }))
    .await;
    assert_eq!(
        out.data["hints"],
        json!([
            "Code search indexes only the default branch; `v1` was not searched.",
            "No indexed match under `src`: unproven absence; a file is `filename` + its parent as `path`; ghStructure lists the path.",
            "broadenSearch rows: queries[0] drops path, ref; queries[1] drops keywords, ref"
        ]),
        "{}",
        out.data
    );
}

/// A path-mode search whose keywords read as code leads to the same search
/// over file contents; that targeted lead is not pushed out of the two lead
/// slots by the generic clone and root listing of an empty repo search.
#[tokio::test]
async fn a_path_mode_search_of_code_leads_to_the_content_search_alone() {
    let out =
        run_empty(json!({"owner":"a","repo":"b","keywords":["res.send("],"matchIn":"path"})).await;
    let next = &out.data["next"];
    assert_eq!(next["findCode"]["tool"], "ghSearchCode", "{}", out.data);
    assert_eq!(
        next["findCode"]["query"]["queries"][0]["matchIn"], "file",
        "{}",
        out.data
    );
    assert!(next.get("cloneRepo").is_none(), "{}", out.data);
    assert!(next.get("viewTree").is_none(), "{}", out.data);
}

/// Keywords are ANDed literals: an item holding a standalone `OR` ran as one
/// phrase. An empty result says so once and leads to one row per
/// alternative; an `OR` inside a word or in lower case is ordinary text.
#[tokio::test]
async fn an_empty_search_of_an_or_phrase_splits_it_into_rows() {
    let out = run_empty(json!({
        "owner":"a","repo":"b","keywords":["SearcherBuilder OR Sink","pub"]
    }))
    .await;
    let data = &out.data;
    assert!(
        data["hints"].as_array().is_some_and(|hints| hints
            .iter()
            .any(|hint| hint == "keywords take no OR; send one row per alternative.")),
        "{data}"
    );
    let rows = &data["next"]["repair"]["query"]["queries"];
    assert_eq!(data["next"]["repair"]["tool"], "ghSearchCode", "{data}");
    assert_eq!(
        rows[0]["keywords"],
        json!(["SearcherBuilder", "pub"]),
        "{data}"
    );
    assert_eq!(rows[1]["keywords"], json!(["Sink", "pub"]), "{data}");
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
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count":0,"incomplete_results":false,"items":[]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .expect(1)
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"missing","keywords":["x"]}))
            .expect("query");
    let error = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect_err("a missing repository");
    assert_eq!(error.kind, octocode_github::ProviderErrorKind::NotFound);
    assert_eq!(
        error.reason,
        Some(octocode_github::ProviderErrorReason::RepositoryNotFound)
    );
}
