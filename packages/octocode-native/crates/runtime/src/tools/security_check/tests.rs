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
