//! The OSV source against a recorded double: wire, states and paging.
use super::*;
use crate::providers::artifact::query::artifact_type::osv_ecosystem;
use crate::providers::artifact::test_http::{RouteHttp, test_budget};
use crate::providers::saved_answers::ReusedSince;
use crate::tools::security_check::ecosystem::osv_name;
use crate::tools::security_check::osv_double::OsvPages;
use octocode_contract_types::ArtifactType;
use serde_json::json;

fn package<'a>(kind: ArtifactType, name: &'a str, version: Option<&'a str>) -> Ask<'a> {
    Ask::Package {
        ecosystem: osv_ecosystem(kind),
        release: None,
        name,
        version,
        import: None,
        by_name: false,
    }
}

async fn on(
    http: &RouteHttp,
    query: &Ask<'_>,
    start: Option<&str>,
) -> Result<SourceResult, ArtifactError> {
    let budget = test_budget();
    let reused = ReusedSince::default();
    let config = crate::tools::security_check::tests::default_config();
    let net = Net {
        http,
        budget: &budget,
        answers: None,
        reused: &reused,
        config: &config,
        github: None,
        osv_api: crate::execution::RuntimeLimits::DEFAULT.osv_api,
    };
    // The selection a package row asks with, as `ask` builds it.
    let names;
    let selection = match query {
        Ask::Package {
            ecosystem, name, ..
        } => {
            names = [(*name).to_owned()];
            Selection::Named(*ecosystem, &names, None)
        }
        Ask::Commit(_) => Selection::All,
    };
    walk(&net, query, &selection, start).await
}

async fn run(http: &RouteHttp, start: Option<&str>) -> Result<SourceResult, ArtifactError> {
    on(
        http,
        &package(ArtifactType::Npm, "lodash", Some("4.17.20")),
        start,
    )
    .await
}

fn record(id: &str) -> Value {
    json!({"id": id, "schema_version": "1.6.0", "affected": [
        {"package": {"name": "lodash", "ecosystem": "npm", "purl": "pkg:npm/lodash"}}
    ]})
}

/// The saved walk a stopped row carries, read back.
fn saved_token(resume: &crate::tools::security_check::source::Resume) -> String {
    let saved: Value = serde_json::from_str(&resume.token).expect("saved walk");
    saved["token"].as_str().expect("token").to_owned()
}

fn ids(walk: &SourceResult) -> Vec<&str> {
    walk.advisories.iter().map(|row| row.id.as_str()).collect()
}

#[tokio::test]
async fn the_request_holds_only_the_package_the_version_and_the_token() {
    let http = RouteHttp::new().osv_pages(vec![
        (
            200,
            json!({"vulns": [record("A")], "next_page_token": "t1"}),
        ),
        (200, json!({})),
    ]);
    run(&http, None).await.expect("walk");
    let bodies = http.osv_queries();
    assert_eq!(
        bodies,
        vec![
            json!({"package": {"name": "lodash", "ecosystem": "npm"}, "version": "4.17.20"}),
            json!({"package": {"name": "lodash", "ecosystem": "npm"}, "version": "4.17.20", "page_token": "t1"}),
        ]
    );
    assert!(http.seen().iter().all(|seen| !seen.authorized));
    assert_eq!(
        http.seen()[0].url.as_str(),
        "https://api.osv.dev/v1/querybatch"
    );
    // A name-only query carries no version key at all.
    let http = RouteHttp::new().osv_pages(vec![(200, json!({}))]);
    on(&http, &package(ArtifactType::Pypi, "requests", None), None)
        .await
        .expect("walk");
    assert_eq!(
        http.osv_queries(),
        vec![json!({"package": {"name": "requests", "ecosystem": "PyPI"}})]
    );
}

#[test]
fn every_registry_uses_osvs_case_sensitive_ecosystem_name() {
    for (kind, name) in [
        (ArtifactType::Npm, "npm"),
        (ArtifactType::Pypi, "PyPI"),
        (ArtifactType::Crates, "crates.io"),
        (ArtifactType::Maven, "Maven"),
        (ArtifactType::Nuget, "NuGet"),
        (ArtifactType::Go, "Go"),
        (ArtifactType::Packagist, "Packagist"),
        (ArtifactType::Rubygems, "RubyGems"),
    ] {
        assert_eq!(osv_name(osv_ecosystem(kind), None), name);
    }
}

