//! `commit` with `repo`: what GitHub says of the commit and its repository,
//! beside OSV's answer, which a GitHub failure never removes.
use super::*;
use crate::artifact::test_http::{RouteHttp, test_budget};
use crate::security_check::advisories::AdvisoryRead;
use crate::security_check::github_double::{Double, net, recorded_advisories};
use crate::security_check::repository::RepoFacts;
use octocode_github::RefHolder;
use octocode_runtime_base::providers::saved_answers::ReusedSince;
use octocode_test_support::json_fixture;

const SHA: &str = "4f0b76e2eca13de1c1fe8b4305abc1f7d63f4b86";

fn found() -> RepoRead {
    RepoRead::Found(RepoFacts {
        owner: "lodash".into(),
        name: "lodash".into(),
        stars: 5000,
        default_branch: "main".into(),
        pushed_at: Some("2999-01-01T00:00:00Z".into()),
        ..RepoFacts::default()
    })
}

fn signed(heads: &[&str], on_default: Option<bool>) -> CommitLookup {
    CommitLookup::Found(CommitFacts {
        verified: true,
        reason: Some("valid".into()),
        heads: Some(heads.iter().map(|head| (*head).to_owned()).collect()),
        pulls: Some(vec![6249]),
        tags: Some(vec!["4.18.1".into()]),
        on_default,
        ..CommitFacts::default()
    })
}

async fn row(github: &Double) -> Value {
    row_in(github, octocode_contract_types::SecurityCheckView::Details).await
}

