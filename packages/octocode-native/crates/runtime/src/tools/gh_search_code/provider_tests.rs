//! Provider-backed ghSearchCode tests: line resolution latency.
use super::{GhSearchCodeQuery, execute};
use crate::security::scan::Passthrough;
use crate::tools::gh_shared::test_support::{MemoryCache, default_context, one_shot_provider};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_github::mock_http::Delayed;
use serde_json::json;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// A ref search's commit, which its hit lines are pinned to, resolves while
/// the index search runs: one round trip, not two in series, before the
/// file reads.
#[tokio::test]
async fn the_ref_commit_resolves_while_the_search_runs() {
    let server = MockServer::builder().start().await;
    let delay = Duration::from_millis(300);
    let search = Delayed::new(
        delay,
        ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":"fn wrap_app() {}","matches":[{"text":"wrap_app","indices":[3,11]}]}]}
            ]
        })),
    );
    let commit = Delayed::new(delay, ResponseTemplate::new(200).set_body_string(SHA));
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(search.clone())
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/dev"))
        .respond_with(commit.clone())
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/app.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("fn wrap_app() {}\n")
        })))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","ref":"dev","keywords":["wrap_app"]}))
            .expect("query");
    let context = default_context();
    let out = execute(&provider, &query, &context, &Passthrough)
        .await
        .expect("search");
    assert_eq!(out.data["commitSha"], SHA, "{}", out.data);
    // The page's core-quota cost: one contents read (an `extras:["stats"]` field).
    assert_eq!(out.data["contentReads"], 1, "{}", out.data);
    assert_eq!(
        out.data["files"][0]["lines"],
        json!(["1\tfn wrap_app() {}"])
    );
    let first = |slow: &Delayed| *slow.arrivals.times().first().expect("requested");
    let (searched, resolved) = (first(&search), first(&commit));
    let gap = if resolved > searched {
        resolved - searched
    } else {
        searched - resolved
    };
    assert!(
        gap < delay / 2,
        "the commit lookup waited {gap:?} for the search"
    );
}

/// A default-branch search reads each hit at the commit the index matched
/// (its `html_url`), so its lines are the text the index searched, with no
/// commit lookup; files whose index blob is the same are read once.
#[tokio::test]
async fn default_branch_lines_are_read_at_the_indexed_commit() {
    let server = MockServer::builder().start().await;
    let item = |name: &str, blob: &str| {
        json!({"name":name,"path":format!("src/{name}"),"sha":blob,
            "html_url":format!("https://github.com/a/b/blob/{SHA}/src/{name}"),
            "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":3,"incomplete_results":false,"items":[
                item("one.rs", "b1"), item("copy.rs", "b1"), item("two.rs", "b2")
            ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("/commits/"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    for (name, body, reads) in [
        ("one.rs", "fn wrap_app() {}\n", 1),
        ("copy.rs", "fn wrap_app() {}\n", 0),
        ("two.rs", "let x = wrap_app();\n", 1),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/src%2F{name}")))
            .and(wiremock::matchers::query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode(body)
            })))
            .expect(reads)
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["wrap_app"]}))
            .expect("query");
    let out = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("search");
    let data = &out.data;
    assert_eq!(data["commitSha"], SHA, "{data}");
    assert_eq!(data["contentReads"], 2, "{data}");
    let listed = data["files"]
        .as_array()
        .expect("files")
        .iter()
        .flat_map(|file| {
            std::iter::once(file["path"].clone())
                .chain(file["alsoAt"].as_array().cloned().unwrap_or_default())
        })
        .collect::<Vec<_>>();
    assert_eq!(listed.len(), 3, "every hit is listed once: {data}");
    assert!(
        data["files"]
            .as_array()
            .expect("files")
            .iter()
            .all(|file| file.get("lines").is_some()),
        "{data}"
    );
}

/// A path-only search, or a `fields` pick without `lines`, reads no lines,
/// so it resolves no commit.
#[tokio::test]
async fn searches_without_line_reads_resolve_no_commit() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}}
            ]
        })))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let context = default_context();
    for row in [
        json!({"owner":"a","repo":"b","keywords":["app"],"matchIn":"path"}),
        json!({"owner":"a","repo":"b","keywords":["app"],"fields":["path"]}),
    ] {
        let query: GhSearchCodeQuery = serde_json::from_value(row).expect("query");
        execute(&provider, &query, &context, &Passthrough)
            .await
            .expect("search");
    }
    let requests = server.received_requests().await.expect("requests");
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() == "/api/v3/search/code"),
        "{:?}",
        requests.iter().map(|r| r.url.path()).collect::<Vec<_>>()
    );
}

