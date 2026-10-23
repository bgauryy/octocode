//! Empty searches: the default-branch index named, cause-specific hints and scoped recovery leads.
use super::*;

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
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"ref":"dev"}),
            "default branch",
        ),
        (
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"]}),
            "in a/b",
        ),
        (
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","keywords":["needle"]}),
            "any a repository",
        ),
        (
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"extensions":["rs"]}),
            "extensions",
        ),
        (
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"matchIn":"path"}),
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
            // A broadened search names what its rows drop, and nothing else.
            let hints = out.data["hints"].as_array().map_or(&[][..], Vec::as_slice);
            assert!(
                hints.iter().all(|hint| hint
                    .as_str()
                    .is_some_and(|hint| hint.starts_with("broadenSearch rows: "))),
                "{cause}: {}",
                out.data
            );
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
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"old","keywords":["needle"]}),
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
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"]}),
        )
        .await
        .expect("search");
    // Beside the clone lead (dropped where clone is unavailable), the
    // root listing is a route every surface runs.
    assert_eq!(
        existing.data["next"]["viewTree"]["query"]["queries"][0],
        json!({"owner":"a","repo":"b"}),
        "{}",
        existing.data
    );
    assert!(
        existing.data["next"].get("cloneRepo").is_some(),
        "{}",
        existing.data
    );
    let scoped = run(
            &server,
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"path":"src/io"}),
        )
        .await
        .expect("search");
    assert_eq!(
        scoped.data["next"]["viewTree"]["query"]["queries"][0]["path"], "src/io",
        "{}",
        scoped.data
    );

    let path_mode = run(
        &server,
        json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b",
                   "keywords":["fn spawn_blocking"],"matchIn":"path"}),
    )
    .await
    .expect("search");
    // The lead is the recovery; no sentence restates it.
    assert!(path_mode.data.get("hints").is_none(), "{}", path_mode.data);
    let content = &path_mode.data["next"]["findCode"]["query"]["queries"][0];
    assert_eq!(content["matchIn"], "file", "{}", path_mode.data);
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
            json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":repo,
                       "keywords":["needle"],"path":"src","ref":"nondefault"}),
        )
        .await
        .expect("search");
        let view = &out.data["next"]["viewTree"]["query"]["queries"][0];
        assert_eq!(view["ref"], "nondefault", "{}", out.data);
    }
    let default = run(
        &server,
        json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                   "keywords":["needle"],"path":"src"}),
    )
    .await
    .expect("search");
    assert!(
        default.data["next"]["viewTree"]["query"]["queries"][0]
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
        json!({"operation":"code","owner":"a","repo":"b","keywords":["absent"],"path":"src"}),
    )
    .await
    .expect("search");
    assert_eq!(out.status, Some("empty"));
    let lead = &out.data["next"]["viewTree"];
    assert_eq!(lead["tool"], "ghStructure", "{}", out.data);
    assert_eq!(lead["query"]["queries"][0]["repo"], "b", "{}", out.data);
    assert_eq!(lead["query"]["queries"][0]["path"], "src", "{}", out.data);
}
