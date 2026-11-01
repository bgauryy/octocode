//! Empty searches: the default-branch index named, cause-specific hints and scoped recovery leads.
use super::*;
use octocode_github::mock_http::mount_json;
use serde_json::json;
use wiremock::MockServer;

/// Empty searches name the default-branch index only when a branch
/// was requested; otherwise the hint names the scope that came up empty.
#[tokio::test]
async fn empty_code_search_names_the_default_branch_index() {
    let server = MockServer::builder().start().await;
    empty_code_search(&server, false).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main","full_name":"a/b"}),
    )
    .await;
    for (query, cause) in [
        (
            json!({ "owner":"a","repo":"b","keywords":["needle"],"ref":"dev"}),
            "default branch",
        ),
        (
            json!({ "owner":"a","repo":"b","keywords":["needle"]}),
            "in a/b",
        ),
        (
            json!({ "owner":"a","keywords":["needle"]}),
            "any a repository",
        ),
        (
            json!({ "owner":"a","repo":"b","keywords":["needle"],"extensions":["rs"]}),
            "extensions",
        ),
        (
            json!({ "owner":"a","repo":"b","keywords":["needle"],"matchIn":["path"]}),
            "path",
        ),
    ] {
        let out = run(&server, query).await.expect("search");
        assert_eq!(out.status, Some("empty"));
        // Only a requested ref is a per-call fact; every other cause is
        // the row's leads and the description.
        if cause == "default branch" {
            let hint = out.data["hints"][0].as_str().unwrap_or_default();
            assert!(hint.contains("`dev` was not searched"), "{}", out.data);
            assert!(hint.len() <= 120, "{hint}");
        } else {
            crate::tools::test_support::checks::assert_leads_valid(&out.data);
        }
    }
}

/// Empty-search hints name the actual cause, and a repository
/// known to exist gets no root viewTree.
#[tokio::test]
async fn empty_code_search_hints_are_cause_specific() {
    let server = MockServer::builder().start().await;
    empty_code_search(&server, false).await;
    for (repo, full_name) in [("a/b", "a/b"), ("a/old", "c/d"), ("c/d", "c/d")] {
        mount_json(
            &server,
            format!("/api/v3/repos/{repo}"),
            200,
            json!({"default_branch":"main","full_name":full_name}),
        )
        .await;
    }
    let renamed = run(
        &server,
        json!({ "owner":"a","repo":"old","keywords":["needle"]}),
    )
    .await
    .expect("search");
    // The rename is followed: the warning names it, and the empty
    // answer is about the canonical repository.
    assert_eq!(
        renamed.data["warnings"],
        json!(["Repository a/old was renamed to c/d; leads use the canonical name."]),
        "{}",
        renamed.data
    );
    assert!(renamed.data.get("hints").is_none(), "{}", renamed.data);
    assert!(
        renamed.data["next"].get("retryRenamed").is_none(),
        "{}",
        renamed.data
    );

    let existing = run(
        &server,
        json!({ "owner":"a","repo":"b","keywords":["needle"]}),
    )
    .await
    .expect("search");
    // Beside the clone lead (dropped where clone is unavailable), the
    // root listing is a route every surface runs.
    assert_eq!(
        crate::tools::test_support::checks::related_call(&existing.data, "ghStructure", |input| {
            input["queries"][0]["operation"] == "tree"
        })["input"]["queries"][0],
        json!({"operation":"tree","owner":"a","repo":"b"}),
        "{}",
        existing.data
    );
    assert!(
        crate::tools::test_support::checks::find_related_call(
            &existing.data,
            "ghCloneRepo",
            |_| true
        )
        .is_some(),
        "{}",
        existing.data
    );
    let scoped = run(
        &server,
        json!({"owner":"a","repo":"b","keywords":["needle"],"pathPrefix":"src/io"}),
    )
    .await
    .expect("search");
    assert_eq!(
        crate::tools::test_support::checks::related_call(
            &scoped.data,
            "ghStructure",
            |input| input["queries"][0]["operation"] == "tree"
        )["input"]["queries"][0]["path"],
        "src/io",
        "{}",
        scoped.data
    );

    let path_mode = run(
        &server,
        json!({ "owner":"a","repo":"b",
                   "keywords":["fn spawn_blocking"],"matchIn":["path"]}),
    )
    .await
    .expect("search");
    // The lead is the recovery; no sentence restates it.
    assert!(path_mode.data.get("hints").is_none(), "{}", path_mode.data);
    let content =
        &crate::tools::test_support::checks::related_call(&path_mode.data, "ghSearchCode", |_| {
            true
        })["input"]["queries"][0];
    let opened = crate::tools::test_support::opened(crate::id::ToolId::GhSearchCode, content);
    assert_eq!(opened["matchIn"], json!(["content"]), "{}", path_mode.data);
    assert_eq!(content["keywords"], json!(["fn spawn_blocking"]));
}

/// A zero-hit recovery for a requested branch inspects that branch,
/// not the default branch the index covers.
#[tokio::test]
async fn empty_scoped_recovery_keeps_the_requested_branch() {
    let server = MockServer::builder().start().await;
    empty_code_search(&server, false).await;
    for (repo, archived) in [("b", false), ("old", true)] {
        mount_json(
            &server,
            format!("/api/v3/repos/a/{repo}"),
            200,
            json!({"default_branch":"main","full_name":format!("a/{repo}"),"archived":archived}),
        )
        .await;
    }
    for repo in ["b", "old"] {
        let out = run(
            &server,
            json!({"owner":"a","repo":repo,
                       "keywords":["needle"],"pathPrefix":"src","ref":"nondefault"}),
        )
        .await
        .expect("search");
        let view =
            &crate::tools::test_support::checks::related_call(&out.data, "ghStructure", |input| {
                input["queries"][0]["operation"] == "tree"
            })["input"]["queries"][0];
        assert_eq!(view["ref"], "nondefault", "{}", out.data);
    }
    let default = run(
        &server,
        json!({"owner":"a","repo":"b","keywords":["needle"],"pathPrefix":"src"}),
    )
    .await
    .expect("search");
    assert!(
        crate::tools::test_support::checks::related_call(
            &default.data,
            "ghStructure",
            |input| input["queries"][0]["operation"] == "tree"
        )["input"]["queries"][0]
            .get("ref")
            .is_none(),
        "{}",
        default.data
    );
}

/// An empty search of an existing repository leads to its structure:
/// the index may lag, so verify the path before concluding absence.
#[tokio::test]
async fn an_empty_scoped_search_leads_to_its_structure() {
    let server = MockServer::builder().start().await;
    empty_code_search(&server, false).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main","full_name":"a/b"}),
    )
    .await;
    let out = run(
        &server,
        json!({"owner":"a","repo":"b","keywords":["absent"],"pathPrefix":"src"}),
    )
    .await
    .expect("search");
    assert_eq!(out.status, Some("empty"));
    let lead =
        crate::tools::test_support::checks::related_call(&out.data, "ghStructure", |input| {
            input["queries"][0]["operation"] == "tree"
        });
    assert_eq!(lead["tool"], "ghStructure", "{}", out.data);
    assert_eq!(lead["input"]["queries"][0]["repo"], "b", "{}", out.data);
    assert_eq!(lead["input"]["queries"][0]["path"], "src", "{}", out.data);
}