/// A path-only page reads no lines, yet names the commit the index matched
/// (from each hit's `html_url`), so a read of a listed path can be pinned;
/// rows indexed at different commits each name theirs.
#[tokio::test]
async fn path_only_rows_name_their_indexed_commit() {
    let other = "89abcdef0123456789abcdef0123456789abcdef";
    let server = MockServer::builder().start().await;
    let item = |name: &str, sha: &str| {
        json!({"name":name,"path":name,"sha":"1",
            "html_url":format!("https://github.com/a/b/blob/{sha}/{name}"),
            "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(wiremock::matchers::query_param("q", "app repo:a/b in:path"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":2,"incomplete_results":false,
            "items":[item("app.rs", SHA), item("app_test.rs", SHA)]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(wiremock::matchers::query_param("q", "lib repo:a/b in:path"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":2,"incomplete_results":false,
            "items":[item("lib.rs", SHA), item("lib_test.rs", other)]
        })))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let context = default_context();
    let search = |keyword: &str| -> GhSearchCodeQuery {
        serde_json::from_value(
            json!({"owner":"a","repo":"b","keywords":[keyword],"matchIn":"path"}),
        )
        .expect("query")
    };
    let shared = execute(&provider, &search("app"), &context, &Passthrough)
        .await
        .expect("search");
    assert_eq!(shared.data["commitSha"], SHA, "{}", shared.data);
    let mixed = execute(&provider, &search("lib"), &context, &Passthrough)
        .await
        .expect("search");
    assert!(mixed.data.get("commitSha").is_none(), "{}", mixed.data);
    let commits = mixed.data["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|row| {
            (
                row["path"].as_str().unwrap_or_default(),
                row["commitSha"].as_str().unwrap_or_default(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(commits["lib.rs"], SHA, "{}", mixed.data);
    assert_eq!(commits["lib_test.rs"], other, "{}", mixed.data);
}

/// Two owner-wide hits in different repositories, indexed at `SHA`.
async fn mount_owner_wide_search(server: &MockServer) {
    let item = |repo: &str| {
        json!({"name":"x.py","path":"src/x.py","sha":"1",
         "html_url": format!("https://github.com/o/{repo}/blob/{SHA}/src/x.py"),
         "repository":{"full_name":format!("o/{repo}"),"html_url":"https://x","url":"https://x"},
         "text_matches":[{"fragment":"class HTTPAdapter(BaseAdapter):\n    pass","matches":[{"text":"HTTPAdapter","indices":[6,17]}]}]})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":2,"incomplete_results":false,"items":[item("a"), item("b")]
        })))
        .mount(server)
        .await;
}

async fn run_owner_wide(server: &MockServer) -> crate::response::result::ToolData {
    let provider = one_shot_provider(server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"o","keywords":["HTTPAdapter"]})).expect("query");
    let context = default_context();
    execute(&provider, &query, &context, &Passthrough)
        .await
        .expect("search")
}

/// Owner-wide rows are read at the commit each hit was indexed at
/// (`html_url`): numbered `lines` replace fragments, and the top read is
/// pinned to that commit instead of a `contextLines` fragment read.
#[tokio::test]
async fn owner_wide_rows_list_numbered_lines_at_the_indexed_commit() {
    let server = MockServer::builder().start().await;
    mount_owner_wide_search(&server).await;
    for repo in ["a", "b"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/o/{repo}/contents/src%2Fx.py")))
            .and(wiremock::matchers::query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64",
                "content":STANDARD.encode("import x\n\nclass HTTPAdapter(BaseAdapter):\n    pass\n")
            })))
            .expect(1)
            .mount(&server)
            .await;
    }
    let out = run_owner_wide(&server).await;
    let files = out.data["files"].as_array().expect("files");
    assert_eq!(files.len(), 2, "{}", out.data);
    for row in files {
        assert_eq!(
            row["lines"],
            json!(["3\tclass HTTPAdapter(BaseAdapter):"]),
            "{}",
            out.data
        );
        assert!(row.get("matches").is_none(), "{}", out.data);
        assert!(row["repo"].is_string(), "{}", out.data);
        assert!(row.get("owner").is_none(), "{}", out.data);
    }
    // What every row shares (owner, one commit) is stated once.
    assert_eq!(out.data["owner"], "o", "{}", out.data);
    assert_eq!(out.data["commitSha"], SHA, "{}", out.data);
    let read = &out.data["next"]["read"]["query"]["queries"][0];
    assert_eq!(read["ref"], SHA, "{}", out.data);
    assert_eq!(read["owner"], "o", "{}", out.data);
    assert_eq!(read["repo"], "a", "{}", out.data);
    assert!(read.get("contextLines").is_none(), "{}", out.data);
}

