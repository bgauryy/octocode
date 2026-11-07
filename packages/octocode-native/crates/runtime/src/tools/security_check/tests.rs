//! Rows by operation, against recorded doubles.
use super::*;
use crate::providers::artifact::test_http::{RouteHttp, test_budget};
use serde_json::json;

/// Every setting at its contract default.
pub(crate) fn default_config() -> ResolvedConfig {
    let missing = |path: &str| crate::config::FileInput::Missing { path: path.into() };
    crate::config::resolve_config(&crate::config::ConfigInput {
        env: Default::default(),
        cwd: "/synthetic/cwd".into(),
        os_home: "/synthetic".into(),
        global_env: missing("/synthetic/.octocode/.env"),
        project_env: missing("/synthetic/cwd/.octocode/.env"),
        config_file: missing("/synthetic/.octocode/.octocoderc"),
        project_config_file: missing("/synthetic/cwd/.octocode/.octocoderc"),
        runtime_surface: crate::config::RuntimeSurface::Cli,
    })
    .resolved
}

fn row(query: Value) -> SecurityCheckQuery {
    serde_json::from_value(query).expect("securityCheck row")
}

async fn answer(http: &RouteHttp, query: Value) -> Result<Value, ArtifactError> {
    let budget = test_budget();
    let reused = ReusedSince::default();
    run(
        &row(query),
        &Net {
            http,
            budget: &budget,
            answers: None,
            reused: &reused,
            config: &default_config(),
        },
    )
    .await
}

fn registry() -> RouteHttp {
    let packument = json!({"name": "lodash", "dist-tags": {"latest": "4.18.1"},
        "versions": {"4.17.20": {"name": "lodash", "version": "4.17.20"}}});
    RouteHttp::new().json("/lodash", packument).json(
        "/lodash/4.17.20",
        json!({"name": "lodash", "version": "4.17.20"}),
    )
}

#[tokio::test]
async fn a_package_url_runs_as_that_package() {
    let http = registry().post_sequence(vec![(200, json!({"vulns": [{"id": "GHSA-1"}]}))]);
    let data = answer(
        &http,
        json!({"operation": "url", "url": "https://www.npmjs.com/package/lodash/v/4.17.20"}),
    )
    .await
    .expect("row");
    assert_eq!(data["advisories"][0]["id"], "GHSA-1");
    assert_eq!(http.post_bodies()[0]["version"], "4.17.20");
}

#[tokio::test]
async fn a_url_that_names_no_package_says_so_and_sends_nothing() {
    let http = RouteHttp::new();
    let error = answer(
        &http,
        json!({"operation": "url", "url": "https://example.com/x"}),
    )
    .await
    .expect_err("unresolved");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert!(
        error.message.contains("cannot be resolved"),
        "{}",
        error.message
    );
    assert!(http.seen().is_empty());
}

/// A repository page is refused before any request. Its lead is the
/// repository's root tree, whose manifest names the package to check: never a
/// guessed package (a Go module form of a JavaScript repository is notFound).
#[tokio::test]
async fn a_repository_url_sends_nothing_and_leads_to_its_manifest() {
    let http = RouteHttp::new();
    let error = answer(
        &http,
        json!({"operation": "url", "url": "https://github.com/acme/private"}),
    )
    .await
    .expect_err("refused");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert!(
        error.message.contains("is a repository"),
        "{}",
        error.message
    );
    assert!(http.seen().is_empty(), "{:?}", http.targets());
    let calls = serde_json::to_value(&error.calls).expect("lead");
    let lead = &calls["viewTree"];
    assert_eq!(lead["tool"], "ghStructure", "{calls}");
    assert_eq!(
        lead["input"]["queries"][0],
        json!({"owner": "acme", "repo": "private", "operation": "tree"}),
        "{calls}"
    );
    assert!(calls.get("repair").is_none(), "{calls}");
}