async fn row_in(github: &Double, view: octocode_contract_types::SecurityCheckView) -> Value {
    let http = RouteHttp::new().post_sequence(vec![(200, json_fixture(r#"{"results": [{}]}"#))]);
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::security_check::tests::default_config();
    crate::security_check::commit(
        SHA,
        Some("lodash/lodash"),
        None,
        Map::new(),
        view,
        &net(&http, &budget, &reused, &config, github),
    )
    .await
    .expect("row")
}

fn rules(data: &Value) -> Vec<&str> {
    data["cautions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|caution| caution["rule"].as_str())
        .collect()
}

fn github_entry(data: &Value) -> &Value {
    let entries = data["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .filter(|entry| entry["source"] == "github")
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1, "one GitHub entry: {data}");
    entries[0]
}

#[tokio::test]
async fn a_signed_commit_reads_as_its_facts_and_never_as_affected() {
    let github = Double::repo(found())
        .commit(signed(&["main"], Some(true)))
        .page(None, recorded_advisories(1), None);
    let data = row(&github).await;
    let facts = &github_entry(&data)["facts"];
    assert_eq!(facts["exists"], true, "{data}");
    assert_eq!(facts["signature"]["verified"], true, "{data}");
    assert_eq!(facts["heads"], json_fixture(r#"["main"]"#), "{data}");
    assert_eq!(facts["pulls"], json_fixture(r#"["6249"]"#), "{data}");
    assert_eq!(facts["inTag"], json_fixture(r#"["4.18.1"]"#), "{data}");
    assert!(
        data.get("affected").is_none(),
        "no OSV match, no verdict: {data}"
    );
    assert_eq!(data["repository"]["stars"], 5000, "{data}");
    assert!(rules(&data).is_empty(), "{data}");
    // The repository's advisories are named, not counted against the commit.
    let named = facts["repositoryAdvisories"].as_array().expect("named");
    assert_eq!(named.len(), recorded_advisories(1).len(), "{data}");
    assert_eq!(data["advisories"], json!([]), "{data}");
    assert_eq!(
        data["related"][0]["input"]["queries"][0],
        json_fixture(r#"{"operation": "repository", "repo": "lodash/lodash"}"#),
        "{data}"
    );
}

/// The summary of a commit GitHub holds says only what OSV knows: where
/// GitHub holds it is detail (`view:"details"`), and a commit it lacks still
/// reads as its caution.
#[tokio::test]
async fn a_commit_summary_leaves_githubs_facts_to_the_details() {
    use octocode_contract_types::SecurityCheckView;
    let github = Double::repo(found())
        .commit(signed(&["main"], Some(true)))
        .page(None, Vec::new(), None);
    let summary = row_in(&github, SecurityCheckView::Summary).await;
    let answer = summary["answer"].as_str().unwrap_or_default();
    assert!(answer.contains("None known to OSV"), "{summary}");
    assert!(!answer.contains("GitHub"), "{summary}");
    let details = row(&github).await;
    assert!(
        details["answer"]
            .as_str()
            .unwrap_or_default()
            .contains("signed and verified"),
        "{details}"
    );
    let missing = Double::repo(found())
        .commit(CommitLookup::Missing)
        .page(None, Vec::new(), None);
    let summary = row_in(&missing, SecurityCheckView::Summary).await;
    assert_eq!(rules(&summary), ["commitNotInRepository"], "{summary}");
}

#[tokio::test]
async fn a_commit_the_repository_lacks_draws_the_contract_rule() {
    let github = Double::repo(found())
        .commit(CommitLookup::Missing)
        .page(None, Vec::new(), None);
    let data = row(&github).await;
    assert_eq!(rules(&data), ["commitNotInRepository"], "{data}");
    assert_eq!(github_entry(&data)["status"], "noneKnown", "{data}");
    assert_eq!(github_entry(&data)["facts"]["exists"], false, "{data}");
}

#[tokio::test]
async fn a_commit_on_no_branch_pull_or_tag_and_not_in_the_default_history_may_be_a_fork_s() {
    let lonely = CommitLookup::Found(CommitFacts {
        verified: false,
        on_default: Some(false),
        heads: Some(Vec::new()),
        pulls: Some(Vec::new()),
        tags: Some(Vec::new()),
        other_ref: OtherRefs::NoneHolds,
        ..CommitFacts::default()
    });
    let github = Double::repo(found())
        .commit(lonely)
        .page(None, Vec::new(), None);
    let data = row(&github).await;
    assert_eq!(rules(&data), ["onlyInForkNetwork"], "{data}");
    let on_main =
        Double::repo(found())
            .commit(signed(&[], Some(true)))
            .page(None, Vec::new(), None);
    assert!(rules(&row(&on_main).await).is_empty());
}

/// A backport: committed on a release branch, in no pull request or tag, not
/// the head of any branch and not in the default branch's history.
#[tokio::test]
async fn a_backport_on_a_release_branch_is_not_a_forks_commit() {
    let backport = CommitLookup::Found(CommitFacts {
        verified: true,
        on_default: Some(false),
        heads: Some(Vec::new()),
        pulls: Some(Vec::new()),
        tags: Some(Vec::new()),
        other_ref: OtherRefs::Holds(RefHolders {
            holders: vec![RefHolder {
                kind: RefKind::Branches,
                name: "release/4.17".into(),
            }],
            complete: true,
            failed: None,
        }),
        ..CommitFacts::default()
    });
    let github = Double::repo(found())
        .commit(backport)
        .page(None, Vec::new(), None);
    let data = row(&github).await;
    assert!(rules(&data).is_empty(), "{data}");
    assert_eq!(
        github_entry(&data)["facts"]["inBranch"],
        json_fixture(r#"["release/4.17"]"#),
        "{data}"
    );
    assert_eq!(
        github_entry(&data)["facts"]["inRefsComplete"],
        true,
        "{data}"
    );
    assert!(
        data["answer"]
            .as_str()
            .unwrap_or_default()
            .contains("release/4.17"),
        "{data}"
    );
}

/// A release branch tagged and deleted: the tag's history is the only place
/// that holds the commit, and it is no fork's.
#[tokio::test]
async fn a_commit_only_a_tags_history_holds_is_not_a_forks_commit_and_names_the_tag() {
    let tagged = CommitLookup::Found(CommitFacts {
        verified: true,
        on_default: Some(false),
        heads: Some(Vec::new()),
        pulls: Some(Vec::new()),
        tags: Some(Vec::new()),
        other_ref: OtherRefs::Holds(RefHolders {
            holders: vec![
                RefHolder {
                    kind: RefKind::Tags,
                    name: "v1.2.0".into(),
                },
                RefHolder {
                    kind: RefKind::Tags,
                    name: "v1.2.1".into(),
                },
            ],
            complete: false,
            failed: None,
        }),
        ..CommitFacts::default()
    });
    let github = Double::repo(found())
        .commit(tagged)
        .page(None, Vec::new(), None);
    let data = row(&github).await;
    assert!(rules(&data).is_empty(), "{data}");
    assert_eq!(
        github_entry(&data)["facts"]["inTag"],
        json_fixture(r#"["v1.2.0", "v1.2.1"]"#),
        "every holder found is named: {data}"
    );
    assert_eq!(
        github_entry(&data)["facts"]["inRefsComplete"],
        false,
        "{data}"
    );
    let answer = data["answer"].as_str().unwrap_or_default();
    assert!(
        answer.contains("v1.2.0, v1.2.1") && answer.contains("other branches or tags"),
        "an unfinished walk says the list may be short: {data}"
    );
    assert!(
        data["answer"]
            .as_str()
            .unwrap_or_default()
            .contains("history of tag"),
        "{data}"
    );
}

#[tokio::test]
async fn a_commit_whose_other_branches_were_not_read_is_no_forks_commit_but_names_what_was_not() {
    let unread = CommitLookup::Found(CommitFacts {
        verified: true,
        on_default: Some(false),
        unread: vec![Unread {
            what: "the history of the other branches",
            failure: RepoFailure {
                code: ErrorCode::RateLimited,
                why: "resets at 2999".into(),
            },
        }],
        ..CommitFacts::default()
    });
    let github = Double::repo(found())
        .commit(unread)
        .page(None, Vec::new(), None);
    let data = row(&github).await;
    assert!(rules(&data).is_empty(), "{data}");
    assert!(
        github_entry(&data)["why"]
            .as_str()
            .unwrap_or_default()
            .contains("the history of the other branches not read"),
        "{data}"
    );
}

#[tokio::test]
async fn a_github_that_fails_leaves_the_osv_answer_and_a_partial_row_with_its_reason() {
    let github = Double::repo(RepoRead::Failed(RepoFailure {
        code: ErrorCode::RateLimited,
        why: "GitHub rate limit reached; it resets at 2999-01-01T00:00:00Z.".into(),
    }));
    let data = row(&github).await;
    let entry = github_entry(&data);
    assert_eq!(entry["status"], "failed", "{data}");
    assert!(
        data.get("status").is_none(),
        "an empty OSV beside a failed GitHub is not an empty row: {data}"
    );
    assert_eq!(entry["errorCode"], "rateLimited", "{data}");
    // The entry says what was not read; the rate limit's reset time is the
    // response's, said once.
    assert_eq!(entry["why"], "Repository not read.", "{data}");
    assert!(data["notices"].to_string().contains("resets at"), "{data}");
    assert!(
        data["partialReasons"].to_string().contains("sourceFailed"),
        "{data}"
    );
    assert!(
        data["sources"].to_string().contains("\"osv\""),
        "OSV stays: {data}"
    );
    assert_eq!(
        data["advisories"],
        json!([]),
        "OSV's finished answer is kept: {data}"
    );
}

#[tokio::test]
async fn reads_that_fail_after_the_commit_was_found_are_named_unread() {
    let facts = CommitLookup::Found(CommitFacts {
        verified: true,
        unread: vec![Unread {
            what: "tags",
            failure: RepoFailure {
                code: ErrorCode::RateLimited,
                why: "resets at 2999".into(),
            },
        }],
        ..CommitFacts::default()
    });
    let github = Double::repo(found())
        .commit(facts)
        .page(None, Vec::new(), None);
    let data = row(&github).await;
    let entry = github_entry(&data);
    assert_eq!(entry["status"], "found", "{data}");
    assert!(data.get("status").is_none(), "{data}");
    assert!(
        entry["why"]
            .as_str()
            .unwrap_or_default()
            .contains("tags not read"),
        "{data}"
    );
    assert!(
        data["partialReasons"].to_string().contains("sourceFailed"),
        "{data}"
    );
}

fn osv_record_of(repo_url: &str) -> Value {
    json!({"id": "OSV-2020-484", "aliases": ["CVE-2023-25193"], "affected": [{
        "ranges": [{"type": "GIT", "repo": repo_url,
            "events": [{"introduced": "0"}, {"fixed": "abc"}]}]}]})
}

async fn url_row(url: &str, record: Value, github: &Double) -> Value {
    use crate::security_check::osv_double::OsvPages;
    let http = RouteHttp::new().osv_pages(vec![(200, json!({"vulns": [record]}))]);
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::security_check::tests::default_config();
    crate::security_check::from_url(
        url,
        Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
        &net(&http, &budget, &reused, &config, github),
    )
    .await
    .expect("row")
}

#[tokio::test]
async fn a_record_naming_the_same_repository_by_another_spelling_raises_no_warning() {
    let url = format!("https://github.com/o/r/commit/{SHA}");
    for spelling in [
        "https://github.com/O/R.git",
        "https://github.com/o/r/",
        "git@github.com:O/r.git",
        "http://www.github.com/o/R",
    ] {
        let github = Double::repo(found())
            .commit(signed(&["main"], Some(true)))
            .page(None, Vec::new(), None);
        let data = url_row(&url, osv_record_of(spelling), &github).await;
        assert_eq!(data["affected"], true, "{spelling}: {data}");
        assert!(
            !data["warnings"].to_string().contains("another repository"),
            "{spelling}: {data}"
        );
    }
    let github = Double::repo(found())
        .commit(signed(&["main"], Some(true)))
        .page(None, Vec::new(), None);
    let other = url_row(&url, osv_record_of("https://gitlab.com/o/r"), &github).await;
    assert!(
        other["warnings"].to_string().contains("another repository"),
        "another host is another repository: {other}"
    );
}

/// An OSV record of this commit that GitHub also publishes, by alias.
async fn row_with_osv_record(github: &Double, record: Value) -> Value {
    use crate::security_check::osv_double::OsvPages;
    let http = RouteHttp::new().osv_pages(vec![(200, json!({"vulns": [record]}))]);
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::security_check::tests::default_config();
    crate::security_check::commit(
        SHA,
        Some("lodash/lodash"),
        None,
        Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
        &net(&http, &budget, &reused, &config, github),
    )
    .await
    .expect("row")
}

#[tokio::test]
async fn an_osv_record_of_the_commit_is_merged_with_its_github_twin_and_the_others_are_named() {
    let published = recorded_advisories(1);
    let twin = &published[0];
    let record = json!({"id": "OSV-1", "aliases": [twin.ghsa_id], "affected": [{
        "ranges": [{"type": "GIT", "repo": "https://github.com/lodash/lodash",
            "events": [{"introduced": "0"}, {"fixed": "abc"}]}]}]});
    let github = Double::repo(found())
        .commit(signed(&["main"], Some(true)))
        .page(None, published.clone(), None);
    let data = row_with_osv_record(&github, record).await;
    let rows = data["advisories"].as_array().expect("advisories");
    // One advisory, counted once; the details list OSV's record and GitHub's
    // twin of it, each whole, the twin with what OSV's record does not state.
    assert!(
        data["answer"]
            .as_str()
            .is_some_and(|lead| lead.contains("affected range of 1 known advisory")),
        "{data}"
    );
    assert_eq!(rows.len(), 2, "{data}");
    let wanted = twin.severity.clone().expect("recorded severity");
    assert_eq!(
        rows[1]["severity"].as_str().map(str::to_lowercase),
        Some(wanted.to_lowercase()),
        "GitHub's twin states what OSV's record does not: {data}"
    );
    let named = github_entry(&data)["facts"]["repositoryAdvisories"].to_string();
    assert!(!named.contains(&twin.ghsa_id), "the twin is merged: {data}");
    assert!(
        named.contains(&published[1].ghsa_id),
        "the others are named: {data}"
    );
}

#[tokio::test]
async fn an_advisory_walk_that_fails_beside_a_found_commit_says_the_advisories_were_not_read() {
    let github = Double::repo(found())
        .commit(signed(&["main"], Some(true)))
        .page_read(
            None,
            AdvisoryRead::Failed(RepoFailure {
                code: ErrorCode::RateLimited,
                why: "resets at 2999".into(),
            }),
        );
    let data = row(&github).await;
    let entry = github_entry(&data);
    assert_eq!(entry["status"], "found", "{data}");
    assert!(
        entry["why"]
            .as_str()
            .unwrap_or_default()
            .contains("Advisories not read"),
        "{data}"
    );
    assert!(data.get("status").is_none(), "{data}");
    assert!(
        data["partialReasons"].to_string().contains("sourceFailed"),
        "{data}"
    );
}

/// The page that failed leaves the rows of the pages before it: the row keeps
/// them and says they are a part, as the repository operation does.
#[tokio::test]
async fn an_advisory_walk_that_stops_after_a_page_says_partly_read_and_keeps_the_rows() {
    let published = recorded_advisories(1);
    let github = Double::repo(found())
        .commit(signed(&["main"], Some(true)))
        .page(None, published.clone(), Some("cursor-2"))
        .page_read(
            Some("cursor-2"),
            AdvisoryRead::Failed(RepoFailure {
                code: ErrorCode::RateLimited,
                why: "resets at 2999".into(),
            }),
        );
    let data = row(&github).await;
    let why = github_entry(&data)["why"].as_str().unwrap_or_default();
    assert!(
        why.contains(&format!(
            "Advisories read in part: {} listed",
            published.len()
        )),
        "{data}"
    );
    assert!(!why.contains("Repository advisories not read"), "{data}");
    assert!(
        data["notices"].to_string().contains("resets at 2999"),
        "{data}"
    );
    let named = github_entry(&data)["facts"]["repositoryAdvisories"].to_string();
    for advisory in &published {
        assert!(named.contains(&advisory.ghsa_id), "rows are kept: {data}");
    }
    assert!(
        data["partialReasons"].to_string().contains("sourceFailed"),
        "{data}"
    );
}

/// A commit read that left a part unread does not hide the advisory walk that
/// stopped too: the listed advisories are still said to be a part.
#[tokio::test]
async fn an_unread_commit_part_and_a_stopped_advisory_walk_are_both_named() {
    let published = recorded_advisories(1);
    let facts = CommitLookup::Found(CommitFacts {
        verified: true,
        unread: vec![Unread {
            what: "tags",
            failure: RepoFailure {
                code: ErrorCode::ProviderError,
                why: "GitHub did not answer.".into(),
            },
        }],
        ..CommitFacts::default()
    });
    let github = Double::repo(found())
        .commit(facts)
        .page(None, published.clone(), Some("cursor-2"))
        .page_read(
            Some("cursor-2"),
            AdvisoryRead::Failed(RepoFailure {
                code: ErrorCode::RateLimited,
                why: "resets at 2999".into(),
            }),
        );
    let data = row(&github).await;
    let why = github_entry(&data)["why"].as_str().unwrap_or_default();
    assert!(why.contains("tags not read"), "{data}");
    assert!(
        why.contains(&format!(
            "Advisories read in part: {} listed",
            published.len()
        )),
        "{data}"
    );
    // Each is stated once, on the entry, never again in `warnings`.
    let warnings = data["warnings"].to_string();
    assert!(!warnings.contains("tags not read"), "{data}");
    assert!(!warnings.contains("Advisories read in part"), "{data}");
}

#[tokio::test]
async fn a_renamed_repository_still_has_its_commit_read_and_says_it_moved() {
    let moved = RepoRead::Moved(RepoFacts {
        owner: "lodash-new".into(),
        name: "lodash".into(),
        stars: 5000,
        default_branch: "main".into(),
        pushed_at: Some("2999-01-01T00:00:00Z".into()),
        ..RepoFacts::default()
    });
    let github = Double::repo(moved)
        .commit(signed(&["main"], Some(true)))
        .page(None, Vec::new(), None);
    let data = row(&github).await;
    assert_eq!(rules(&data), ["repositoryMoved"], "{data}");
    assert_eq!(data["repository"]["owner"], "lodash-new", "{data}");
    assert_eq!(github_entry(&data)["facts"]["exists"], true, "{data}");
}

/// An OSV record of the commit that names `alias`, as GitHub's twin of it does.
fn osv_record_aliased(id: &str, alias: &str) -> Value {
    json!({"id": id, "aliases": [alias], "affected": [{
        "ranges": [{"type": "GIT", "repo": "https://github.com/lodash/lodash",
            "events": [{"introduced": "0"}, {"fixed": "abc"}]}]}]})
}

/// One `commit` call over `http`, resumed or not.
async fn commit_call(
    http: &RouteHttp,
    repo: Option<&str>,
    resume: Option<&str>,
    github: &Double,
) -> Value {
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::security_check::tests::default_config();
    crate::security_check::commit(
        SHA,
        repo,
        resume,
        Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
        &net(http, &budget, &reused, &config, github),
    )
    .await
    .expect("row")
}

/// OSV lists the commit's advisories over two pages and the second page fails
/// transiently, so the call ends with the walk unfinished.
fn first_page_then_a_failure(first: Value) -> RouteHttp {
    use crate::security_check::osv_double::OsvPages;
    RouteHttp::new().osv_pages(vec![
        (200, json!({"vulns": [first], "next_page_token": "t1"})),
        (503, json!({})),
    ])
}

#[tokio::test]
async fn an_unfinished_osv_walk_names_no_repository_advisory_as_not_known_to_affect_the_commit() {
    let published = recorded_advisories(1);
    let github = Double::repo(found())
        .commit(signed(&["main"], Some(true)))
        .page(None, published.clone(), None);
    let http = first_page_then_a_failure(osv_record_aliased("OSV-1", &published[0].ghsa_id));
    let data = commit_call(&http, Some("lodash/lodash"), None, &github).await;
    assert!(
        data["sources"].to_string().contains("partial"),
        "the OSV walk is unfinished: {data}"
    );
    assert!(
        github_entry(&data)["facts"]
            .get("repositoryAdvisories")
            .is_none(),
        "no advisory is called not known to affect the commit before OSV's last page: {data}"
    );
    assert!(
        data["warnings"]
            .to_string()
            .contains("repository advisories"),
        "the row says the comparison waits: {data}"
    );
    assert!(
        github.asked().is_empty(),
        "no advisory list is read to be compared with half of OSV's: {:?}",
        github.asked()
    );
}

#[tokio::test]
async fn the_final_page_of_an_osv_walk_merges_twins_and_names_the_others_from_the_whole_list() {
    use crate::security_check::osv_double::OsvPages;
    let published = recorded_advisories(1);
    let github = || {
        Double::repo(found())
            .commit(signed(&["main"], Some(true)))
            .page(None, published.clone(), None)
    };
    let first = first_page_then_a_failure(osv_record_aliased("OSV-1", &published[0].ghsa_id));
    let stopped = commit_call(&first, Some("lodash/lodash"), None, &github()).await;
    let resume = stopped["next"]["nextPage"]["input"]["queries"][0]["resume"]
        .as_str()
        .expect("a call that reads on")
        .to_owned();
    // The advisory OSV lists on its second page is the second one GitHub publishes.
    let rest = RouteHttp::new().osv_pages(vec![(
        200,
        json!({"vulns": [osv_record_aliased("OSV-2", &published[1].ghsa_id)]}),
    )]);
    let finished = github();
    let data = commit_call(&rest, Some("lodash/lodash"), Some(&resume), &finished).await;
    // Two advisories, counted once each; the details list OSV's two records
    // and GitHub's twin of each, whole.
    assert!(
        data["answer"]
            .as_str()
            .is_some_and(|lead| lead.contains("affected range of 2 known advisories")),
        "{data}"
    );
    assert_eq!(
        data["advisories"].as_array().map(Vec::len),
        Some(4),
        "{data}"
    );
    assert!(
        github_entry(&data)["facts"]
            .get("repositoryAdvisories")
            .is_none(),
        "both of GitHub's advisories are OSV's, from either page: {data}"
    );
    assert!(
        data["next"]["nextPage"].is_null(),
        "the walk is finished: {data}"
    );
}

#[tokio::test]
async fn a_repo_that_is_not_on_github_says_it_was_not_read_there() {
    use crate::security_check::osv_double::OsvPages;
    let github = Double::repo(found());
    let http = RouteHttp::new().osv_pages(vec![(200, json!({}))]);
    let data = commit_call(&http, Some("https://gitlab.com/o/r"), None, &github).await;
    let warnings = data["warnings"].to_string();
    assert!(
        warnings.contains("https://gitlab.com/o/r is not a GitHub repository")
            && warnings.contains("not read on GitHub"),
        "{data}"
    );
    assert!(
        !data["sources"].to_string().contains("github"),
        "no GitHub entry for a repository GitHub was never asked about: {data}"
    );
}

/// GitHub withdrew the repository advisory that is the twin of OSV's record
/// of the commit (live: CVE-2024-51999 / GHSA-pj86-cfqh-vqx6): the advisory is
/// withdrawn, stated as such, and never counted (check C D2).
#[tokio::test]
async fn a_commit_row_never_counts_an_advisory_github_withdrew() {
    let published = recorded_advisories(1);
    let mut twin = published[0].clone();
    twin.withdrawn_at = Some("2025-12-02T15:11:19Z".into());
    let record = json!({"id": twin.ghsa_id, "aliases": ["CVE-2024-51999"], "affected": [{
        "ranges": [{"type": "GIT", "repo": "https://github.com/lodash/lodash",
            "events": [{"introduced": "0"}, {"fixed": "abc"}]}]}]});
    let github = Double::repo(found())
        .commit(signed(&["main"], Some(true)))
        .page(None, vec![twin.clone()], None);
    let data = row_with_osv_record(&github, record).await;
    // OSV's record and GitHub's withdrawn twin: GitHub's withdrawal is the
    // advisory's, so neither is counted.
    let rows = data["advisories"].as_array().expect("advisories");
    assert!(
        rows.iter()
            .any(|row| row["withdrawn"] == "2025-12-02T15:11:19Z"),
        "{data}"
    );
    assert!(data.get("affected").is_none(), "never counted: {data}");
    let answer = data["answer"].as_str().unwrap_or_default();
    assert!(
        !answer.contains("affected range of 1") && answer.contains("withdrawn"),
        "{answer}"
    );
    assert!(!answer.contains("CVEs: CVE-2024-51999"), "{answer}");
}

/// GitHub withdrew a duplicate repository advisory that shares its CVE with
/// the live OSV record of the commit: the live advisory stays counted.
#[tokio::test]
async fn a_withdrawn_duplicate_never_hides_the_commits_live_advisory() {
    let published = recorded_advisories(1);
    let mut duplicate = published[0].clone();
    duplicate.ghsa_id = "GHSA-dupl-xxxx-wwww".into();
    duplicate.cve_id = Some("CVE-2024-0001".into());
    duplicate.withdrawn_at = Some("2025-12-02T15:11:19Z".into());
    let record = json_fixture(
        r#"{
            "id": "OSV-LIVE-1",
            "aliases": ["CVE-2024-0001"],
            "affected": [
                {
                    "ranges": [
                        {
                            "type": "GIT",
                            "repo": "https://github.com/lodash/lodash",
                            "events": [{"introduced": "0"}, {"fixed": "abc"}]
                        }
                    ]
                }
            ]
        }"#,
    );
    let github = Double::repo(found())
        .commit(signed(&["main"], Some(true)))
        .page(None, vec![duplicate], None);
    let data = row_with_osv_record(&github, record).await;
    assert_eq!(
        data["affected"], true,
        "the live advisory is hidden: {data}"
    );
    let rows = data["advisories"].as_array().expect("advisories");
    let live = rows
        .iter()
        .find(|row| row["id"] == "OSV-LIVE-1")
        .unwrap_or_else(|| panic!("the live record: {data}"));
    assert!(live.get("withdrawn").is_none(), "{data}");
    let answer = data["answer"].as_str().unwrap_or_default();
    assert!(
        answer.contains("affected range of 1") && answer.contains("GHSA-dupl-xxxx-wwww"),
        "{answer}"
    );
}

/// A failed GitHub read is stated once, on its `sources[]` entry: the row
/// joins the entry before it is marked partial, so no warning repeats it.
#[tokio::test]
async fn a_failed_github_read_is_stated_on_its_source_only() {
    let why = "GitHub did not answer (providerError); retry later.";
    let github = Double::repo(RepoRead::Failed(RepoFailure {
        code: ErrorCode::ProviderError,
        why: why.into(),
    }));
    let data = row(&github).await;
    assert_eq!(github_entry(&data)["why"], why, "{data}");
    assert!(
        !data["warnings"].to_string().contains("did not answer"),
        "{data}"
    );
    assert!(
        data["partialReasons"].to_string().contains("sourceFailed"),
        "{data}"
    );
}

/// A commit's details name each record's package: the record is listed whole,
/// with its own `affected` entries, not folded into one brief row.
#[tokio::test]
async fn commit_details_name_each_records_package() {
    use crate::security_check::osv_double::OsvPages;
    let record = |id: &str, package: &str| {
        json!({"id": id, "affected": [{
            "package": {"name": package, "ecosystem": "npm"},
            "ranges": [{"type": "GIT", "repo": "https://github.com/lodash/lodash",
                "events": [{"introduced": "0"}, {"fixed": "abc"}]}]}]})
    };
    let http = RouteHttp::new().osv_pages(vec![(
        200,
        json!({"vulns": [record("OSV-1", "lodash"), record("OSV-2", "lodash-es")]}),
    )]);
    let data = commit_call(&http, None, None, &Double::repo(found())).await;
    let packages = data["advisories"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["affected"][0]["packageName"].clone())
        .collect::<Vec<_>>();
    assert_eq!(packages, [json!("lodash"), json!("lodash-es")], "{data}");
}