/// A row whose file is gone at its indexed commit keeps its fragment
/// and says its lines were not read.
#[tokio::test]
async fn owner_wide_row_missing_at_its_commit_keeps_fragments() {
    let server = MockServer::builder().start().await;
    mount_owner_wide_search(&server).await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            "^/api/v3/repos/o/[ab]/contents/",
        ))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    let out = run_owner_wide(&server).await;
    let row = &out.data["files"][0];
    assert_eq!(row["lineResolved"], false, "{}", out.data);
    assert!(row["matches"][0]["value"].is_string(), "{}", out.data);
    assert!(row.get("lines").is_none(), "{}", out.data);
}

/// Anchors: a repo-scoped page states the commit its lines were read at once
/// (`commitSha`); each file row is its `path` and numbered `lines`
/// ("N\ttext"), with no owner/repo/ref echo of the query.
#[tokio::test]
async fn a_repo_scoped_row_is_path_and_numbered_lines_at_one_commit() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"src/app.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":"fn wrap_app() {}","matches":[{"text":"wrap_app","indices":[3,11]}]}]}
            ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fapp.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64",
            "content":STANDARD.encode("// app\nfn wrap_app() {}\n")
        })))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["wrap_app"]}))
            .expect("query");
    let context = default_context();
    let out = execute(&provider, &query, &context, &Passthrough)
        .await
        .expect("search");
    assert_eq!(out.data["commitSha"], SHA, "{}", out.data);
    assert_eq!(
        out.data["files"],
        json!([{"path":"src/app.rs","lines":["2\tfn wrap_app() {}"]}]),
        "{}",
        out.data
    );
    for echo in ["owner", "repo", "ref", "indexRef", "keywords"] {
        assert!(out.data.get(echo).is_none(), "{echo}: {}", out.data);
    }
}

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
        let hint = super::Output.fallback_hint(&query);
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
            "`path` is a directory prefix and `packages/next/src/config-shared.ts` looks like a file: search it by `filename:\"config-shared.ts\"` with `path:\"packages/next/src\"`, or read it with matchString."
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
        hints.len() == 1
            && hints[0].as_str().is_some_and(|hint| hint.contains(
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
            "No indexed match under `packages/next/src`: unproven absence; a file is `filename` + its parent as `path`; ghStructure lists the path."
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
            "No indexed match under `src`: unproven absence; a file is `filename` + its parent as `path`; ghStructure lists the path."
        ]),
        "{}",
        out.data
    );
}