#[tokio::test]
async fn an_advisory_id_reads_its_record_and_a_missing_one_is_not_clean() {
    let http = RouteHttp::new().json(
        "/v1/vulns/CVE-2021-44228",
        json!({"id": "CVE-2021-44228", "summary": "Log4Shell"}),
    );
    let data = answer(
        &http,
        json!({"operation": "advisory", "id": "CVE-2021-44228"}),
    )
    .await
    .expect("row");
    assert_eq!(data["advisories"][0]["summary"], "Log4Shell");
    assert_eq!(
        data["sources"],
        json!([{"source": "osv", "status": "found"}])
    );
    assert_eq!(
        data["advisories"][0]["link"],
        "https://osv.dev/vulnerability/CVE-2021-44228"
    );
    let error = answer(
        &http,
        json!({"operation": "advisory", "id": "CVE-0000-0000"}),
    )
    .await
    .expect_err("not found");
    assert_eq!(error.code, ErrorCode::NotFound);
    assert!(
        error.message.contains("OSV has no record"),
        "{}",
        error.message
    );
}

/// OSV ids are case-sensitive and a few keep a lower-case part: the id the caller
/// wrote is asked first and never rewritten when OSV knows it.
#[tokio::test]
async fn a_mixed_case_advisory_id_is_found_as_given() {
    let id = "openSUSE-SU-2019:0174-1";
    let path = "/v1/vulns/openSUSE-SU-2019%3A0174-1";
    let http = RouteHttp::new().json(path, json!({"id": id, "summary": "SUSE update"}));
    let data = answer(&http, json!({"operation": "advisory", "id": id}))
        .await
        .expect("row");
    assert_eq!(data["advisories"][0]["id"], id);
    assert!(data.get("warnings").is_none(), "{data}");
    assert_eq!(http.targets(), [path]);
}

/// An advisory row leads with `answer`: the id, its aliases, each affected
/// package with its ranges and the fix that closes each one (RUSTSEC-2020-0071:
/// 0.2.10 is affected and needs 0.2.23, not 0.2.0), then the whole record.
#[tokio::test]
async fn an_advisory_row_answers_with_its_ranges_and_fixes() {
    let id = "RUSTSEC-2020-0071";
    let http = RouteHttp::new().json(
        "/v1/vulns/RUSTSEC-2020-0071",
        json!({"id": id, "summary": "Potential segfault in the time crate",
            "aliases": ["CVE-2020-26235", "GHSA-wcg3-cvx6-7396"],
            "affected": [{"package": {"ecosystem": "crates.io", "name": "time"},
                "ranges": [{"type": "SEMVER", "events": [
                    {"introduced": "0.0.0-0"}, {"fixed": "0.2.0"},
                    {"introduced": "0.2.1-0"}, {"fixed": "0.2.1"},
                    {"introduced": "0.2.7-0"}, {"fixed": "0.2.23"}]}]}]}),
    );
    let data = answer(&http, json!({"operation": "advisory", "id": id}))
        .await
        .expect("row");
    let sentence = data["answer"].as_str().expect("answer");
    for expected in [
        id,
        "CVE-2020-26235",
        "GHSA-wcg3-cvx6-7396",
        "time",
        "<0.2.0 (fixed 0.2.0)",
        ">=0.2.1-0 <0.2.1 (fixed 0.2.1)",
        ">=0.2.7-0 <0.2.23 (fixed 0.2.23)",
    ] {
        assert!(sentence.contains(expected), "{expected}: {sentence}");
    }
    let keys = data.as_object().expect("row").keys().collect::<Vec<_>>();
    let at = |key: &str| keys.iter().position(|known| *known == key);
    assert!(at("answer") < at("advisories"), "{keys:?}");
    assert_eq!(data["advisories"][0]["id"], id, "the record stays whole");
}

