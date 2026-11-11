//! Repository rows, empty pages and their broadening, and the missing-owner hint.
use super::*;

/// A missing owner is not told to broaden its keywords: the error names
/// the login, and no search change repairs it.

#[test]
fn zero_repositories_are_empty_and_only_a_complete_search_broadens() {
    let query: GhSearchRepoQuery = serde_json::from_value(
        json!({"keywords":["x"],"operation":"search","filters":{"language":"rust"}}),
    )
    .expect("query");
    let complete = repository_output(json!({"repositories":[]}), &query, true, false, false);
    assert_eq!(complete.status, Some("empty"));
    let rows =
        &crate::tools::test_support::checks::related_call(&complete.data, "ghSearchRepo", |_| true)
            ["input"]["queries"];
    assert_eq!(rows.as_array().map(Vec::len), Some(2));
    assert_eq!(rows[0]["keywords"], json!(["x"]));
    assert!(rows[0].get("filters").is_none());
    assert_eq!(rows[1]["filters"]["language"], "rust");
    assert!(rows[1].get("keywords").is_none());
    crate::tools::test_support::checks::assert_leads_valid(&complete.data);

    // GitHub's incomplete page: empty (exit 1), no broadening; its retry
    // is added with the partial marking.
    let incomplete = repository_output(json!({"repositories":[]}), &query, true, true, false);
    assert_eq!(incomplete.status, Some("empty"));
    assert!(incomplete.data.get("next").is_none());
    // A capped result has rows beyond the cap: not empty.
    let capped = repository_output(json!({"repositories":[]}), &query, true, false, true);
    assert_eq!(capped.status, None);
    assert!(capped.data.get("next").is_none());
}

/// An empty search inside one repository with a lone keyword broadens
/// to that repository's own row; each filter drops by its wire name.
#[test]
fn a_lone_keyword_in_one_repository_broadens_to_its_row() {
    let query: GhSearchRepoQuery = serde_json::from_value(
        json!({"owner":"o","repo":"r","keywords":["x"],"operation":"search"}),
    )
    .expect("query");
    let empty = repository_output(json!({"repositories":[]}), &query, true, false, false);
    let row = &crate::tools::test_support::checks::related_call(
        &empty.data,
        "ghSearchRepo",
        |_| true,
    )["input"]["queries"][0];
    assert_eq!(row["owner"], "o");
    assert_eq!(row["repo"], "r");
    assert!(row.get("keywords").is_none());
    crate::tools::test_support::checks::assert_leads_valid(&empty.data);
    let matched: GhSearchRepoQuery =
        serde_json::from_value(json!({"keywords":["x"],"matchIn":["name"],"operation":"search"}))
            .expect("query");
    let empty = repository_output(json!({"repositories":[]}), &matched, true, false, false);
    let row = &crate::tools::test_support::checks::related_call(
        &empty.data,
        "ghSearchRepo",
        |_| true,
    )["input"]["queries"][0];
    assert_eq!(row["keywords"], json!(["x"]));
    assert!(row.get("matchIn").is_none());
    crate::tools::test_support::checks::assert_leads_valid(&empty.data);
}

/// A topic that only repeats the repository's own name is dropped;
/// every other topic stays.
#[test]
fn topic_naming_the_repository_itself_is_dropped() {
    let item: octocode_github::RepositorySearchItem = serde_json::from_value(json!({
        "full_name":"harlanc/xiu","name":"xiu","html_url":"h","default_branch":"main",
        "topics":["rtmp","XIU","rust"]
    }))
    .expect("item");
    let row = repository_row(item, &[]);
    assert_eq!(row["topics"], json!(["rtmp", "rust"]), "{row}");
}

/// A topic that only repeats the row's `language` is dropped too.
#[test]
fn topic_naming_the_language_is_dropped() {
    let item: octocode_github::RepositorySearchItem = serde_json::from_value(json!({
        "full_name":"psf/requests","name":"requests","html_url":"h","default_branch":"main",
        "language":"Python","topics":["python","http"]
    }))
    .expect("item");
    let row = repository_row(item, &[]);
    assert_eq!(row["topics"], json!(["http"]), "{row}");
    assert_eq!(row["language"], "Python", "{row}");
}

/// A repository only the token's account can see is marked with its
/// visibility (GitHub's `visibility`, else its `private` flag); a public
/// one carries no mark.
#[test]
fn a_repository_that_is_not_public_names_its_visibility() {
    let row = |extra: serde_json::Value| {
        let mut item =
            json!({"full_name":"me/r","name":"r","html_url":"h","default_branch":"main"});
        item.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        repository_row(serde_json::from_value(item).expect("item"), &[])
    };
    assert_eq!(
        row(json!({"visibility":"private","private":true}))["visibility"],
        "private"
    );
    assert_eq!(
        row(json!({"visibility":"internal"}))["visibility"],
        "internal"
    );
    assert_eq!(row(json!({"private":true}))["visibility"], "private");
    for public in [json!({"visibility":"public","private":false}), json!({})] {
        assert!(row(public.clone()).get("visibility").is_none(), "{public}");
    }
    let mut picked = json!({"repositories":[row(json!({"visibility":"private"}))]});
    crate::tools::gh_shared::pick_rows(&mut picked, "repositories", REPO_ID, &["stars".into()]);
    assert_eq!(
        picked["repositories"][0]["visibility"], "private",
        "{picked}"
    );
}

/// A listed row answers with what chooses the repository (its name, stars,
/// description and marks); one repository's own row adds its branch,
/// language, license and last push. A `fields` pick names any other fact
/// back, on either.
#[test]
fn a_default_row_leaves_secondary_facts_to_a_fields_pick() {
    let item = || -> octocode_github::RepositorySearchItem {
        serde_json::from_value(json!({
            "full_name":"psf/requests","name":"requests","html_url":"h","default_branch":"main",
            "stargazers_count":5,"forks_count":2,"language":"Python","archived":true,
            "license":{"spdx_id":"Apache-2.0"},"description":"HTTP for Humans","topics":["http"],
            "pushed_at":"2024-12-17T00:00:00Z","created_at":"2012-02-25T00:00:00Z",
            "updated_at":"2026-10-01T00:00:00Z"
        }))
        .expect("item")
    };
    let shown = |detail: &[&str], fields: &[&str]| {
        let fields = fields.iter().map(|field| (*field).to_owned()).collect::<Vec<_>>();
        let mut value = json!({"repositories":[repository_row(item(), &[])]});
        crate::tools::gh_shared::lean_rows(&mut value, "repositories", detail, &fields);
        crate::tools::gh_shared::pick_rows(&mut value, "repositories", REPO_ID, &fields);
        value["repositories"][0].clone()
    };
    assert_eq!(
        shown(LISTED_DETAIL, &[]),
        json!({"owner":"psf","repo":"requests","stars":5,"description":"HTTP for Humans","archived":true})
    );
    assert_eq!(
        shown(EXACT_DETAIL, &[]),
        json!({"owner":"psf","repo":"requests","defaultBranch":"main","stars":5,
            "language":"Python","license":"Apache-2.0","pushedAt":"2024-12-17",
            "description":"HTTP for Humans","archived":true})
    );
    // Every fact a default row leaves out is one `fields` pick away.
    for detail in [LISTED_DETAIL, EXACT_DETAIL] {
        let picked = shown(detail, detail);
        for key in detail {
            assert!(picked.get(*key).is_some(), "{key} is unreachable: {picked}");
        }
    }
}
