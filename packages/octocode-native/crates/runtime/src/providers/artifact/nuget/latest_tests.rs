//! NuGet rows: the newest stable version a search row names, and what an exact
//! row keeps about a version's safety.
use super::super::query::artifact_query;
use super::super::test_http::RouteHttp;
use super::super::test_http::recorded;
use super::super::test_http::test_budget;
use super::*;
use serde_json::json;

const REGISTRATION: &str = "/v3/registration5/newtonsoft.json/index.json";

fn service_index() -> serde_json::Value {
    json!({
        "resources": [
            {
                "@type": "RegistrationsBaseUrl/3.6.0",
                "@id": "https://api.nuget.org/v3/registration5/"
            }
        ]
    })
}

/// The exact `Newtonsoft.Json` lookup against `http`.
async fn newtonsoft_exact(http: &RouteHttp) -> ArtifactProviderPage {
    let b = test_budget();
    let client = RegistryClient::new(http, &b);
    let q = artifact_query(
        json!({"operation":"resolve","ecosystem": ArtifactType::Nuget, "packageName": "Newtonsoft.Json"}),
        None,
    );
    nuget(&q, &ArtifactProviderState::default(), &client)
        .await
        .expect("nuget exact")
}

/// A search asks with `prerelease=true` so prerelease-only packages stay
/// findable, and then each row's `version` is the newest of everything. Its
/// `versions` list names the newest stable one, which is the row's latest.
#[tokio::test]
async fn a_nuget_search_row_names_the_newest_stable_version() {
    let index = json!({"resources": [{
        "@type": "SearchQueryService/3.5.0",
        "@id": "https://azuresearch-usnc.nuget.org/query"
    }]});
    let listed = |versions: &[&str]| {
        versions
            .iter()
            .map(|version| json!({"version": version, "downloads": 1}))
            .collect::<Vec<_>>()
    };
    let http = RouteHttp::new().json("/v3/index.json", index).json(
        "/query",
        json!({"totalHits": 3, "data": [
            {"id": "Newtonsoft.Json", "version": "14.0.1-beta2",
             "versions": listed(&["13.0.3", "13.0.4", "14.0.1-beta1", "14.0.1-beta2"])},
            {"id": "Only.Preview", "version": "1.0.0-preview2",
             "versions": listed(&["1.0.0-preview1", "1.0.0-preview2"])},
            {"id": "No.List", "version": "2.0.0-rc1"}
        ]}),
    );
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let query = artifact_query(
        json!({"operation":"search","ecosystem": ArtifactType::Nuget, "keywords": ["json"]}),
        None,
    );
    let page = nuget(&query, &ArtifactProviderState::default(), &client)
        .await
        .expect("nuget search");
    let versions = page
        .artifacts
        .iter()
        .map(|row| row.version.as_deref().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(versions, ["13.0.4", "1.0.0-preview2", "2.0.0-rc1"]);
}

/// An exact row keeps what the registry states about the version's safety:
/// its advisories (none listed is `[]`, as NuGet omits an empty list) and its
/// deprecation.
#[tokio::test]
async fn nuget_exact_rows_keep_vulnerabilities_and_deprecation() {
    let registration = json!({"items": [{
        "upper": "12.0.3",
        "items": [
            {"catalogEntry": {"id": "Newtonsoft.Json", "version": "12.0.3",
                "vulnerabilities": [
                    {"advisoryUrl": "https://github.com/advisories/GHSA-5crp-9r3c-p9vr", "severity": "2"},
                    {"advisoryUrl": "https://example.test/low", "severity": "0"},
                    {"advisoryUrl": "https://example.test/unknown", "severity": "9"}
                ],
                "deprecation": {"reasons": ["Legacy"], "message": "Obsolete.",
                    "alternatePackage": {"id": "New.Package", "range": "*"}}}}
        ]
    }]});
    let http = RouteHttp::routes(vec![
        (REGISTRATION, registration),
        ("/v3/index.json", service_index()),
    ]);
    let page = newtonsoft_exact(&http).await;
    let row = serde_json::to_value(&page.artifacts[0]).expect("row");
    assert_eq!(
        row["advisories"],
        json!([
            {"url": "https://github.com/advisories/GHSA-5crp-9r3c-p9vr", "severity": "high"},
            {"url": "https://example.test/low", "severity": "low"},
            {"url": "https://example.test/unknown", "severity": "9"}
        ])
    );
    let deprecated = row["deprecated"].as_str().unwrap_or_default();
    assert!(
        deprecated.contains("Obsolete.") && deprecated.contains("New.Package"),
        "{deprecated}"
    );
    let reasons = RouteHttp::routes(vec![
        (
            REGISTRATION,
            json!({"items": [{"upper": "1.0.0", "items": [
                {"catalogEntry": {"id": "Newtonsoft.Json", "version": "1.0.0",
                    "deprecation": {"reasons": ["CriticalBugs", "Legacy"]}}}
            ]}]}),
        ),
        ("/v3/index.json", service_index()),
    ]);
    let page = newtonsoft_exact(&reasons).await;
    let row = serde_json::to_value(&page.artifacts[0]).expect("row");
    let deprecated = row["deprecated"].as_str().unwrap_or_default();
    assert!(
        deprecated.contains("CriticalBugs") && deprecated.contains("Legacy"),
        "{deprecated}"
    );
    // A catalog entry with no list: NuGet lists no advisory for this version.
    assert_eq!(row["advisories"], json!([]));
}

/// NuGet's index lists its older registration hives first, and only the newest
/// (`RegistrationsBaseUrl/3.6.0`) carries `vulnerabilities`, `deprecation` and
/// SemVer 2.0 versions: an exact lookup reads that one wherever it sits, or its
/// empty advisory list would claim a vulnerable version is clean. Every hive here
/// is NuGet's real answer.
#[tokio::test]
async fn an_exact_lookup_reads_the_newest_registration_hive() {
    let case = "nuget/exact-advisory";
    let http = RouteHttp::recorded(case);
    let newest = recorded::answer(case, "/registration5-gz-semver2/");
    let listed: Vec<&serde_json::Value> = newest["items"]
        .as_array()
        .expect("pages")
        .iter()
        .flat_map(|page| page["items"].as_array().expect("versions"))
        .filter(|item| item["catalogEntry"]["version"] == "12.0.3")
        .filter_map(|item| {
            item["catalogEntry"]["vulnerabilities"][0]["advisoryUrl"]
                .as_str()
                .map(|_| &item["catalogEntry"]["vulnerabilities"][0]["advisoryUrl"])
        })
        .collect();
    assert!(!listed.is_empty(), "the newest hive lists the advisory");
    assert!(
        !recorded::answer(case, "/registration5-semver1/")
            .to_string()
            .contains("vulnerabilities"),
        "the lower hive never does"
    );
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let q = artifact_query(
        json!({"operation":"resolve","ecosystem": ArtifactType::Nuget, "packageName": "Newtonsoft.Json", "version": "12.0.3"}),
        None,
    );
    let page = nuget(&q, &ArtifactProviderState::default(), &client)
        .await
        .expect("nuget exact");
    let row = serde_json::to_value(&page.artifacts[0]).expect("row");
    assert_eq!(&row["advisories"][0]["url"], listed[0], "{row}");
    // The nuspec read asks the package base address, not the search service
    // listed before it.
    assert!(
        http.targets()
            .iter()
            .any(|target| target.starts_with("/v3-flatcontainer/")),
        "{:?}",
        http.targets()
    );
}