/// A single complete page whose file lists fewer hit lines than it has
/// is partial because of those unlisted lines, and the row says so; its
/// `next.readHits` reaches them. A page with another page behind it states
/// its remainder through `pagination` instead.
#[tokio::test]
async fn a_complete_page_with_unlisted_hit_lines_names_why_it_is_partial() {
    let server = MockServer::builder().start().await;
    let item = |name: &str| {
        json!({"name":name,"path":format!("src/{name}"),"sha":"1","html_url":"https://x",
            "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":2,"incomplete_results":false,"items":[item("busy.rs"), item("calm.rs")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    for (name, body) in [
        ("busy.rs", "wrap_app();\n".repeat(8)),
        ("calm.rs", "fn wrap_app() {}\n".to_owned()),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/src%2F{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode(body)
            })))
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let search = |keywords: serde_json::Value| {
        serde_json::from_value::<GhSearchCodeQuery>(
            json!({"owner":"a","repo":"b","keywords":keywords}),
        )
        .expect("query")
    };
    let context = default_context();
    let out = execute(
        &provider,
        &search(json!(["wrap_app"])),
        &context,
        &Passthrough,
    )
    .await
    .expect("search");
    assert_eq!(out.data["isPartial"], true, "{}", out.data);
    assert_eq!(
        out.data["partialReasons"],
        json!(["hitLinesNotListed"]),
        "{}",
        out.data
    );
    assert!(out.data["next"]["readHits"].is_object(), "{}", out.data);
}

/// `pageSize` counts files: a page of two files states the search's file
/// total in its pagination, a file with more keyword lines than the five it
/// lists states its `matchCount`, and `next.readHits` reads the rest of
/// exactly that file; a file whose hits all show states no count.
#[tokio::test]
async fn a_page_counts_files_and_a_file_with_more_hits_says_how_many_and_leads_to_them() {
    let server = MockServer::builder().start().await;
    let item = |name: &str| {
        json!({"name":name,"path":format!("src/{name}"),"sha":"1","html_url":"https://x",
            "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":40,"incomplete_results":false,"items":[item("busy.rs"), item("calm.rs")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    for (name, body) in [
        ("busy.rs", "wrap_app();\n".repeat(8)),
        ("calm.rs", "fn wrap_app() {}\n".to_owned()),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/src%2F{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode(body)
            })))
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(
        json!({"owner":"a","repo":"b","keywords":["wrap_app"],"pageSize":2}),
    )
    .expect("query");
    let context = default_context();
    let out = execute(&provider, &query, &context, &Passthrough)
        .await
        .expect("search");
    let (busy, calm) = (&out.data["files"][0], &out.data["files"][1]);
    assert_eq!(
        busy["lines"].as_array().map(Vec::len),
        Some(5),
        "{}",
        out.data
    );
    assert_eq!(busy["matchCount"], 8, "{}", out.data);
    assert!(calm.get("matchCount").is_none(), "{}", out.data);
    // The page's total counts files (GitHub's `total_count`), not lines.
    assert_eq!(out.data["pagination"]["totalItems"], 40, "{}", out.data);
    assert_eq!(out.data["pagination"]["pageSize"], 2, "{}", out.data);
    let reads = &out.data["next"]["readHits"]["query"]["queries"];
    assert_eq!(reads.as_array().map(Vec::len), Some(1), "{}", out.data);
    assert_eq!(reads[0]["path"], "src/busy.rs", "{}", out.data);
    assert_eq!(reads[0]["ref"], SHA, "{}", out.data);
    // `ranges` are 1-based line numbers, never byte offsets: from the line
    // after the last shown hit (5) to the file's last line (8), so the read
    // returns exactly the hits the listing did not (lines 6-8).
    assert_eq!(reads[0]["ranges"], json!(["6-8"]), "{}", out.data);
    assert_eq!(reads[0]["matchString"], "wrap_app", "{}", out.data);
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

/// A search of `pages` total results at 20 a page: GitHub answers every
/// page with the same `total_count`.
async fn mount_counted_search(server: &MockServer, total: usize) {
    let item = |name: &str| {
        json!({"name":name,"path":format!("src/{name}"),"sha":"1","html_url":"https://x",
            "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":total,"incomplete_results":false,"items":[item("busy.rs"), item("calm.rs")]
        })))
        .mount(server)
        .await;
}

/// A page past GitHub's 1,000-result window repairs to the search's last
/// real page, not to the window's last page (an empty page past the end).
#[tokio::test]
async fn a_page_past_the_window_repairs_to_the_last_real_page() {
    let server = MockServer::builder().start().await;
    mount_counted_search(&server, 150).await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"a","repo":"b","keywords":["needle"],"pageSize":20,"page":60,"fields":["path"]
    }))
    .expect("query");
    let context = default_context();
    let Err(failure) = super::run(&provider, &query, Ok(&context), &Passthrough).await else {
        panic!("a page past the window is a failure");
    };
    let next = failure.next.expect("repair");
    assert_eq!(next["repair"]["query"]["queries"][0]["page"], 8, "{next}");
}

/// Code search serves a page only while its last result lies inside the
/// 1,000-result window: at 3 a page the window ends at page 333 (999
/// results), which is the terminal page, and page 334 repairs to it.
#[tokio::test]
async fn the_code_search_window_ends_at_its_last_whole_page() {
    let server = MockServer::builder().start().await;
    mount_counted_search(&server, 2236).await;
    let provider = one_shot_provider(&server);
    let at = |page: usize| -> GhSearchCodeQuery {
        serde_json::from_value(json!({
            "owner":"a","repo":"b","keywords":["needle"],"pageSize":3,"page":page,"fields":["path"]
        }))
        .expect("query")
    };
    let context = default_context();
    let first = execute(&provider, &at(1), &context, &Passthrough)
        .await
        .expect("first page");
    assert_eq!(
        first.data["pagination"]["totalItems"], 999,
        "{}",
        first.data
    );
    assert_eq!(
        first.data["pagination"]["totalPages"], 333,
        "{}",
        first.data
    );
    let last = execute(&provider, &at(333), &context, &Passthrough)
        .await
        .expect("last page");
    assert!(last.data["next"]["nextPage"].is_null(), "{}", last.data);
    assert_eq!(last.data["terminalLimit"], true, "{}", last.data);
    let Err(failure) = super::run(&provider, &at(334), Ok(&context), &Passthrough).await else {
        panic!("a page past the window is a failure");
    };
    let next = failure.next.expect("repair");
    assert_eq!(next["repair"]["query"]["queries"][0]["page"], 333, "{next}");
}

/// A merged multi-`extensions` page is page N of each extension's own
/// search of `ceil(pageSize / k)` files, so the window is that search's: at
/// 50 files over 3 extensions (17 each) page 58 is the last, and page 59
/// repairs to it. The error states the caller's page size and the merged
/// reachable total (3 × 986), as page 1 does, never the per-search size.
#[tokio::test]
async fn a_multi_extension_window_is_each_searchs_own() {
    let server = MockServer::builder().start().await;
    mount_counted_search(&server, 5000).await;
    let provider = one_shot_provider(&server);
    let at = |page: usize| -> GhSearchCodeQuery {
        serde_json::from_value(json!({
            "owner":"a","repo":"b","keywords":["needle"],"extensions":["rs","ts","go"],
            "pageSize":50,"page":page,"fields":["path"]
        }))
        .expect("query")
    };
    let context = default_context();
    execute(&provider, &at(58), &context, &Passthrough)
        .await
        .expect("the last page");
    let Err(failure) = super::run(&provider, &at(59), Ok(&context), &Passthrough).await else {
        panic!("a page past the window is a failure");
    };
    assert_eq!(
        failure.message,
        "Page 59 is past GitHub's search window: at pageSize 50, page 58 is the last, reaching the first 2958 results."
    );
    let next = failure.next.expect("repair");
    assert_eq!(next["repair"]["query"]["queries"][0]["page"], 58, "{next}");
}

/// `fields:["path","matchCount"]` states each file's hit count without
/// listing its lines, and no warning says the count is missing.
#[tokio::test]
async fn a_match_count_pick_counts_every_file_without_lines() {
    let server = MockServer::builder().start().await;
    mount_counted_search(&server, 2).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    for (name, body) in [
        ("busy.rs", "wrap_app();\n".repeat(8)),
        ("calm.rs", "fn wrap_app() {}\n".to_owned()),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/src%2F{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode(body)
            })))
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"a","repo":"b","keywords":["wrap_app"],"fields":["path","matchCount"]
    }))
    .expect("query");
    let context = default_context();
    let out = execute(&provider, &query, &context, &Passthrough)
        .await
        .expect("search");
    let files = &out.data["files"];
    assert_eq!(files[0]["path"], "src/busy.rs", "{}", out.data);
    assert_eq!(files[0]["matchCount"], 8, "{}", out.data);
    assert_eq!(files[1]["matchCount"], 1, "{}", out.data);
    assert!(files[0].get("lines").is_none(), "{}", out.data);
    assert!(out.data.get("warnings").is_none(), "{}", out.data);
}