/// `{}` finishes the walk with nothing: the only empty answer.
#[tokio::test]
async fn an_empty_object_is_a_finished_walk_with_no_records() {
    let http = RouteHttp::new().osv_pages(vec![(200, json!({}))]);
    let done = run(&http, None).await.expect("walk");
    assert!(done.advisories.is_empty());
    assert!(done.resume.is_none());
}

/// "The response might contain only the next_page_token": a page with no
/// `vulns` and a token is not empty, so the walk goes on.
#[tokio::test]
async fn a_token_only_page_walks_on() {
    let http = RouteHttp::new().osv_pages(vec![
        (200, json!({"next_page_token": "t1"})),
        (200, json!({"next_page_token": "t2"})),
        (200, json!({"vulns": [record("A")]})),
    ]);
    let done = run(&http, None).await.expect("walk");
    assert_eq!(ids(&done), ["A"]);
    assert_eq!(http.osv_queries().len(), 3);
    assert!(done.resume.is_none());
}

#[tokio::test]
async fn pages_join_in_order_exactly_once() {
    let http = RouteHttp::new().osv_pages(vec![
        (
            200,
            json!({"vulns": [record("A"), record("B")], "next_page_token": "t1"}),
        ),
        (200, json!({"vulns": [record("C")]})),
    ]);
    let done = run(&http, None).await.expect("walk");
    assert_eq!(ids(&done), ["A", "B", "C"]);
}

/// A later page lost to a transient failure saves what was read with the exact
/// token that reaches the rest: the row lists nothing until the walk finished,
/// and then every record once, with no gap or repeat.
#[tokio::test]
async fn a_walk_stopped_midway_resumes_with_no_gap_or_repeat() {
    let http = RouteHttp::new().osv_pages(vec![
        (
            200,
            json!({"vulns": [record("A")], "next_page_token": "t1"}),
        ),
        (503, json!({})),
    ]);
    let stopped = run(&http, None).await.expect("partial");
    assert!(stopped.advisories.is_empty(), "listed only once finished");
    let resume = stopped.resume.expect("a token to resume from");
    assert_eq!(saved_token(&resume), "t1");
    assert!(resume.reason.contains("OSV"), "{}", resume.reason);
    let http = RouteHttp::new().osv_pages(vec![(200, json!({"vulns": [record("B")]}))]);
    let rest = run(&http, Some(&resume.token)).await.expect("rest");
    assert_eq!(ids(&rest), ["A", "B"]);
    assert_eq!(http.osv_queries()[0]["page_token"], "t1");
}

/// A token-only page at the deadline is not empty: no records, and a token.
#[tokio::test]
async fn a_token_only_page_at_the_deadline_is_partial_not_empty() {
    let http = RouteHttp::new()
        .osv_pages(vec![(200, json!({"next_page_token": "t1"}))])
        .post_fails_from(1, ErrorCode::Timeout);
    let stopped = run(&http, None).await.expect("partial");
    assert!(stopped.advisories.is_empty());
    assert_eq!(saved_token(&stopped.resume.expect("resume")), "t1");
}

#[tokio::test]
async fn failures_name_osv_and_never_read_as_clean() {
    for status in [429, 500, 503, 400] {
        let http = RouteHttp::new().osv_pages(vec![(status, json!({"code": 3, "message": "why"}))]);
        let error = run(&http, None).await.expect_err("failure");
        assert_eq!(error.status, Some(status));
        assert_eq!(
            error.code,
            ProviderErrorKind::from_status(status).code(),
            "{status}"
        );
        for spelled in ["OSV", "lodash@4.17.20", "not checked"] {
            assert!(error.message.contains(spelled), "{}", error.message);
        }
        assert!(!error.message.contains("registry"), "{}", error.message);
    }
    let http = RouteHttp::new().post_fails_from(0, ErrorCode::ProviderError);
    let error = run(&http, None).await.expect_err("transport");
    assert!(
        error.message.starts_with("OSV request failed"),
        "{}",
        error.message
    );
    assert!(!error.message.contains("registry"), "{}", error.message);
}