/// OSV only recommends sorted events, and its evaluation sorts them by the
/// range's version order: shuffled events pair the same as sorted ones.
#[test]
fn shuffled_osv_events_pair_in_version_order() {
    let row = json!({"advisories": [{"id": "RUSTSEC-2020-0071",
        "affected": [{"packageName": "time", "ranges": [{"type": "SEMVER", "events": [
            {"fixed": "0.2.23"}, {"introduced": "0.2.1-0"}, {"fixed": "0.2.0"},
            {"introduced": "0.2.7-0"}, {"introduced": "0.0.0-0"}, {"fixed": "0.2.1"}]}]}]}]});
    let lead = super::answer::advisory_lead(&row);
    let sentence = lead["answer"].as_str().expect("answer");
    assert!(
        sentence.contains(
            "<0.2.0 (fixed 0.2.0), >=0.2.1-0 <0.2.1 (fixed 0.2.1), >=0.2.7-0 <0.2.23 (fixed 0.2.23)"
        ),
        "{sentence}"
    );
}

/// An entry with no ranges says what it holds: GHSA-jfh8-c2jp-5v3q lists
/// single versions for some packages, and an entry with only GIT ranges
/// names commits.
#[test]
fn an_entry_without_version_ranges_says_what_it_lists() {
    let row = json!({"advisories": [{"id": "GHSA-jfh8-c2jp-5v3q",
        "affected": [
            {"packageName": "org.xbib.elasticsearch:log4j", "ranges": null,
                "versions": ["6.3.2.1"]},
            {"packageName": "g:git", "ranges": [{"type": "GIT", "repo": "https://g",
                "events": [{"introduced": "0"}, {"fixed": "abc"}]}]},
            {"packageName": "g:none"}]}]});
    let lead = super::answer::advisory_lead(&row);
    let sentence = lead["answer"].as_str().expect("answer");
    for expected in [
        "org.xbib.elasticsearch:log4j (1 version listed in the record)",
        "g:git (git commit ranges in the record)",
        "g:none (no ranges or versions in the record)",
    ] {
        assert!(sentence.contains(expected), "{expected}: {sentence}");
    }
    assert!(!sentence.contains("(ranges in the record)"), "{sentence}");
}

/// An ECOSYSTEM range keeps the order OSV lists: a generic comparator
/// mis-orders Maven's `1.0-sp1` and PyPI's `1.0.post1`, both after `1.0`.
#[test]
fn ecosystem_osv_events_keep_their_listed_order() {
    let row = json!({"advisories": [{"id": "GHSA-test",
        "affected": [
            {"packageName": "p", "ranges": [{"type": "ECOSYSTEM", "events": [
                {"introduced": "1.0"}, {"fixed": "1.0.post1"}]}]},
            {"packageName": "g:m", "ranges": [{"type": "ECOSYSTEM", "events": [
                {"introduced": "1.0"}, {"fixed": "1.0-sp1"}]}]}]}]});
    let lead = super::answer::advisory_lead(&row);
    let sentence = lead["answer"].as_str().expect("answer");
    assert!(
        sentence.contains("p >=1.0 <1.0.post1 (fixed 1.0.post1)")
            && sentence.contains("g:m >=1.0 <1.0-sp1 (fixed 1.0-sp1)"),
        "{sentence}"
    );
}

/// A spelling OSV does not know falls back to the usual one once, and says so.
#[tokio::test]
async fn a_lower_case_advisory_id_resolves_through_one_disclosed_retry() {
    let http = RouteHttp::new().json(
        "/v1/vulns/GHSA-5crp-9r3c-p9vr",
        json!({"id": "GHSA-5crp-9r3c-p9vr", "summary": "Axios"}),
    );
    let data = answer(
        &http,
        json!({"operation": "advisory", "id": "ghsa-5crp-9r3c-p9vr"}),
    )
    .await
    .expect("row");
    assert_eq!(data["advisories"][0]["id"], "GHSA-5crp-9r3c-p9vr");
    assert_eq!(
        http.targets(),
        [
            "/v1/vulns/ghsa-5crp-9r3c-p9vr",
            "/v1/vulns/GHSA-5crp-9r3c-p9vr"
        ]
    );
    let warnings = data["warnings"].to_string();
    assert!(
        warnings.contains("ghsa-5crp-9r3c-p9vr") && warnings.contains("GHSA-5crp-9r3c-p9vr"),
        "{warnings}"
    );
}