/// A search page of `names` (paths in `a/b`), GitHub reporting `total` matches.
async fn mount_search_page(server: &MockServer, page: usize, total: usize, names: &[&str]) {
    let items = names
        .iter()
        .map(|name| {
            json!({"name":name,"path":name,"sha":"1","html_url":"https://x",
             "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(wiremock::matchers::query_param("page", page.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":total,"incomplete_results":false,"items":items
        })))
        .mount(server)
        .await;
}

async fn search_page_of(
    provider: &octocode_github::GitHubProvider,
    page: usize,
) -> crate::response::result::ToolData {
    let query: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"a","repo":"b","keywords":["needle"],"matchIn":"path","pageSize":2,"page":page
    }))
    .expect("query");
    execute(provider, &query, &default_context(), &Passthrough)
        .await
        .expect("search")
}

fn paths_of(out: &crate::response::result::ToolData) -> Vec<String> {
    out.data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["path"].as_str().unwrap_or_default().to_owned())
        .collect()
}

fn warnings_of(out: &crate::response::result::ToolData) -> &serde_json::Value {
    &out.data["warnings"]
}

/// A paged search page states the repeats it found: a later page drops a
/// file an earlier cached page listed, and counts the earlier pages it could
/// check. An uncached earlier page keeps its repeats listed and adds no note
/// (a jump to a deep page dropped nothing), nor does a clean first page:
/// that totals are estimates is static and lives in the tool description.
#[tokio::test]
async fn paged_pages_state_estimates_and_the_repeats_they_dropped() {
    let checked =
        "Repeat check read 1 of 1 earlier page; pages no longer cached were not compared.";
    let dropped = "Dropped 1 file GitHub listed again (already on an earlier page or this one).";
    for (cached, page, paths, warnings) in [
        (true, 1, vec!["a.rs", "b.rs"], serde_json::Value::Null),
        (true, 2, vec!["c.rs"], json!([dropped, checked])),
        (false, 2, vec!["b.rs", "c.rs"], serde_json::Value::Null),
    ] {
        let server = MockServer::builder().start().await;
        mount_search_page(&server, 1, 4, &["a.rs", "b.rs"]).await;
        mount_search_page(&server, 2, 4, &["b.rs", "c.rs"]).await;
        let mut provider = one_shot_provider(&server);
        if cached {
            provider.transport.cache = std::sync::Arc::new(MemoryCache::default());
        }
        let mut out = search_page_of(&provider, 1).await;
        if page == 2 {
            out = search_page_of(&provider, 2).await;
        }
        assert_eq!(out.data["pagination"]["totalItems"], 4, "{}", out.data);
        assert_eq!(paths_of(&out), paths, "cached {cached}: {}", out.data);
        assert_eq!(
            warnings_of(&out),
            &warnings,
            "cached {cached}: {}",
            out.data
        );
    }
}

