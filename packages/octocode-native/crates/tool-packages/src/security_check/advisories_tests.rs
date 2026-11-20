//! Repository advisories: every page walked once, read as OSV-shaped rows,
//! merged with OSV by id, and filtered to a monorepo's package.
use super::*;
use crate::artifact::test_http::{RouteHttp, test_budget};
use crate::security_check::ecosystem::Ecosystem;
use crate::security_check::github_double::{Double, net, recorded_advisories};
use crate::security_check::repository::{RepoFacts, RepoRead, Visibility};
use octocode_runtime_base::providers::saved_answers::ReusedSince;
use octocode_test_support::json_fixture;
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
    let config = crate::security_check::tests::default_config();
    let data = crate::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("row");
    let expected = recorded_advisories(1).len() + recorded_advisories(2).len();
    let rows = data["advisories"].as_array().expect("advisories");
    assert_eq!(rows.len(), expected, "{data}");
    let merged = rows.iter().find(|row| row["id"] == first).expect("merged");
    assert_eq!(
        merged.get("link"),
        None,
        "OSV's page is derivable from the id: {merged}"
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
    let config = crate::security_check::tests::default_config();
    let data = crate::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("row");
    // No advisory was read, so the source did not answer; a rate limit may clear.
    assert_eq!(data["sources"][0]["status"], "failed", "{data}");
    assert_eq!(data["sources"][0]["errorCode"], "rateLimited", "{data}");
    assert_eq!(data["retryable"], true, "{data}");
    // The entry says what was left unread; the reset time is the rate
    // limit's, said once for the response.
    assert_eq!(data["sources"][0]["why"], "Advisories not read.", "{data}");
    assert!(
        data["notices"].to_string().contains("resets at 2999"),
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
    let config = crate::security_check::tests::default_config();
    let data = crate::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
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
    assert!(why.contains("the rest not read"), "{data}");
    assert!(
        data["notices"].to_string().contains("resets at 2999"),
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
    let config = crate::security_check::tests::default_config();
    let data = crate::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
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
    // An `advisory` call sends its id to OSV: none is offered for a
    // repository not known to be public.
    assert!(
        !data["related"].to_string().contains("\"advisory\""),
        "{data}"
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
    let config = crate::security_check::tests::default_config();
    let data = crate::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
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
    let config = crate::security_check::tests::default_config();
    let data = crate::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("row");
    assert!(!http.targets().is_empty(), "{data}");
    assert!(data.get("warnings").is_none(), "{data}");
    // OSV was asked and answered: the row names it among its sources.
    assert_eq!(
        osv_entry(&data),
        json_fixture(r#"{"source": "osv", "status": "found"}"#),
        "{data}"
    );
}

fn osv_entry(data: &Value) -> Value {
    data["sources"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|entry| entry["source"] == "osv")
        .cloned()
        .unwrap_or(Value::Null)
}

/// OSV asked for every advisory id and keeping none of them is a source that
/// was asked: `noneKnown`, never left off the row.
#[tokio::test]
async fn osv_asked_for_a_repositorys_advisories_is_a_source_even_when_it_knows_none() {
    let github = two_pages();
    let http = RouteHttp::new();
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::security_check::tests::default_config();
    let data = crate::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("row");
    assert!(!http.targets().is_empty(), "OSV was asked: {data}");
    assert_eq!(
        osv_entry(&data),
        json_fixture(r#"{"source": "osv", "status": "noneKnown"}"#),
        "{data}"
    );
}

/// A double that counts the requests in flight together: each answers after
/// the other tasks of the call had their turn.
struct Overlap {
    inner: RouteHttp,
    now: std::sync::atomic::AtomicUsize,
    most: std::sync::atomic::AtomicUsize,
}

impl crate::artifact::ArtifactHttp for Overlap {
    fn get<'a>(
        &'a self,
        request: crate::artifact::ArtifactHttpRequest,
        budget: &'a octocode_io::budget::RequestBudget,
    ) -> crate::artifact::ArtifactHttpFuture<'a> {
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
    let config = crate::security_check::tests::default_config();
    let net = super::Net {
        http: &http,
        budget: &budget,
        answers: None,
        reused: &reused,
        config: &config,
        github: Some(&github),
        osv_api: octocode_runtime_base::execution::RuntimeLimits::DEFAULT.osv_api,
        share: &crate::artifact::CallShare::new(),
    };
    let data = crate::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
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
    let config = crate::security_check::tests::default_config();
    let data = crate::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("GitHub's list survives OSV's failure");
    let expected = recorded_advisories(1).len() + recorded_advisories(2).len();
    let rows = data["advisories"].as_array().expect("advisories");
    assert_eq!(rows.len(), expected, "every GitHub row stays: {data}");
    assert_eq!(
        rows.iter()
            .find(|row| row["id"] == read)
            .expect("read")
            .get("link"),
        None,
        "{data}"
    );
    let osv = data["sources"]
        .as_array()
        .and_then(|sources| sources.iter().find(|entry| entry["source"] == "osv"))
        .unwrap_or_else(|| panic!("an OSV entry: {data}"));
    assert_eq!(osv["status"], "partial", "{data}");
    let why = osv["why"].as_str().unwrap_or_default();
    assert!(why.contains(&failing) && !why.contains(&read), "{data}");
    let why_once = |data: &Value, why: &str| {
        let repeated = data["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|warning| warning.as_str() == Some(why))
            .count();
        assert_eq!(
            repeated, 0,
            "the source's why is stated once, on it: {data}"
        );
    };
    why_once(&data, why);
    assert_eq!(data["isPartial"], true, "{data}");
    assert!(
        data["partialReasons"].to_string().contains("sourceFailed"),
        "{data}"
    );
}

/// What GitHub publishes beyond OSV's shape reaches the row: each CVSS vector
/// with GitHub's numeric score under `score` (never the vector), EPSS, the
/// publisher and the credits; the record's API address is its link's and is
/// not restated as a reference.
#[test]
fn a_github_advisory_row_keeps_its_scores_epss_publisher_and_credits() {
    let advisory: RepositoryAdvisory = serde_json::from_value(json_fixture(
        r#"{
            "ghsa_id": "GHSA-aaaa-bbbb-cccc",
            "url": "https://api.github.com/repos/o/r/security-advisories/GHSA-aaaa-bbbb-cccc",
            "html_url": "https://github.com/o/r/security/advisories/GHSA-aaaa-bbbb-cccc",
            "description": "Whole text.",
            "publisher": {"login": "maintainer"},
            "credits": [{"login": "finder", "type": "finder"}],
            "cvss": {
                "vector_string": "CVSS:3.1/AV:N/AC:L/PR:N/UI:R/S:U/C:L/I:L/A:N",
                "score": 5.4
            },
            "cvss_severities": {"cvss_v4": {"vector_string": null, "score": 0.0}},
            "epss": {"percentage": 0.01965, "percentile": 0.80029},
            "cwes": [{"cwe_id": "CWE-79", "name": "XSS"}],
            "vulnerabilities": [
                {
                    "package": {"ecosystem": "npm", "name": "x"},
                    "vulnerable_version_range": "< 2.0.0",
                    "first_patched_version": "2.0.0"
                }
            ]
        }"#,
    ))
    .expect("advisory");
    let row = row(&advisory, &Selection::All).expect("row");
    assert_eq!(
        row.cvss,
        [json_fixture(
            r#"{
                "type": "CVSS_V3",
                "vector": "CVSS:3.1/AV:N/AC:L/PR:N/UI:R/S:U/C:L/I:L/A:N",
                "score": 5.4
            }"#
        )]
    );
    assert_eq!(
        row.epss,
        Some(json_fixture(
            r#"{"percentage": 0.01965, "percentile": 0.80029}"#
        ))
    );
    assert_eq!(row.publisher.as_deref(), Some("maintainer"));
    assert_eq!(
        row.credits,
        [json_fixture(r#"{"login": "finder", "type": "finder"}"#)]
    );
    assert_eq!(row.details.as_deref(), Some("Whole text."));
    assert_eq!(row.cwes, ["CWE-79"]);
    assert_eq!(row.fixed_in, ["2.0.0"]);
    // The record's API address is its link's: the link states it.
    assert_eq!(
        row.link,
        "https://github.com/o/r/security/advisories/GHSA-aaaa-bbbb-cccc"
    );
    assert!(row.references.is_empty(), "{:?}", row.references);
    // The list row leaves EPSS to the record's own row.
    assert!(row.brief().get("epss").is_none());
    assert_eq!(
        serde_json::to_value(&row).expect("row")["epss"]["percentile"],
        0.80029
    );
}

/// A public repository's advisory OSV keeps no record of is one ready
/// `advisory` call away, with `repo`, so GitHub's own record of it is read
/// whole (RV-SEC F6).
#[tokio::test]
async fn a_repository_row_offers_each_advisory_osv_lacks_with_its_repo() {
    let github = two_pages();
    let first = recorded_advisories(1)[0].ghsa_id.clone();
    let http = RouteHttp::new().json(
        &format!("/v1/vulns/{first}"),
        json!({"id": first, "summary": "from OSV", "modified": "2026-01-01T00:00:00Z"}),
    );
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::security_check::tests::default_config();
    let data = crate::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("row");
    let rows = data["related"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|call| {
            call["input"]["queries"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter(|row| row["operation"] == "advisory")
        .collect::<Vec<_>>();
    assert!(!rows.is_empty(), "{data}");
    assert!(
        rows.iter().all(|row| row["repo"] == "lodash/lodash"),
        "{rows:?}"
    );
    assert!(
        rows.iter().all(|row| row["id"] != json!(first)),
        "OSV's own record needs no repository read: {rows:?}"
    );
}

/// GitHub's class of a record reaches its row as `githubType`, in the whole
/// row and the brief one; a class the contract does not name stays out of the
/// field (ASEC-2).
#[test]
fn a_github_record_states_its_class_as_github_type() {
    let of = |kind: &str| {
        let advisory: RepositoryAdvisory = serde_json::from_value(json!({
            "ghsa_id": "GHSA-aaaa-bbbb-cccc", "type": kind,
            "vulnerabilities": [{"package": {"ecosystem": "npm", "name": "x"},
                "vulnerable_version_range": "< 2.0.0"}],
        }))
        .expect("advisory");
        row(&advisory, &Selection::All).expect("row")
    };
    for kind in ["reviewed", "unreviewed", "malware"] {
        let advisory = of(kind);
        assert_eq!(advisory.github_type.as_deref(), Some(kind));
        // A list row states the class only when it is a caution.
        assert_eq!(
            advisory.brief().get("githubType"),
            (kind != "reviewed").then(|| json!(kind)).as_ref()
        );
        assert_eq!(
            serde_json::to_value(&advisory).expect("row")["githubType"],
            kind
        );
    }
    assert_eq!(of("something else").github_type, None);
}

/// The repository row counts only published advisories GitHub has not
/// withdrawn, and says how many it withdrew (check C D3).
#[tokio::test]
async fn a_repository_row_never_counts_a_withdrawn_advisory_as_published() {
    let mut listed = recorded_advisories(1);
    listed[0].withdrawn_at = Some("2025-12-02T15:11:19Z".into());
    let total = listed.len();
    let github = Double::repo(found()).page(None, listed, None);
    let http = RouteHttp::new();
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::security_check::tests::default_config();
    let data = crate::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("row");
    let answer = data["answer"].as_str().unwrap_or_default();
    let live = total - 1;
    let expected = if live == 1 {
        "1 published repository advisory; 1 withdrawn, not counted.".to_owned()
    } else {
        format!("{live} published repository advisories; 1 withdrawn, not counted.")
    };
    assert!(answer.ends_with(&expected), "{answer}");
}

/// A withdrawn duplicate that shares its CVE with a live repository advisory
/// is one advisory with it: counted once, as published.
#[tokio::test]
async fn a_withdrawn_duplicate_never_hides_a_live_repository_advisory() {
    let mut listed = recorded_advisories(1);
    listed.truncate(1);
    let mut duplicate = listed[0].clone();
    duplicate.ghsa_id = "GHSA-dupl-xxxx-wwww".into();
    duplicate.withdrawn_at = Some("2025-12-02T15:11:19Z".into());
    listed.push(duplicate);
    let github = Double::repo(found()).page(None, listed, None);
    let http = RouteHttp::new();
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::security_check::tests::default_config();
    let data = crate::security_check::repository::run(
        "lodash/lodash",
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
        &net(&http, &budget, &reused, &config, &github),
    )
    .await
    .expect("row");
    let answer = data["answer"].as_str().unwrap_or_default();
    assert!(
        answer.ends_with("1 published repository advisory."),
        "{answer}"
    );
    let rows = data["advisories"].as_array().expect("advisories");
    assert_eq!(rows.len(), 1, "{data}");
    assert!(rows[0].get("withdrawn").is_none(), "{data}");
}

/// A maintainer writes patched versions free hand, as a list or as ranges:
/// each fix is stated once, as the version OSV's `fixed` event names, and a
/// range's upper bound is no fix, so no entry is a range.
#[test]
fn patched_ranges_state_each_fix_once_as_a_version() {
    let fixes = |patched: &str, first: Option<&str>| {
        let advisory: RepositoryAdvisory = serde_json::from_value(json!({
            "ghsa_id": "GHSA-aaaa-bbbb-cccc",
            "summary": "s",
            "vulnerabilities": [{"package": {"ecosystem": "npm", "name": "x"},
                "vulnerable_version_range": "< 6.2.6", "patched_versions": patched,
                "first_patched_version": first}],
        }))
        .expect("advisory");
        row(&advisory, &Selection::All).expect("row").fixed_in
    };
    assert_eq!(
        fixes(
            ">=6.2.6, >=6.1.5, <6.2.0, >= 6.0.15 < 6.1.0, 5.4.18",
            Some("6.2.6")
        ),
        ["6.2.6", "6.1.5", "6.0.15", "5.4.18"]
    );
    assert_eq!(fixes("2.0.0, 1.5.3", None), ["2.0.0", "1.5.3"]);
    // Only a fixed version is listed: a bound that names none is left to the
    // record's own text on the `advisory` row.
    assert!(fixes("< 2.0.0", None).is_empty());
    assert_eq!(fixes(">1.9.9, 2.0.0", None), ["2.0.0"]);
}

/// A reference that is the API address of the row's own link (a global or a
/// repository advisory) is left out; GitHub's API address of another record
/// stays, as the link does not state it.
#[test]
fn a_reference_is_left_out_only_when_it_is_the_links_api_address() {
    let shaped = |html: Option<&str>, url: &str| -> SecurityAdvisory {
        let mut advisory = json!({
            "ghsa_id": "GHSA-aaaa-bbbb-cccc",
            "url": url,
            "references": ["https://example.test/fix"],
            "vulnerabilities": [{"package": {"ecosystem": "npm", "name": "x"},
                "vulnerable_version_range": "< 2.0.0"}],
        });
        if let Some(html) = html {
            advisory["html_url"] = json!(html);
        }
        row(
            &serde_json::from_value::<RepositoryAdvisory>(advisory).expect("advisory"),
            &Selection::All,
        )
        .expect("row")
    };
    let urls = |row: &SecurityAdvisory| {
        row.references
            .iter()
            .filter_map(|reference| reference["url"].as_str().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    let global = shaped(
        Some("https://github.com/advisories/GHSA-aaaa-bbbb-cccc"),
        "https://api.github.com/advisories/GHSA-aaaa-bbbb-cccc",
    );
    assert_eq!(urls(&global), ["https://example.test/fix"]);
    let unlinked = shaped(
        None,
        "https://api.github.com/advisories/GHSA-aaaa-bbbb-cccc",
    );
    assert_eq!(
        unlinked.link,
        "https://github.com/advisories/GHSA-aaaa-bbbb-cccc"
    );
    assert_eq!(urls(&unlinked), ["https://example.test/fix"]);
    let elsewhere = shaped(
        None,
        "https://api.github.com/repos/o/r/security-advisories/GHSA-aaaa-bbbb-cccc",
    );
    assert_eq!(
        urls(&elsewhere),
        [
            "https://example.test/fix",
            "https://api.github.com/repos/o/r/security-advisories/GHSA-aaaa-bbbb-cccc"
        ]
    );
}

/// GitHub lists its references as bare URLs: the row keeps each as given,
/// with no type GitHub did not state.
#[test]
fn github_references_carry_no_type_github_did_not_state() {
    let advisory = json!({
        "ghsa_id": "GHSA-aaaa-bbbb-cccc",
        "html_url": "https://github.com/advisories/GHSA-aaaa-bbbb-cccc",
        "references": ["https://example.test/fix", "https://example.test/report"],
        "vulnerabilities": [{"package": {"ecosystem": "npm", "name": "x"},
            "vulnerable_version_range": "< 2.0.0"}],
    });
    let row = row(
        &serde_json::from_value::<RepositoryAdvisory>(advisory).expect("advisory"),
        &Selection::All,
    )
    .expect("row");
    assert_eq!(
        row.references,
        [
            json!({"url": "https://example.test/fix"}),
            json!({"url": "https://example.test/report"})
        ]
    );
}
