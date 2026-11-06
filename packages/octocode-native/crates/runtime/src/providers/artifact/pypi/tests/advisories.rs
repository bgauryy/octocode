use super::*;

/// A release's row keeps what PyPI states about its safety: why it was yanked
/// and its `vulnerabilities`, whole; none listed is `[]`. A response without
/// the key says nothing, so the row claims nothing.
#[tokio::test]
async fn pypi_rows_keep_the_yanked_reason_and_vulnerabilities() {
    let mut release = pypi_release("2.32.0");
    release["info"]["yanked"] = json!(true);
    release["info"]["yanked_reason"] =
        json!("Yanked due to conflicts with CVE-2024-35195 mitigation");
    release["vulnerabilities"] = json!([{
        "aliases": ["CVE-2024-47081", "PYSEC-2026-1872"],
        "details": "URL parsing issue.",
        "fixed_in": ["2.32.4"],
        "id": "GHSA-9hjg-9r4m-mvj7",
        "link": "https://osv.dev/vulnerability/GHSA-9hjg-9r4m-mvj7",
        "source": "osv",
        "summary": null,
        "withdrawn": null
    }, {"id": "PYSEC-1", "aliases": [], "fixed_in": [], "withdrawn": "2026-01-02T00:00:00Z"}]);
    let row = |release: serde_json::Value| async move {
        let http = RouteHttp::routes(vec![("/pypi/requests/2.32.0/json", release)]);
        let b = test_budget();
        let client = RegistryClient::new(&http, &b);
        let query = versioned(ArtifactType::Pypi, "requests", "2.32.0");
        let page = pypi(&query, &client).await.expect("pypi");
        serde_json::to_value(&page.artifacts[0]).expect("row")
    };
    let yanked = row(release).await;
    assert_eq!(yanked["yanked"], true);
    assert_eq!(
        yanked["yankedReason"],
        "Yanked due to conflicts with CVE-2024-35195 mitigation"
    );
    assert_eq!(
        yanked["advisories"],
        json!([{
            "id": "GHSA-9hjg-9r4m-mvj7",
            "aliases": ["CVE-2024-47081", "PYSEC-2026-1872"],
            "url": "https://osv.dev/vulnerability/GHSA-9hjg-9r4m-mvj7",
            "fixedIn": ["2.32.4"]
        }, {"id": "PYSEC-1", "withdrawn": "2026-01-02T00:00:00Z"}])
    );
    let mut clean = pypi_release("2.32.0");
    clean["vulnerabilities"] = json!([]);
    assert_eq!(row(clean).await["advisories"], json!([]));
    let silent = row(pypi_release("2.32.0")).await;
    assert!(silent.get("advisories").is_none(), "{silent}");
    assert!(silent.get("yankedReason").is_none(), "{silent}");
}

/// PyPI lists one advisory once per database (GHSA and its PYSEC twin, each
/// aliasing the other and the CVE): the row lists it once, keeping every id
/// and alias and every fixed version; an unrelated advisory stays its own.
#[tokio::test]
async fn pypi_twin_advisories_are_one_row_with_every_alias() {
    let mut release = pypi_release("2.31.0");
    release["vulnerabilities"] = json!([
        {"id": "GHSA-9wx4-h78v-vm56", "aliases": ["CVE-2024-35195", "PYSEC-2026-1873"],
         "details": "verify=False sticks.", "fixed_in": ["2.32.0"],
         "link": "https://osv.dev/vulnerability/GHSA-9wx4-h78v-vm56"},
        {"id": "GHSA-9hjg-9r4m-mvj7", "aliases": ["CVE-2024-47081"], "details": ".netrc leak.",
         "fixed_in": ["2.32.4"]},
        {"id": "PYSEC-2026-1873", "aliases": ["CVE-2024-35195", "GHSA-9wx4-h78v-vm56"],
         "details": "verify=False sticks.", "fixed_in": ["2.32.0", "2.32.1"],
         "link": "https://osv.dev/vulnerability/PYSEC-2026-1873"}
    ]);
    let http = RouteHttp::routes(vec![("/pypi/requests/2.31.0/json", release)]);
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let query = versioned(ArtifactType::Pypi, "requests", "2.31.0");
    let page = pypi(&query, &client).await.expect("pypi");
    let row = serde_json::to_value(&page.artifacts[0]).expect("row");
    assert_eq!(
        row["advisories"],
        json!([{
            "id": "GHSA-9wx4-h78v-vm56",
            "aliases": ["CVE-2024-35195", "PYSEC-2026-1873"],
            "url": "https://osv.dev/vulnerability/GHSA-9wx4-h78v-vm56",
            "fixedIn": ["2.32.0", "2.32.1"]
        }, {
            "id": "GHSA-9hjg-9r4m-mvj7",
            "aliases": ["CVE-2024-47081"],
            "fixedIn": ["2.32.4"]
        }])
    );
}
/// artifactSearch advisories carry ids, link and fixes only: the advisory
/// text (`summary`, `details`) is securityCheck's, so a registry that sends
/// it must not put it on the row.
#[tokio::test]
async fn an_advisory_row_never_carries_the_registrys_advisory_text() {
    let mut release = pypi_release("2.31.0");
    release["vulnerabilities"] = json!([{
        "id": "GHSA-x", "aliases": ["CVE-1"], "summary": "S", "details": "D",
        "fixed_in": ["2.32.0"], "link": "https://osv.dev/vulnerability/GHSA-x"
    }]);
    let http = RouteHttp::routes(vec![("/pypi/requests/2.31.0/json", release)]);
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let page = pypi(
        &versioned(ArtifactType::Pypi, "requests", "2.31.0"),
        &client,
    )
    .await
    .expect("pypi");
    let row = serde_json::to_value(&page.artifacts[0]).expect("row");
    let advisory = row["advisories"][0].as_object().expect("advisory");
    for key in ["summary", "details"] {
        assert!(!advisory.contains_key(key), "{key} in {advisory:?}");
    }
    assert_eq!(advisory["id"], "GHSA-x");
}