/// A page made only of repeats is not an empty search: it keeps its
/// pagination and `next.nextPage`, and claims no absence.
#[tokio::test]
async fn a_page_of_only_repeats_is_not_an_empty_search() {
    let server = MockServer::builder().start().await;
    mount_search_page(&server, 1, 6, &["a.rs", "b.rs"]).await;
    mount_search_page(&server, 2, 6, &["a.rs", "b.rs"]).await;
    let mut provider = one_shot_provider(&server);
    provider.transport.cache = std::sync::Arc::new(MemoryCache::default());
    search_page_of(&provider, 1).await;
    let second = search_page_of(&provider, 2).await;
    assert!(paths_of(&second).is_empty(), "{}", second.data);
    assert_ne!(second.status, Some("empty"), "{}", second.data);
    assert_eq!(
        second.data["pagination"]["hasMore"], true,
        "{}",
        second.data
    );
    assert!(
        second.data["next"]["nextPage"].is_object(),
        "{}",
        second.data
    );
    assert_eq!(
        warnings_of(&second),
        &json!([
            "Dropped 2 files GitHub listed again (already on an earlier page or this one).",
            "Repeat check read 1 of 1 earlier page; pages no longer cached were not compared."
        ]),
        "{}",
        second.data
    );
    let requests = server.received_requests().await.expect("requests");
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() == "/api/v3/search/code"),
        "{:?}",
        requests.iter().map(|r| r.url.path()).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn empty_index_pages_keep_the_collection_when_partial_normalization_answers() {
    for incomplete in [false, true] {
        for fields in [json!([]), json!(["path"])] {
            let server = MockServer::builder().start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/search/code"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "total_count": 0, "incomplete_results": incomplete, "items": []
                })))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/commits/HEAD"))
                .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "full_name": "a/b", "default_branch": "main", "archived": false
                })))
                .mount(&server)
                .await;
            let mut row = json!({"owner": "a", "repo": "b", "keywords": ["marker"]});
            if fields != json!([]) {
                row["fields"] = fields;
            }
            let query: GhSearchCodeQuery = serde_json::from_value(row.clone()).expect("query");
            let provider = one_shot_provider(&server);
            let context = default_context();
            let out = execute(&provider, &query, &context, &Passthrough)
                .await
                .expect("provider search");
            if incomplete {
                assert_eq!(out.data["files"], json!([]), "{}", out.data);
                assert_eq!(out.data["isPartial"], true, "{}", out.data);
                assert!(
                    out.data["partialReasons"]
                        .as_array()
                        .expect("reasons")
                        .contains(&json!("providerIncompleteResults")),
                    "{}",
                    out.data
                );
                let retry = &out.data["next"]["retry"];
                assert_eq!(retry["tool"], "ghSearchCode", "{retry}");
                let prepared = crate::contracts::validate("ghSearchCode", retry["query"].clone())
                    .expect("executable retry");
                let continued = &prepared["queries"][0];
                for field in ["owner", "repo", "keywords", "fields"] {
                    assert_eq!(
                        continued[field], row[field],
                        "retry changed {field}: {retry}"
                    );
                }
                assert_eq!(continued["page"], 1, "{retry}");
            } else {
                assert_eq!(out.status, Some("empty"), "{}", out.data);
                assert!(out.data["next"].get("retry").is_none(), "{}", out.data);
                assert_ne!(out.data["isPartial"], true, "{}", out.data);
            }
            let shaped = crate::response::rows::result_row(
                crate::id::ToolId::GhSearchCode,
                0,
                &row,
                out.data,
                out.status,
            );
            assert_eq!(shaped["status"] == "empty", !incomplete, "{shaped}");
            crate::contracts::validate_output("ghSearchCode", &json!({"results": [shaped]}))
                .expect("the actual normalized row satisfies the canonical output contract");
        }
    }
}

