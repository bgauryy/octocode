//! Repository advisories: every page walked once, read as OSV-shaped rows,
//! merged with OSV by id, and filtered to a monorepo's package.
use super::*;
use crate::providers::artifact::test_http::{RouteHttp, test_budget};
use crate::providers::saved_answers::ReusedSince;
use crate::tools::security_check::ecosystem::Ecosystem;
use crate::tools::security_check::github_double::{Double, net, recorded_advisories};
use crate::tools::security_check::repository::{RepoFacts, RepoRead, Visibility};
use serde_json::json;

fn found() -> RepoRead {
    RepoRead::Found(RepoFacts {
        owner: "lodash".into(),
        name: "lodash".into(),
        stars: 5000,
        visibility: Visibility::Public,
        ..RepoFacts::default()
    })
}

fn two_pages() -> Double {
    Double::repo(found())
        .page(None, recorded_advisories(1), Some("cursor-2"))
        .page(Some("cursor-2"), recorded_advisories(2), None)
}

#[tokio::test]
async fn the_walk_asks_each_page_once_and_reads_every_advisory_once() {
    let github = two_pages();
    let walked = walk("lodash", "lodash", &github, None).await;
    assert!(matches!(walked.end, End::Complete), "{:?}", walked.end);
    let expected = recorded_advisories(1).len() + recorded_advisories(2).len();
    let ids = walked
        .advisories
        .iter()
        .map(|advisory| advisory.ghsa_id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(walked.advisories.len(), expected);
    assert_eq!(ids.len(), expected, "an advisory was read twice");
    assert_eq!(github.asked(), [None, Some("cursor-2".to_owned())]);
}

#[tokio::test]
async fn a_cursor_named_twice_ends_the_walk_as_a_failure_and_asks_no_page_again() {
    let github = Double::repo(found())
        .page(None, recorded_advisories(1), Some("loop"))
        .page(Some("loop"), recorded_advisories(2), Some("loop"));
    let walked = walk("lodash", "lodash", &github, None).await;
    let End::Failed(failure) = &walked.end else {
        panic!("{:?}", walked.end);
    };
    assert!(failure.why.contains("twice"), "{}", failure.why);
    assert_eq!(github.asked(), [None, Some("loop".to_owned())]);
}

#[tokio::test]
async fn a_page_that_fails_keeps_what_was_read_and_says_the_list_is_not_whole() {
    let refused = RepoFailure {
        code: ErrorCode::RateLimited,
        why: "resets at 2999-01-01T00:00:00Z".into(),
    };
    let github = Double::repo(found())
        .page(None, recorded_advisories(1), Some("cursor-2"))
        .page_read(Some("cursor-2"), AdvisoryRead::Failed(refused));
    let walked = walk("lodash", "lodash", &github, None).await;
    assert_eq!(walked.advisories.len(), recorded_advisories(1).len());
    assert_eq!(
        failure(&walked.end).map(|failure| failure.code),
        Some(ErrorCode::RateLimited)
    );
}

#[test]
fn a_monorepo_advisory_keeps_the_rows_package_and_names_the_others() {
    let advisory = &recorded_advisories(1)[0];
    let names = ["lodash.template".to_owned()];
    let own = row(advisory, &Selection::Named(Ecosystem::Npm, &names, None)).expect("row");
    let package_names = own
        .affected
        .iter()
        .filter_map(|entry| entry["packageName"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(package_names, ["lodash.template"]);
    assert!(
        own.also_affects.contains(&"lodash-es".to_owned()),
        "{own:?}"
    );
    assert!(!own.also_affects.contains(&"lodash.template".to_owned()));
    let whole = row(advisory, &Selection::All).expect("row");
    assert_eq!(whole.affected.len(), advisory.vulnerabilities.len());
    assert_eq!(whole.id, advisory.ghsa_id);
}

/// GitHub's ecosystem names read as OSV's by the contract's one map.
#[test]
fn github_ecosystem_names_read_as_the_osv_ecosystems_the_contract_maps_them_to() {
    let contract = security_contract().expect("contract");
    let mut advisory = recorded_advisories(1)[0].clone();
    for (github, osv) in [
        ("swift", "SwiftURL"),
        ("pip", "PyPI"),
        ("rust", "crates.io"),
        ("actions", "GitHub Actions"),
        ("erlang", "Hex"),
        ("composer", "Packagist"),
        ("RubyGems", "RubyGems"),
        // The contract maps none for `other`: it stays as GitHub wrote it.
        ("other", "other"),
    ] {
        advisory.vulnerabilities[0]
            .package
            .as_mut()
            .expect("package")
            .ecosystem = Some(github.to_owned());
        let shaped = osv_shape(&advisory, contract);
        assert_eq!(
            shaped["affected"][0]["package"]["ecosystem"], osv,
            "{github}"
        );
    }
}

#[test]
fn twins_are_the_github_rows_sharing_an_id_or_alias_with_osv() {
    let github = recorded_advisories(1)
        .iter()
        .chain(&recorded_advisories(2))
        .map(|advisory| row(advisory, &Selection::All).expect("row"))
        .collect::<Vec<_>>();
    let by_alias = SecurityAdvisory {
        id: "PYSEC-1".into(),
        aliases: vec![github[1].id.clone()],
        ..SecurityAdvisory::default()
    };
    let unrelated = SecurityAdvisory {
        id: "OSV-0".into(),
        ..SecurityAdvisory::default()
    };
    let twin = twins(&[&by_alias, &unrelated], github.clone());
    assert_eq!(twin.len(), 1);
    assert_eq!(twin[0].id, github[1].id);
}

#[tokio::test]
async fn the_repository_row_merges_github_with_osv_by_id_and_lists_each_advisory_once() {
    let github = two_pages();
    let first = recorded_advisories(1)[0].ghsa_id.clone();
    let http = RouteHttp::new().json(
        &format!("/v1/vulns/{first}"),
        json!({"id": first, "aliases": ["CVE-2026-4800"], "summary": "from OSV", "modified": "2026-01-01T00:00:00Z"}),
    );
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::tools::security_check::tests::default_config();
    let data = crate::tools::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("row");
    let expected = recorded_advisories(1).len() + recorded_advisories(2).len();
    let rows = data["advisories"].as_array().expect("advisories");
    assert_eq!(rows.len(), expected, "{data}");
    let merged = rows.iter().find(|row| row["id"] == first).expect("merged");
    assert_eq!(
        merged["summary"], "from OSV",
        "OSV's record leads: {merged}"
    );
    assert!(
        merged
            .get("aliases")
            .is_none_or(|aliases| !aliases.to_string().contains(&first)),
        "an advisory is no alias of itself: {merged}"
    );
    assert!(data.get("isPartial").is_none(), "{data}");
}

#[tokio::test]
async fn a_refused_page_leaves_the_row_partial_with_the_reset_time_and_no_advisory_list() {
    let github = Double::repo(found()).page_read(
        None,
        AdvisoryRead::Failed(RepoFailure {
            code: ErrorCode::RateLimited,
            why: "GitHub rate limit reached; it resets at 2999-01-01T00:00:00Z.".into(),
        }),
    );
    let http = RouteHttp::new();
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::tools::security_check::tests::default_config();
    let data = crate::tools::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("row");
    assert_eq!(data["sources"][0]["status"], "partial", "{data}");
    assert_eq!(data["sources"][0]["errorCode"], "rateLimited", "{data}");
    assert!(
        data["sources"][0]["why"]
            .as_str()
            .is_some_and(|why| why.contains("resets at 2999")),
        "{data}"
    );
    assert!(data.get("advisories").is_none(), "{data}");
    assert!(
        data["partialReasons"].to_string().contains("sourceFailed"),
        "{data}"
    );
    assert!(
        data["repository"]["stars"].is_number(),
        "repository facts stay: {data}"
    );
}

#[tokio::test]
async fn a_later_page_that_fails_keeps_the_advisories_read_and_marks_github_partial() {
    let github = Double::repo(found())
        .page(None, recorded_advisories(1), Some("cursor-2"))
        .page_read(
            Some("cursor-2"),
            AdvisoryRead::Failed(RepoFailure {
                code: ErrorCode::RateLimited,
                why: "resets at 2999-01-01T00:00:00Z".into(),
            }),
        );
    let http = RouteHttp::new();
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::tools::security_check::tests::default_config();
    let data = crate::tools::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("row");
    let rows = data["advisories"]
        .as_array()
        .expect("the rows already read");
    assert_eq!(rows.len(), recorded_advisories(1).len(), "{data}");
    assert_eq!(data["sources"][0]["status"], "partial", "{data}");
    assert_eq!(data["sources"][0]["errorCode"], "rateLimited", "{data}");
    let why = data["sources"][0]["why"].as_str().unwrap_or_default();
    assert!(
        why.contains("resets at 2999") && why.contains("not read"),
        "{data}"
    );
    assert!(
        !data["answer"]
            .as_str()
            .unwrap_or_default()
            .contains("No published"),
        "the rows read are not the whole list: {data}"
    );
    assert_eq!(data["isPartial"], true, "{data}");
}

#[tokio::test]
async fn a_private_repositorys_advisory_ids_are_never_sent_to_osv_and_the_row_says_so() {
    let private = RepoRead::Found(RepoFacts {
        visibility: Visibility::NotPublic,
        ..RepoFacts::default()
    });
    let github = Double::repo(private).page(None, recorded_advisories(1), None);
    let http = RouteHttp::new();
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::tools::security_check::tests::default_config();
    let data = crate::tools::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("row");
    assert!(
        http.targets().is_empty(),
        "nothing of a private repository goes to OSV: {:?}",
        http.targets()
    );
    let rows = data["advisories"].as_array().expect("GitHub's own rows");
    assert_eq!(rows.len(), recorded_advisories(1).len(), "{data}");
    let why = data["warnings"].to_string();
    assert!(
        why.contains("not asked") && why.contains("private"),
        "the skipped merge is stated with its reason: {data}"
    );
}

/// A `RepoFacts` built without a visibility must never read as public: a
/// public default would send advisory ids to OSV for a repository GitHub never
/// called public.
#[test]
fn a_visibility_nobody_set_is_not_one_a_third_party_may_hear() {
    assert_eq!(Visibility::default(), Visibility::Unknown);
    assert!(!RepoFacts::default().visibility.third_party_may_hear());
}

#[test]
fn github_stating_neither_private_nor_visibility_is_an_unknown_visibility() {
    for (private, text, expected) in [
        (Some(true), "", Visibility::NotPublic),
        (Some(true), "public", Visibility::NotPublic),
        (None, "private", Visibility::NotPublic),
        (Some(false), "internal", Visibility::NotPublic),
        (Some(false), "", Visibility::Public),
        (Some(false), "public", Visibility::Public),
        (None, "public", Visibility::Public),
        (None, "", Visibility::Unknown),
        (None, "something-new", Visibility::Unknown),
    ] {
        assert_eq!(
            Visibility::of(private, text),
            expected,
            "{private:?} {text:?}"
        );
    }
}

#[tokio::test]
async fn a_repository_whose_visibility_github_did_not_state_never_has_advisory_ids_sent_to_osv() {
    let unknown = RepoRead::Found(RepoFacts {
        visibility: Visibility::Unknown,
        ..RepoFacts::default()
    });
    let github = Double::repo(unknown).page(None, recorded_advisories(1), None);
    let http = RouteHttp::new();
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::tools::security_check::tests::default_config();
    let data = crate::tools::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("row");
    assert!(
        http.targets().is_empty(),
        "an unknown visibility is treated as private: {:?}",
        http.targets()
    );
    let rows = data["advisories"].as_array().expect("GitHub's own rows");
    assert_eq!(rows.len(), recorded_advisories(1).len(), "{data}");
    let why = data["warnings"].to_string();
    assert!(
        why.contains("did not say") && why.contains("not asked"),
        "{data}"
    );
}

#[tokio::test]
async fn a_public_repositorys_advisories_are_still_merged_with_osv() {
    let github = two_pages();
    let first = recorded_advisories(1)[0].ghsa_id.clone();
    let http = RouteHttp::new().json(
        &format!("/v1/vulns/{first}"),
        json!({"id": first, "summary": "from OSV", "modified": "2026-01-01T00:00:00Z"}),
    );
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::tools::security_check::tests::default_config();
    let data = crate::tools::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("row");
    assert!(!http.targets().is_empty(), "{data}");
    assert!(data.get("warnings").is_none(), "{data}");
}

/// A double that counts the requests in flight together: each answers after
/// the other tasks of the call had their turn.
struct Overlap {
    inner: RouteHttp,
    now: std::sync::atomic::AtomicUsize,
    most: std::sync::atomic::AtomicUsize,
}

impl crate::providers::artifact::ArtifactHttp for Overlap {
    fn get<'a>(
        &'a self,
        request: crate::providers::artifact::ArtifactHttpRequest,
        budget: &'a octocode_io::budget::RequestBudget,
    ) -> crate::providers::artifact::ArtifactHttpFuture<'a> {
        use std::sync::atomic::Ordering::SeqCst;
        Box::pin(async move {
            let together = self.now.fetch_add(1, SeqCst) + 1;
            self.most.fetch_max(together, SeqCst);
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            let answer = self.inner.get(request, budget).await;
            self.now.fetch_sub(1, SeqCst);
            answer
        })
    }
}

#[tokio::test]
async fn the_advisories_are_asked_of_osv_together_not_one_after_another() {
    use std::sync::atomic::Ordering::SeqCst;
    let github = two_pages();
    let http = Overlap {
        inner: RouteHttp::new(),
        now: 0.into(),
        most: 0.into(),
    };
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::tools::security_check::tests::default_config();
    let net = super::Net {
        http: &http,
        budget: &budget,
        answers: None,
        reused: &reused,
        config: &config,
        github: Some(&github),
        osv_api: crate::execution::RuntimeLimits::DEFAULT.osv_api,
    };
    let data = crate::tools::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        &net,
    )
    .await
    .expect("row");
    assert!(
        data["advisories"]
            .as_array()
            .is_some_and(|rows| rows.len() > 1)
    );
    assert!(
        http.most.load(SeqCst) > 1,
        "each advisory waited for the one before it: {} at once",
        http.most.load(SeqCst)
    );
}