#[tokio::test]
async fn an_id_in_the_canonical_spelling_is_asked_once() {
    let http = RouteHttp::new();
    let error = answer(
        &http,
        json!({"operation": "advisory", "id": "CVE-0000-0000"}),
    )
    .await
    .expect_err("not found");
    assert_eq!(error.code, ErrorCode::NotFound);
    assert_eq!(http.targets(), ["/v1/vulns/CVE-0000-0000"]);
}

#[tokio::test]
async fn an_id_unknown_in_both_spellings_is_not_found_and_names_both() {
    let http = RouteHttp::new();
    let error = answer(
        &http,
        json!({"operation": "advisory", "id": "cve-0000-0000"}),
    )
    .await
    .expect_err("not found");
    assert_eq!(error.code, ErrorCode::NotFound);
    assert_eq!(http.targets().len(), 2);
    assert!(
        error.message.contains("cve-0000-0000") && error.message.contains("CVE-0000-0000"),
        "{}",
        error.message
    );
}

/// The schema: nothing else leaves, and a caller cannot hand in OSV's token.
#[test]
fn the_contract_admits_only_the_public_form() {
    let validates = |row: Value| crate::contracts::validate_query("securityCheck", row).is_ok();
    let package = json!({"operation": "package", "ecosystem": "npm", "packageName": "lodash"});
    assert!(validates(package.clone()));
    for (key, value) in [
        ("registryUrl", json!("https://registry.internal.example")),
        ("resume", json!("osv-token")),
        ("limit", json!(5)),
    ] {
        let mut row = package.clone();
        row[key] = value;
        assert!(!validates(row), "{key}");
    }
    assert!(validates(
        json!({"operation": "url", "url": "pkg:npm/lodash"})
    ));
    assert!(validates(
        json!({"operation": "advisory", "id": "CVE-2021-44228"})
    ));
    assert!(!validates(json!({"operation": "advisory", "id": "../etc"})));
}