/// `ref` re-reads one repository's hits: without `repo` it is rejected as
/// invalid input before any request, and the repair is the same search
/// without it.
#[tokio::test]
async fn a_ref_without_a_repo_is_invalid_input_repaired_without_it() {
    let server = MockServer::builder().start().await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"o","keywords":["needle"],"ref":"dev","pageSize":5
    }))
    .expect("query");
    let context = default_context();
    let Err(failure) = super::run(&provider, &query, Ok(&context), &Passthrough).await else {
        panic!("a ref without a repo is rejected");
    };
    assert_eq!(
        failure.error.kind,
        octocode_github::ProviderErrorKind::Validation
    );
    assert!(
        failure.message.contains("needs repo"),
        "{}",
        failure.message
    );
    let next = failure.next.expect("repair");
    let repair = &next["repair"]["query"]["queries"][0];
    assert!(repair.get("ref").is_none(), "{next}");
    assert_eq!(repair["owner"], "o", "{next}");
    assert_eq!(repair["keywords"], json!(["needle"]), "{next}");
    assert_eq!(repair["pageSize"], 5, "{next}");
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

/// `filename:` matches names containing its text, never a glob: a glob is
/// invalid input before any request. One whose only wildcards are leading
/// or trailing `*` repairs to its text; any other names the rule.
#[tokio::test]
async fn a_filename_glob_is_refused_before_any_request() {
    let server = MockServer::builder().start().await;
    let provider = one_shot_provider(&server);
    let context = default_context();
    async fn refused(
        provider: &octocode_github::GitHubProvider,
        context: &octocode_github::RequestContext,
        filename: &str,
    ) -> crate::tools::gh_shared::GhFailure {
        let query: GhSearchCodeQuery = serde_json::from_value(json!({
            "owner":"o","repo":"r","keywords":["describe"],"filename":filename
        }))
        .expect("query");
        match super::run(provider, &query, Ok(context), &Passthrough).await {
            Err(failure) => failure,
            Ok(_) => panic!("{filename}: a glob filename is refused"),
        }
    }
    for (glob, text) in [
        ("*.test.js", ".test.js"),
        ("README*", "README"),
        ("*util*", "util"),
    ] {
        let failure = refused(&provider, &context, glob).await;
        assert_eq!(
            failure.error.kind,
            octocode_github::ProviderErrorKind::Validation
        );
        assert!(failure.message.contains(glob), "{}", failure.message);
        let next = failure.next.expect("repair");
        let repair = &next["repair"]["query"]["queries"][0];
        assert_eq!(repair["filename"], text, "{next}");
        assert_eq!(repair["keywords"], json!(["describe"]), "{next}");
    }
    for glob in ["a*b.js", "*.{js,ts}", "*"] {
        let failure = refused(&provider, &context, glob).await;
        assert!(failure.next.is_none(), "{glob}");
        assert!(
            failure.message.contains("names containing its text"),
            "{}",
            failure.message
        );
    }
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

/// Identical searches in one batch share one request: the second waits for
/// the first and reads the page it stored, so the duplicate spends no
/// code-search quota.
#[tokio::test]
async fn identical_concurrent_searches_send_one_request() {
    let server = MockServer::builder().start().await;
    let search = Delayed::new(
        Duration::from_millis(200),
        ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}}
            ]
        })),
    );
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(search.clone())
        .mount(&server)
        .await;
    let mut provider = one_shot_provider(&server);
    provider.transport.cache = std::sync::Arc::new(MemoryCache::default());
    let query: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"a","repo":"b","keywords":["app"],"matchIn":"path"
    }))
    .expect("query");
    let context = default_context();
    let (first, second) = futures_util::future::join(
        execute(&provider, &query, &context, &Passthrough),
        execute(&provider, &query, &context, &Passthrough),
    )
    .await;
    assert_eq!(
        first.expect("first").data["files"],
        second.expect("second").data["files"]
    );
    assert_eq!(search.arrivals.count(), 1, "one GET /search/code");
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