#[tokio::test]
async fn a_malformed_answer_fails_whole() {
    for body in [
        b"<html>nope</html>".to_vec(),
        b"[]".to_vec(),
        br#"{"vulns":[{"id":"A"}]}"#.to_vec(),
        br#"{"results":{}}"#.to_vec(),
        br#"{"results":[]}"#.to_vec(),
        br#"{"results":[{},{}]}"#.to_vec(),
        br#"{"results":[7]}"#.to_vec(),
        br#"{"results":[{"vulns":{"id":"A"}}]}"#.to_vec(),
        br#"{"results":[{"vulns":[{"summary":"no id"}]}]}"#.to_vec(),
        br#"{"results":[{"vulns":[{"id":"A"}],"next_page_token":7}]}"#.to_vec(),
    ] {
        let http = RouteHttp::new().post_raw(body.clone());
        let error = run(&http, None).await.expect_err("malformed");
        assert_eq!(error.code, ErrorCode::ProviderError, "{body:?}");
        assert!(error.message.contains("not checked"), "{}", error.message);
    }
    // A record OSV states in a schema this tool does not read fails the whole answer.
    let http = RouteHttp::new().osv_pages(vec![(
        200,
        json!({"vulns": [{"id": "A", "schema_version": "2.0.0"}]}),
    )]);
    run(&http, None).await.expect_err("fail whole");
    // So does a record with no id.
    let http = RouteHttp::new()
        .json("/v1/vulns/A", json!({"summary": "no id"}))
        .post_raw(br#"{"results":[{"vulns":[{"id":"A"}]}]}"#.to_vec());
    run(&http, None).await.expect_err("fail whole");
    // A page lost to a malformed answer after the first stops the walk with
    // the token that asks it again: nothing read is dropped.
    let http = RouteHttp::new().osv_pages(vec![
        (
            200,
            json!({"vulns": [record("A")], "next_page_token": "t1"}),
        ),
        (200, json!({"results": [{"vulns": [{"summary": "no id"}]}]})),
    ]);
    let stopped = run(&http, None).await.expect("partial");
    assert_eq!(saved_token(&stopped.resume.expect("resume")), "t1");
    // A token that repeats would never end.
    let http = RouteHttp::new().osv_pages(vec![
        (200, json!({"next_page_token": "t1"})),
        (200, json!({"next_page_token": "t1"})),
    ]);
    run(&http, None).await.expect_err("a repeated token");
}

#[tokio::test]
async fn a_non_adjacent_page_token_cycle_fails_before_requesting_it_again() {
    let http = RouteHttp::new().osv_pages(vec![
        (200, json!({"next_page_token": "t1"})),
        (200, json!({"next_page_token": "t2"})),
        (200, json!({"next_page_token": "t1"})),
        (200, json!({})),
    ]);
    let error = run(&http, None).await.expect_err("a repeated token");
    assert_eq!(error.code, ErrorCode::ProviderError);
    assert!(error.message.contains("token"), "{}", error.message);
    assert_eq!(http.osv_queries().len(), 3, "no repeated provider request");
}

#[tokio::test]
async fn a_page_token_cycle_across_a_resumed_walk_fails() {
    let http = RouteHttp::new().osv_pages(vec![
        (200, json!({"next_page_token": "t1"})),
        (200, json!({"next_page_token": "t2"})),
        (503, json!({})),
    ]);
    let stopped = run(&http, None).await.expect("partial");
    let resume = stopped.resume.expect("resumable provider failure");
    let http = RouteHttp::new().osv_pages(vec![
        (200, json!({"next_page_token": "t1"})),
        (200, json!({})),
    ]);
    let error = run(&http, Some(&resume.token))
        .await
        .expect_err("a token from an earlier page");
    assert_eq!(error.code, ErrorCode::ProviderError);
    assert_eq!(http.osv_queries().len(), 1);
}

/// A rejected token does not resume: it fails, so the row is an error.
#[tokio::test]
async fn a_rejected_token_fails_instead_of_resuming() {
    let http = RouteHttp::new().osv_pages(vec![
        (
            200,
            json!({"vulns": [record("A")], "next_page_token": "t1"}),
        ),
        (400, json!({"code": 3, "message": "invalid cursor"})),
    ]);
    let error = run(&http, None).await.expect_err("rejected");
    assert_eq!(error.status, Some(400));
}

#[tokio::test]
async fn an_advisory_reads_by_id_and_a_withdrawn_one_keeps_the_field() {
    let http = RouteHttp::new().json(
        "/v1/vulns/GHSA-8rfx-6mr3-5jh3",
        json!({"id": "GHSA-8rfx-6mr3-5jh3", "withdrawn": "2024-01-01T00:00:00Z",
            "affected": [{"package": {"name": "a", "ecosystem": "NuGet"}}]}),
    );
    let budget = test_budget();
    let reused = ReusedSince::default();
    let config = crate::tools::security_check::tests::default_config();
    let net = Net {
        http: &http,
        budget: &budget,
        answers: None,
        reused: &reused,
        config: &config,
        github: None,
        osv_api: crate::execution::RuntimeLimits::DEFAULT.osv_api,
    };
    let found = check_advisory(&net, "GHSA-8rfx-6mr3-5jh3")
        .await
        .expect("a record");
    assert_eq!(
        found.advisories[0].withdrawn.as_deref(),
        Some("2024-01-01T00:00:00Z")
    );
    assert_eq!(
        found.advisories[0].affected.len(),
        1,
        "every entry is its own"
    );
    assert_eq!(
        http.seen()[0].url.as_str(),
        "https://api.osv.dev/v1/vulns/GHSA-8rfx-6mr3-5jh3"
    );
    let missing = check_advisory(&net, "GHSA-0000-0000-0000")
        .await
        .expect_err("no record");
    assert_eq!(missing.code, ErrorCode::NotFound);
    assert!(
        missing.message.contains("OSV has no record"),
        "{}",
        missing.message
    );
}

/// A commit goes alone: OSV refuses a version beside it.
#[tokio::test]
async fn a_commit_query_holds_only_the_commit_and_the_token() {
    let commit = "6879efc2c1596d11a6a6ad296f80063b558d5e0f";
    let http = RouteHttp::new().osv_pages(vec![
        (
            200,
            json!({"vulns": [{"id": "OSV-2020-484"}], "next_page_token": "t1"}),
        ),
        (200, json!({})),
    ]);
    let done = on(&http, &Ask::Commit(commit), None).await.expect("walk");
    assert_eq!(ids(&done), ["OSV-2020-484"]);
    assert_eq!(
        http.osv_queries(),
        [
            json!({"commit": commit}),
            json!({"commit": commit, "page_token": "t1"})
        ]
    );
}

/// Records of module `golang.org/x/net`, as OSV states them: Go-database
/// records list their import paths, GHSA twins and some records state none.
mod go_imports {
    use super::*;

    const MODULE: &str = "golang.org/x/net";

    fn go_entry(imports: Option<&[&str]>) -> Value {
        let mut entry = json!({"package": {"name": MODULE, "ecosystem": "Go"}});
        if let Some(paths) = imports {
            let rows: Vec<Value> = paths.iter().map(|path| json!({"path": path})).collect();
            entry["ecosystem_specific"] = json!({"imports": rows});
        }
        entry
    }

    fn go_record(id: &str, aliases: &[&str], imports: Option<&[&str]>) -> Value {
        json!({"id": id, "schema_version": "1.6.0", "aliases": aliases,
               "affected": [go_entry(imports)]})
    }

    fn package_path(base: &str, leaf: &str) -> String {
        format!("{base}/{leaf}")
    }

    /// One record per case: a package-listing record with a GHSA twin that
    /// states no imports, a sibling-package record with its twin, a module-wide
    /// record, and a GHSA record that names the package as an entry of its own.
    fn module_records(package: &str, sibling: &str) -> Vec<Value> {
        vec![
            go_record("GO-HERE", &["GHSA-HERE"], Some(&[package])),
            go_record("GHSA-HERE", &["GO-HERE"], None),
            go_record("GO-SIBLING", &["GHSA-SIBLING"], Some(&[sibling])),
            go_record("GHSA-SIBLING", &["GO-SIBLING"], None),
            go_record("GO-WIDE", &[], None),
            json!({"id": "GHSA-NAMED", "schema_version": "1.6.0", "affected": [
                {"package": {"name": package, "ecosystem": "Go"}}, go_entry(None)]}),
        ]
    }

    async fn walked(import: Option<&str>) -> SourceResult {
        let package = package_path(MODULE, "http2");
        let sibling = package_path(MODULE, "html");
        let http = RouteHttp::new().osv_pages(vec![(
            200,
            json!({"vulns": module_records(&package, &sibling)}),
        )]);
        let ask = Ask::Package {
            ecosystem: osv_ecosystem(ArtifactType::Go),
            release: None,
            name: MODULE,
            version: None,
            import,
            by_name: false,
        };
        on(&http, &ask, None).await.expect("walk")
    }

    fn ids_of(rows: &[crate::tools::security_check::record::SecurityAdvisory]) -> Vec<&str> {
        rows.iter().map(|row| row.id.as_str()).collect()
    }

    #[tokio::test]
    async fn a_package_keeps_the_records_that_list_it_or_cover_the_whole_module() {
        let package = package_path(MODULE, "http2");
        let done = walked(Some(&package)).await;
        assert_eq!(
            ids_of(&done.advisories),
            ["GO-HERE", "GHSA-HERE", "GO-WIDE", "GHSA-NAMED"]
        );
    }

    #[tokio::test]
    async fn module_records_listing_other_packages_are_set_apart_with_their_twins() {
        let package = package_path(MODULE, "http2");
        let done = walked(Some(&package)).await;
        assert_eq!(ids_of(&done.module_only), ["GO-SIBLING", "GHSA-SIBLING"]);
    }

    #[tokio::test]
    async fn a_bare_module_keeps_every_record() {
        let done = walked(None).await;
        assert_eq!(done.advisories.len(), module_records("a", "b").len());
        assert!(done.module_only.is_empty());
    }

    #[tokio::test]
    async fn a_stopped_walk_keeps_the_package_it_narrows_to() {
        let package = package_path(MODULE, "http2");
        let sibling = package_path(MODULE, "html");
        let http = RouteHttp::new().osv_pages(vec![
            (
                200,
                json!({"vulns": module_records(&package, &sibling), "next_page_token": "t1"}),
            ),
            (503, json!({})),
        ]);
        let ask = Ask::Package {
            ecosystem: osv_ecosystem(ArtifactType::Go),
            release: None,
            name: MODULE,
            version: None,
            import: Some(&package),
            by_name: false,
        };
        let stopped = on(&http, &ask, None).await.expect("walk");
        let saved: Value =
            serde_json::from_str(&stopped.resume.expect("stopped").token).expect("saved");
        assert_eq!(saved["import"], json!(package));
    }
}

/// An ecosystem OSV cannot order is asked by name alone, and the version is
/// compared with each record's ranges here: the false negative of a version
/// query that matches none of them.
#[tokio::test]
async fn an_ecosystem_osv_cannot_order_is_asked_by_name_and_compared_here() {
    let record = |id: &str, fixed: &str| {
        json!({"id": id, "schema_version": "1.6.0", "affected": [{
            "package": {"name": "tj-actions/changed-files", "ecosystem": "GitHub Actions"},
            "ranges": [{"type": "ECOSYSTEM", "events": [{"introduced": "0"}, {"fixed": fixed}]}],
        }]})
    };
    let ask = |version| Ask::Package {
        ecosystem: crate::tools::security_check::ecosystem::Ecosystem::GitHubActions,
        release: None,
        name: "tj-actions/changed-files",
        version: Some(version),
        import: None,
        by_name: true,
    };
    let answer = json!({"vulns": [record("OLD", "41"), record("NEW", "46.0.1")]});
    let http = RouteHttp::new().osv_pages(vec![(200, answer.clone())]);
    let walked = on(&http, &ask("40.0.0"), None).await.expect("walk");
    assert_eq!(ids(&walked), ["OLD", "NEW"]);
    assert!(walked.compared_here && walked.unmatched.is_empty());
    assert_eq!(
        http.osv_queries(),
        vec![
            json!({"package": {"name": "tj-actions/changed-files", "ecosystem": "GitHub Actions"}})
        ]
    );
    let http = RouteHttp::new().osv_pages(vec![(200, answer.clone())]);
    assert_eq!(
        ids(&on(&http, &ask("45.0.0"), None).await.expect("walk")),
        ["NEW"]
    );
    let http = RouteHttp::new().osv_pages(vec![(200, answer)]);
    let cleared = on(&http, &ask("46.0.1"), None).await.expect("walk");
    assert!(cleared.advisories.is_empty() && cleared.compared_here);
}

/// `/v1/querybatch` and the records it lists: positional answers, OSV's batch
/// limit, one read for each distinct record, and a walk that resumes exactly.
mod batch_walk {
    use super::*;
    use crate::providers::saved_answers::KeptAnswers;

    fn net_of<'a>(
        http: &'a RouteHttp,
        budget: &'a octocode_io::budget::RequestBudget,
        reused: &'a ReusedSince,
        config: &'a crate::config::ResolvedConfig,
        answers: Option<&'a dyn octocode_github::SavedAnswers>,
    ) -> Net<'a> {
        Net {
            http,
            budget,
            answers,
            reused,
            config,
            github: None,
            osv_api: crate::execution::RuntimeLimits::DEFAULT.osv_api,
        }
    }

    fn listed(id: &str, modified: &str) -> Value {
        json!({"id": id, "modified": modified})
    }

    fn batch(results: Vec<Value>) -> Value {
        json!({"results": results})
    }

    fn lodash(version: &str) -> Ask<'static> {
        Ask::Package {
            ecosystem: osv_ecosystem(ArtifactType::Npm),
            release: None,
            name: "lodash",
            version: Some(Box::leak(version.to_owned().into_boxed_str())),
            import: None,
            by_name: false,
        }
    }

    fn gets(http: &RouteHttp, id: &str) -> usize {
        let target = format!("/v1/vulns/{id}");
        http.targets().iter().filter(|got| **got == target).count()
    }

    /// More queries than one request holds go in several, each answered
    /// positionally: result `i` of the request is query `i` of it.
    #[tokio::test]
    async fn queries_beyond_osvs_batch_limit_split_and_map_positionally() {
        let asks = (0..=batch::OSV_QUERYBATCH_MAX_QUERIES)
            .map(|index| Ask::Package {
                ecosystem: osv_ecosystem(ArtifactType::Npm),
                release: None,
                name: Box::leak(format!("p{index}").into_boxed_str()),
                version: None,
                import: None,
                by_name: false,
            })
            .collect::<Vec<_>>();
        let mut first = vec![json!({}); batch::OSV_QUERYBATCH_MAX_QUERIES];
        first[5] = json!({"vulns": [listed("X5", "m")]});
        let http = RouteHttp::new().post_sequence(vec![
            (200, batch(first)),
            (200, batch(vec![json!({"vulns": [listed("Y0", "m")]})])),
        ]);
        let (budget, reused, config) = (
            test_budget(),
            ReusedSince::default(),
            crate::tools::security_check::tests::default_config(),
        );
        let queries = asks
            .iter()
            .map(|ask| batch::PageQuery { ask, token: None })
            .collect::<Vec<_>>();
        let pages = batch::pages(&net_of(&http, &budget, &reused, &config, None), &queries).await;
        assert_eq!(pages.len(), asks.len());
        let refs = |index: usize| {
            pages[index]
                .as_ref()
                .expect("page")
                .refs
                .iter()
                .map(|listed| listed.id.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(refs(5), ["X5"]);
        assert_eq!(refs(batch::OSV_QUERYBATCH_MAX_QUERIES), ["Y0"]);
        assert!(refs(4).is_empty() && refs(6).is_empty());
        let sent = http
            .post_bodies()
            .iter()
            .map(|body| body["queries"].as_array().expect("queries").len())
            .collect::<Vec<_>>();
        assert_eq!(sent, [batch::OSV_QUERYBATCH_MAX_QUERIES, 1]);
        assert_eq!(
            http.seen()[1].url.as_str(),
            "https://api.osv.dev/v1/querybatch"
        );
    }

    /// A request OSV refuses or loses fails exactly its own queries.
    #[tokio::test]
    async fn a_lost_request_fails_its_own_queries_and_spares_the_others() {
        let asks = (0..=batch::OSV_QUERYBATCH_MAX_QUERIES)
            .map(|_| lodash("1.0.0"))
            .collect::<Vec<_>>();
        let http = RouteHttp::new().post_sequence(vec![
            (
                200,
                batch(vec![json!({}); batch::OSV_QUERYBATCH_MAX_QUERIES]),
            ),
            (400, json!({"code": 3, "message": "too many queries"})),
        ]);
        let (budget, reused, config) = (
            test_budget(),
            ReusedSince::default(),
            crate::tools::security_check::tests::default_config(),
        );
        let queries = asks
            .iter()
            .map(|ask| batch::PageQuery { ask, token: None })
            .collect::<Vec<_>>();
        let pages = batch::pages(&net_of(&http, &budget, &reused, &config, None), &queries).await;
        assert!(
            pages[..batch::OSV_QUERYBATCH_MAX_QUERIES]
                .iter()
                .all(Result::is_ok)
        );
        let error = pages.last().expect("last").as_ref().err().expect("error");
        assert_eq!(error.status, Some(400));
        assert!(
            error.message.contains("too many queries"),
            "{}",
            error.message
        );
    }

    /// Two walks listing one record read it once.
    #[tokio::test]
    async fn a_record_two_walks_list_is_read_once() {
        let http = RouteHttp::new()
            .json("/v1/vulns/A", record("A"))
            .post_sequence(vec![(
                200,
                batch(vec![
                    json!({"vulns": [listed("A", "m")]}),
                    json!({"vulns": [listed("A", "m")]}),
                ]),
            )]);
        let (budget, reused, config) = (
            test_budget(),
            ReusedSince::default(),
            crate::tools::security_check::tests::default_config(),
        );
        let (one, two) = (lodash("1.0.0"), lodash("2.0.0"));
        let walked = walk::walk_all(
            &net_of(&http, &budget, &reused, &config, None),
            &[
                walk::Start {
                    ask: &one,
                    resume: None,
                },
                walk::Start {
                    ask: &two,
                    resume: None,
                },
            ],
        )
        .await;
        assert_eq!(walked.len(), 2);
        assert!(
            walked
                .iter()
                .all(|walked| walked.as_ref().expect("walk").read.len() == 1)
        );
        assert_eq!(gets(&http, "A"), 1);
        assert_eq!(http.post_bodies().len(), 1, "one batch for both");
    }

    /// A record unchanged since it was read is not read again, by any later
    /// call; one OSV has changed is.
    #[tokio::test]
    async fn a_record_is_read_again_only_when_its_modified_stamp_changed() {
        let store = KeptAnswers::new(None);
        let (budget, reused, config) = (
            test_budget(),
            ReusedSince::default(),
            crate::tools::security_check::tests::default_config(),
        );
        let stamped = |modified: &str| {
            let mut stamped = record("A");
            stamped["modified"] = json!(modified);
            stamped
        };
        let asking = |modified: &'static str| {
            RouteHttp::new()
                .json("/v1/vulns/A", stamped(modified))
                .post_sequence(vec![(
                    200,
                    batch(vec![json!({"vulns": [listed("A", modified)]})]),
                )])
        };
        for (modified, reads) in [("m1", 1), ("m1", 0), ("m2", 1)] {
            let http = asking(modified);
            let net = net_of(&http, &budget, &reused, &config, Some(&store));
            let walked = walk::walk_one(&net, &lodash("1.0.0"), None)
                .await
                .expect("walk");
            assert_eq!(walked.read[0]["modified"], modified);
            assert_eq!(gets(&http, "A"), reads, "{modified}");
        }
    }

    /// A kept record that does not state the stamp it was listed with is never trusted.
    #[tokio::test]
    async fn a_record_newer_than_the_stamp_listed_is_kept_under_its_own() {
        let store = KeptAnswers::new(None);
        let (budget, reused, config) = (
            test_budget(),
            ReusedSince::default(),
            crate::tools::security_check::tests::default_config(),
        );
        let mut newer = record("A");
        newer["modified"] = json!("m2");
        let http = RouteHttp::new()
            .json("/v1/vulns/A", newer.clone())
            .post_sequence(vec![(
                200,
                batch(vec![json!({"vulns": [listed("A", "m1")]})]),
            )]);
        let net = net_of(&http, &budget, &reused, &config, Some(&store));
        walk::walk_one(&net, &lodash("1.0.0"), None)
            .await
            .expect("walk");
        // The next call lists `m1` again: nothing kept states it, so it is read.
        let http = RouteHttp::new()
            .json("/v1/vulns/A", newer)
            .post_sequence(vec![(
                200,
                batch(vec![json!({"vulns": [listed("A", "m1")]})]),
            )]);
        let net = net_of(&http, &budget, &reused, &config, Some(&store));
        walk::walk_one(&net, &lodash("1.0.0"), None)
            .await
            .expect("walk");
        assert_eq!(gets(&http, "A"), 1);
    }

    /// A record that cannot be read stops the walk with exactly the ids still
    /// unread: the rest reads on with no gap or repeat, and no new query.
    #[tokio::test]
    async fn records_lost_to_a_transient_failure_resume_exactly_once() {
        let http = RouteHttp::new()
            .json("/v1/vulns/A", record("A"))
            .status("/v1/vulns/B", 503, json!({}))
            .post_sequence(vec![(
                200,
                batch(vec![json!({"vulns": [listed("A", "m"), listed("B", "m")]})]),
            )]);
        let stopped = run(&http, None).await.expect("partial");
        assert!(stopped.advisories.is_empty(), "listed only once finished");
        let resume = stopped.resume.expect("a state to resume from");
        let saved: Value = serde_json::from_str(&resume.token).expect("saved");
        assert_eq!(saved["pending"], json!([{"id": "B", "modified": "m"}]));
        assert!(saved["token"].is_null());
        let http = RouteHttp::new().json("/v1/vulns/B", record("B"));
        let rest = run(&http, Some(&resume.token)).await.expect("rest");
        assert_eq!(ids(&rest), ["A", "B"]);
        assert!(http.post_bodies().is_empty(), "no page was asked again");
        assert_eq!(gets(&http, "A"), 0);
    }

    /// Records that cannot be read stop the walk even when none was, since the
    /// page was: the ids are saved. A resumed walk that fails again at once, with
    /// nothing read, fails whole instead: a retry could only repeat it.
    #[tokio::test]
    async fn a_resumed_walk_that_reads_nothing_fails_whole() {
        let http = RouteHttp::new()
            .status("/v1/vulns/A", 503, json!({}))
            .post_sequence(vec![(
                200,
                batch(vec![json!({"vulns": [listed("A", "m")]})]),
            )]);
        let stopped = run(&http, None).await.expect("partial");
        let resume = stopped.resume.expect("the ids still unread");
        let saved: Value = serde_json::from_str(&resume.token).expect("saved");
        assert_eq!(saved["pending"], json!([{"id": "A", "modified": "m"}]));
        let error = run(&http, Some(&resume.token))
            .await
            .expect_err("no progress");
        assert_eq!(error.status, Some(503));
    }

    /// A listed id OSV has no record of is never silently dropped: the walk
    /// stops with the id saved.
    #[tokio::test]
    async fn a_listed_id_with_no_record_is_not_dropped() {
        let http = RouteHttp::new().post_sequence(vec![(
            200,
            batch(vec![json!({"vulns": [listed("A", "m")]})]),
        )]);
        let stopped = run(&http, None).await.expect("partial");
        let resume = stopped.resume.expect("the id still unread");
        assert!(resume.reason.contains("no record of"), "{}", resume.reason);
        assert!(resume.token.contains("\"A\""), "{}", resume.token);
    }

    /// The last page of a resumed walk may list nothing: what earlier pages
    /// read still counts.
    #[tokio::test]
    async fn an_empty_final_page_keeps_what_earlier_pages_read() {
        let saved = json!({"pending": [], "token": "t1", "read": [record("A")], "seen": ["t1"]})
            .to_string();
        let http = RouteHttp::new().post_sequence(vec![(200, batch(vec![json!({})]))]);
        let done = run(&http, Some(&saved)).await.expect("walk");
        assert_eq!(ids(&done), ["A"]);
    }
}