#[tokio::test]
async fn an_osv_failure_for_one_advisory_keeps_github_rows_and_names_the_unread_ids() {
    let github = two_pages();
    let (read, failing) = (
        recorded_advisories(1)[0].ghsa_id.clone(),
        recorded_advisories(1)[1].ghsa_id.clone(),
    );
    let http = RouteHttp::new()
        .json(
            &format!("/v1/vulns/{read}"),
            json!({"id": read, "summary": "from OSV", "modified": "2026-01-01T00:00:00Z"}),
        )
        .status(&format!("/v1/vulns/{failing}"), 503, json!({}));
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::tools::security_check::tests::default_config();
    let data = crate::tools::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("GitHub's list survives OSV's failure");
    let expected = recorded_advisories(1).len() + recorded_advisories(2).len();
    let rows = data["advisories"].as_array().expect("advisories");
    assert_eq!(rows.len(), expected, "every GitHub row stays: {data}");
    assert_eq!(
        rows.iter().find(|row| row["id"] == read).expect("read")["summary"],
        "from OSV",
        "{data}"
    );
    let osv = data["sources"]
        .as_array()
        .and_then(|sources| sources.iter().find(|entry| entry["source"] == "osv"))
        .unwrap_or_else(|| panic!("an OSV entry: {data}"));
    assert_eq!(osv["status"], "partial", "{data}");
    let why = osv["why"].as_str().unwrap_or_default();
    assert!(why.contains(&failing) && !why.contains(&read), "{data}");
    assert_eq!(data["isPartial"], true, "{data}");
    assert!(
        data["partialReasons"].to_string().contains("sourceFailed"),
        "{data}"
    );
}