/// Every layout of merged searches lists each GitHub page once: page 1 of
/// each search, then the later pages `k` to a merged page, every merged
/// page full but the last.
#[test]
fn merged_pages_list_every_github_page_once_and_fill_each_page() {
    for pages in [
        vec![4, 0],
        vec![0, 4],
        vec![1, 5, 2],
        vec![3, 3, 3],
        vec![58, 58, 58],
        vec![0, 0],
        vec![7],
        vec![2, 0, 0, 9, 1],
    ] {
        let k = pages.len();
        let total = super::merged_pages(&pages);
        let mut listed: Vec<(usize, usize)> = (0..k).map(|search| (search, 1)).collect();
        listed.retain(|&(search, _)| pages[search] > 0);
        for current in 2..=total {
            let units = super::later_pages(&pages, current);
            assert!(!units.is_empty(), "{pages:?} page {current}");
            if current < total {
                assert_eq!(units.len(), k, "{pages:?} page {current} is full");
            }
            listed.extend(units);
        }
        assert!(
            super::later_pages(&pages, total.max(1) + 1).is_empty(),
            "{pages:?}"
        );
        let mut expected: Vec<(usize, usize)> = pages
            .iter()
            .enumerate()
            .flat_map(|(search, &count)| (1..=count).map(move |page| (search, page)))
            .collect();
        let mut sorted = listed.clone();
        sorted.sort_unstable();
        expected.sort_unstable();
        assert_eq!(sorted, expected, "{pages:?}: every page once");
    }
}

/// A multi-extension walk never asks an exhausted extension again and
/// fills its pages from the live one: 12 `.ts` files and no `.rs` at
/// pageSize 6 list 3 + 6 + 3 files over 3 pages, every file once, in 5
/// searches (4 for `.ts`, 1 for `.rs`) instead of 8.
#[tokio::test]
async fn a_multi_extension_walk_skips_exhausted_searches_and_fills_its_pages() {
    let server = MockServer::builder().start().await;
    for page in 1..=4 {
        let items = (0..3)
            .map(|file| {
                let name = format!("p{page}_{file}.ts");
                json!({"name":name,"path":name,"sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
            })
            .collect::<Vec<_>>();
        Mock::given(method("GET"))
            .and(path("/api/v3/search/code"))
            .and(wiremock::matchers::query_param_contains(
                "q",
                "extension:ts",
            ))
            .and(wiremock::matchers::query_param("page", page.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count":12,"incomplete_results":false,"items":items
            })))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(wiremock::matchers::query_param_contains(
            "q",
            "extension:rs",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count":0,"incomplete_results":false,"items":[]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let mut provider = one_shot_provider(&server);
    provider.transport.cache = std::sync::Arc::new(MemoryCache::default());
    let mut sizes = Vec::new();
    let mut listed = Vec::new();
    for page in 1.. {
        let query: GhSearchCodeQuery = serde_json::from_value(json!({
            "owner":"a","repo":"b","keywords":["needle"],"matchIn":"path",
            "extensions":["ts","rs"],"pageSize":6,"page":page
        }))
        .expect("query");
        let out = execute(&provider, &query, &default_context(), &Passthrough)
            .await
            .expect("page");
        assert_eq!(out.data["pagination"]["totalPages"], 3, "{}", out.data);
        let paths = paths_of(&out);
        sizes.push(paths.len());
        listed.extend(paths);
        if out.data["pagination"]["hasMore"] != true {
            break;
        }
    }
    assert_eq!(sizes, [3, 6, 3]);
    let mut unique = listed.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 12, "{listed:?}");
    assert_eq!(listed.len(), 12, "{listed:?}");
    let searches = server.received_requests().await.unwrap_or_default().len();
    assert_eq!(searches, 5, "4 .ts pages and one .rs page");
}

mod backed;