/// Core's `source` enum and this tool's sources name the same set.
#[test]
fn the_source_names_match_the_contract() {
    fn enums(value: &Value, found: &mut Vec<Vec<String>>) {
        match value {
            Value::Object(fields) => {
                let row = fields.get("properties");
                let names = row
                    .filter(|row| row.get("status").is_some())
                    .and_then(|row| row.pointer("/source/enum"))
                    .and_then(Value::as_array);
                if let Some(names) = names {
                    found.push(
                        names
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect(),
                    );
                }
                for child in fields.values() {
                    enums(child, found);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| enums(item, found)),
            _ => {}
        }
    }
    let contract = crate::contracts::parsed_contract().expect("contract");
    let tool = contract["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "securityCheck")
        .expect("securityCheck");
    let mut found = Vec::new();
    enums(tool, &mut found);
    enums(&contract["$defs"], &mut found);
    let ours = source::Source::ALL.map(|source| source.name().to_owned());
    assert!(!found.is_empty(), "no source enum in the contract");
    assert!(found.iter().all(|names| names == &ours), "{found:?}");
}

const COMMIT: &str = "6879efc2c1596d11a6a6ad296f80063b558d5e0f";

/// A commit's `{}` is OSV's answer for a clean commit and an unknown one alike,
/// so the row says "none known" and that it cannot tell them apart.
#[tokio::test]
async fn an_empty_commit_answer_never_reads_as_clean() {
    let http = RouteHttp::new().post_sequence(vec![(200, json!({}))]);
    let data = answer(&http, json!({"operation": "commit", "commit": COMMIT}))
        .await
        .expect("row");
    assert_eq!(data["status"], "empty");
    assert!(
        data.get("affected").is_none(),
        "unknown is not false: {data}"
    );
    let lead = data["answer"].as_str().expect("answer");
    assert!(lead.contains("None known to OSV for commit"), "{lead}");
    assert!(lead.contains("proves nothing"), "{lead}");
    assert!(lead.contains("does not track"), "{lead}");
    assert_eq!(http.post_bodies(), [json!({"commit": COMMIT})]);
}

#[tokio::test]
async fn a_found_commit_says_so_and_the_repo_stays_local() {
    let record = json!({"id": "OSV-2020-484", "aliases": ["CVE-2023-25193"], "affected": [{
        "ranges": [{"type": "GIT", "repo": "https://github.com/harfbuzz/harfbuzz",
            "events": [{"introduced": "0"}, {"fixed": "abc"}]}]}]});
    let http = RouteHttp::new().post_sequence(vec![(200, json!({"vulns": [record.clone()]}))]);
    let data = answer(
        &http,
        json!({"operation": "commit", "commit": COMMIT, "repo": "https://github.com/harfbuzz/harfbuzz.git"}),
    )
    .await
    .expect("row");
    assert_eq!(data["affected"], true);
    assert_eq!(data["cves"], json!(["CVE-2023-25193"]));
    assert!(data.get("warnings").is_none(), "{data}");
    assert_eq!(
        http.post_bodies(),
        [json!({"commit": COMMIT})],
        "the repo is not sent"
    );
    // A record in another repository is pointed out.
    let http = RouteHttp::new().post_sequence(vec![(200, json!({"vulns": [record]}))]);
    let data = answer(
        &http,
        json!({"operation": "commit", "commit": COMMIT, "repo": "https://example.test/o/r"}),
    )
    .await
    .expect("row");
    assert!(
        data["warnings"][0]
            .as_str()
            .unwrap_or("")
            .contains("another repository"),
        "{data}"
    );
}

#[test]
fn a_commit_is_one_full_hash() {
    let validates = |row: Value| crate::contracts::validate_query("securityCheck", row).is_ok();
    assert!(validates(json!({"operation": "commit", "commit": COMMIT})));
    for short in ["6879efc", "nothex", ""] {
        assert!(
            !validates(json!({"operation": "commit", "commit": short})),
            "{short}"
        );
    }
    assert!(!validates(
        json!({"operation": "commit", "commit": COMMIT, "version": "1.0"})
    ));
}

/// Every row form this tool builds holds to its output contract: a database with
/// no severity word gives vectors in the brief row, not a text.
#[tokio::test]
async fn rows_hold_to_the_output_contract() {
    use crate::tools::test_support::assert_row_contract;
    let pysec = json!({"id": "PYSEC-1", "details": "First.", "aliases": ["CVE-2024-0001"],
        "severity": [{"type": "CVSS_V3", "score": "CVSS:3.1/AV:L"}],
        "references": [{"type": "WEB", "url": "https://example.test/a"}],
        "affected": [{"package": {"name": "lodash", "ecosystem": "npm"},
            "versions": ["1.0.0"],
            "ecosystem_specific": {"imports": [{"path": "a"}]},
            "database_specific": {"source": "https://example.test/s", "cvss": "x"},
            "ranges": [{"type": "GIT", "repo": "https://example.test/r", "events": [{"fixed": "abc"}]}]}]});
    let http = registry().post_sequence(vec![(200, json!({"vulns": [pysec.clone()]}))]);
    let package = answer(
        &http,
        json!({"operation": "package", "ecosystem": "npm", "packageName": "lodash", "version": "4.17.20"}),
    )
    .await
    .expect("package row");
    assert_row_contract(ToolId::SecurityCheck, &package);
    let http = RouteHttp::new().post_sequence(vec![(200, json!({"vulns": [pysec.clone()]}))]);
    let commit = answer(&http, json!({"operation": "commit", "commit": COMMIT}))
        .await
        .expect("commit row");
    assert_row_contract(ToolId::SecurityCheck, &commit);
    let http = RouteHttp::new().json("/v1/vulns/PYSEC-1", pysec);
    let whole = answer(&http, json!({"operation": "advisory", "id": "PYSEC-1"}))
        .await
        .expect("advisory row");
    assert_row_contract(ToolId::SecurityCheck, &whole);
}
