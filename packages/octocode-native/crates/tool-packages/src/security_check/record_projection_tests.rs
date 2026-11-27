//! Independent selected records through both providers and the advisory caller.
use super::*;
use crate::security_check::github_double::Double;
use crate::security_check::repository::RepoRead;
use octocode_test_support::json_fixture;

const ID: &str = "GHSA-aaaa-bbbb-cccc";

fn osv_record(references: Value) -> Value {
    json!({"id":ID,"summary":"shared summary","details":"shared details",
        "references":references})
}

fn github_record(extra: Value) -> octocode_github::RepositoryAdvisory {
    let mut fields = json_fixture(
        r#"{
            "summary": "shared summary",
            "description": "shared details",
            "references": [],
            "vulnerabilities": []
        }"#,
    );
    fields
        .as_object_mut()
        .expect("fields")
        .extend(extra.as_object().expect("extra fields").clone());
    github_advisory(ID, fields)
}

async fn whole_records(osv: Value, github: octocode_github::RepositoryAdvisory) -> Value {
    let mut expected_osv =
        serde_json::to_value(record::advisory(&osv, &record::Selection::All).expect("OSV record"))
            .expect("selected OSV row");
    // This fixture's OSV fixes all come from its own fixed events. The
    // selected view omits that same-record duplication, not GitHub's fixes.
    expected_osv.as_object_mut().expect("row").remove("fixedIn");
    // The OSV page its id names is stated once, in the output description.
    expected_osv.as_object_mut().expect("row").remove("link");
    let expected_github = serde_json::to_value(
        advisories::row(&github, &record::Selection::All).expect("GitHub record"),
    )
    .expect("selected GitHub row");
    let http = RouteHttp::new().json(&format!("/v1/vulns/{ID}"), osv);
    let github = Double::repo(RepoRead::Missing).ghsa(None, ID, GhsaRead::Found(Box::new(github)));
    let data = answer_on_github(&http, &github, json!({"operation":"advisory","id":ID}))
        .await
        .expect("both records");
    let rows = data["advisories"].as_array().expect("advisories");
    assert_eq!(rows.len(), 2, "{data}");
    assert_eq!(rows[0], expected_osv, "the first record is whole: {data}");
    assert_same_text_once(rows, &[expected_osv, expected_github]);
    assert!(
        !data["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|warning| {
                warning
                    .as_str()
                    .is_some_and(|text| text.contains("later one states only"))
            }),
        "no cross-source inheritance claim: {data}"
    );
    octocode_runtime_base::test_support::assert_row_contract(ToolId::SecurityCheck, &data);
    data
}

/// `rows` against the records as each source states them (`whole`): each
/// record keeps every field but a summary or details equal to an earlier
/// record's, line endings aside; a record leaving one out names, in
/// `textSameAs`, a record holding it (by `link`, or the OSV page its id
/// names, which the details view leaves unstated); and each distinct text
/// of the id is stated once.
fn assert_same_text_once(rows: &[Value], whole: &[Value]) {
    let unified = |value: &Value| value.as_str().map(|text| text.replace("\r\n", "\n"));
    let link_of = |row: &Value| {
        row.get("link").cloned().unwrap_or_else(|| {
            json!(format!(
                "https://osv.dev/vulnerability/{}",
                row["id"].as_str().unwrap_or_default()
            ))
        })
    };
    for (row, record) in rows.iter().zip(whole) {
        for (field, value) in record.as_object().expect("record") {
            match (field.as_str(), row.get(field)) {
                (_, Some(own)) => assert_eq!(own, value, "{field}: {row}"),
                ("summary" | "details", None) => {
                    let pointer = row.get("textSameAs").unwrap_or_else(|| panic!("{row}"));
                    let holder = rows
                        .iter()
                        .find(|held| &link_of(held) == pointer)
                        .unwrap_or_else(|| panic!("{pointer} resolves: {rows:?}"));
                    assert!(holder.get("textSameAs").is_none(), "{holder}");
                    assert_eq!(unified(&holder[field]), unified(value), "{field}");
                }
                (_, None) => panic!("{field} lost: {row}"),
            }
        }
        // Nothing but the pointer is added.
        for field in row.as_object().expect("row").keys() {
            assert!(
                field == "textSameAs" || record.get(field).is_some(),
                "{field} added: {row}"
            );
        }
    }
    for (field, row) in ["summary", "details"]
        .iter()
        .flat_map(|field| rows.iter().map(move |row| (*field, row)))
    {
        let stated = rows
            .iter()
            .filter(|other| other["id"] == row["id"])
            .filter_map(|other| unified(&other[field]))
            .collect::<Vec<_>>();
        let mut distinct = stated.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(
            stated.len(),
            distinct.len(),
            "{field} of {}: {stated:?}",
            row["id"]
        );
    }
}

#[tokio::test]
async fn references_keep_shared_and_unique_members() {
    whole_records(
        osv_record(json_fixture(
            r#"[
                {"type": "WEB", "url": "https://refs.example/shared"},
                {"type": "WEB", "url": "https://refs.example/first"}
            ]"#,
        )),
        github_record(json_fixture(
            r#"{
                "references": [
                    "https://refs.example/shared",
                    "https://refs.example/second"
                ]
            }"#,
        )),
    )
    .await;
}

#[tokio::test]
async fn references_keep_a_sources_subset() {
    whole_records(
        osv_record(json_fixture(
            r#"[
                {"type": "WEB", "url": "https://refs.example/one"},
                {"type": "WEB", "url": "https://refs.example/two"}
            ]"#,
        )),
        github_record(json_fixture(
            r#"{"references": ["https://refs.example/two"]}"#,
        )),
    )
    .await;
}

#[tokio::test]
async fn references_keep_types_even_for_the_same_url() {
    whole_records(
        osv_record(json_fixture(
            r#"[{"type": "ADVISORY", "url": "https://refs.example/shared"}]"#,
        )),
        github_record(json_fixture(
            r#"{"references": ["https://refs.example/shared"]}"#,
        )),
    )
    .await;
}

#[tokio::test]
async fn references_keep_each_sources_order() {
    whole_records(
        osv_record(json_fixture(
            r#"[
                {"type": "WEB", "url": "https://refs.example/one"},
                {"type": "WEB", "url": "https://refs.example/two"}
            ]"#,
        )),
        github_record(json_fixture(
            r#"{
                "references": ["https://refs.example/two", "https://refs.example/one"]
            }"#,
        )),
    )
    .await;
}

#[tokio::test]
async fn identical_references_are_kept_in_both_records() {
    whole_records(
        osv_record(json_fixture(
            r#"[{"type": "WEB", "url": "https://refs.example/shared"}]"#,
        )),
        github_record(json_fixture(
            r#"{"references": ["https://refs.example/shared"]}"#,
        )),
    )
    .await;
}

#[tokio::test]
async fn a_reference_to_the_records_own_page_is_not_borrowed_from_its_link() {
    whole_records(
        osv_record(json!([])),
        github_record(
            json!({"references":[format!("https://github.com/o/r/security/advisories/{ID}")]}),
        ),
    )
    .await;
}

#[tokio::test]
async fn empty_second_source_arrays_do_not_inherit_first_source_values() {
    let mut osv = osv_record(json_fixture(
        r#"[{"type": "WEB", "url": "https://refs.example/first"}]"#,
    ));
    osv["database_specific"] = json_fixture(r#"{"severity": "HIGH"}"#);
    osv["affected"] = json_fixture(
        r#"[
            {
                "package": {"name": "first", "ecosystem": "npm"},
                "versions": ["1"]
            }
        ]"#,
    );
    let data = whole_records(
        osv,
        github_record(json_fixture(r#"{"references": [], "vulnerabilities": []}"#)),
    )
    .await;
    let second = &data["advisories"][1];
    for key in ["references", "affected", "severity"] {
        assert!(
            second.get(key).is_none(),
            "no selected {key} in this source: {second}"
        );
    }
    assert_eq!(
        data["advisories"][0]["references"].as_array().map(Vec::len),
        Some(1)
    );
}

#[tokio::test]
async fn unstated_second_source_scalars_do_not_inherit_first_source_values() {
    let mut osv = osv_record(json_fixture(
        r#"[{"type": "WEB", "url": "https://refs.example/first"}]"#,
    ));
    osv["database_specific"] = json_fixture(r#"{"severity": "HIGH"}"#);
    let mut github = github_record(json_fixture(r#"{"summary": null, "description": null}"#));
    github.references.clear();
    let data = whole_records(osv, github).await;
    for key in ["summary", "details", "severity", "references"] {
        assert!(data["advisories"][1].get(key).is_none(), "{key}: {data}");
        assert!(
            data["advisories"][0].get(key).is_some(),
            "first source still states {key}"
        );
    }
}

#[tokio::test]
async fn partially_equal_affected_arrays_keep_every_source_entry() {
    let common = json_fixture(
        r#"{
            "package": {"name": "shared", "ecosystem": "npm"},
            "ecosystem_specific": {
                "vulnerable_version_range": "< 2.0.0",
                "patched_versions": null
            }
        }"#,
    );
    let mut osv = osv_record(json!([]));
    osv["affected"] = json!([common,{"package":{"name":"first","ecosystem":"npm","purl":"pkg:npm/first"},"versions":["1"]}]);
    whole_records(
        osv,
        github_record(json_fixture(
            r#"{
                "vulnerabilities": [
                    {
                        "package": {"name": "shared", "ecosystem": "npm"},
                        "vulnerable_version_range": "< 2.0.0",
                        "patched_versions": null
                    },
                    {
                        "package": {"name": "second", "ecosystem": "npm"},
                        "vulnerable_version_range": "< 3.0.0",
                        "patched_versions": null
                    }
                ]
            }"#,
        )),
    )
    .await;
}

#[tokio::test]
async fn differently_notated_affected_arrays_keep_their_complete_source_values() {
    let mut osv = osv_record(json_fixture(
        r#"[
            {"type": "WEB", "url": "https://refs.example/fix"},
            {"type": "ADVISORY", "url": "https://refs.example/cve"}
        ]"#,
    ));
    osv["affected"] = json_fixture(
        r#"[
            {
                "package": {"name": "shared", "ecosystem": "npm"},
                "ranges": [
                    {
                        "type": "SEMVER",
                        "events": [{"introduced": "8.3.0"}, {"fixed": "8.3.3"}]
                    }
                ],
                "database_specific": {"last_known_affected_version_range": "<= x"}
            },
            {
                "package": {"name": "shared", "ecosystem": "npm"},
                "ranges": [
                    {
                        "type": "SEMVER",
                        "events": [{"introduced": "0"}, {"fixed": "6.4.4"}]
                    }
                ]
            }
        ]"#,
    );
    whole_records(osv, github_record(json!({
        "references":["https://refs.example/cve",format!("https://github.com/o/r/security/advisories/{ID}"),"https://refs.example/new"],
        "vulnerabilities":[
            {"package":{"name":"shared","ecosystem":"npm"},"vulnerable_version_range":">= 8.3.0, <= 8.3.2","patched_versions":"8.3.3"},
            {"package":{"name":"shared","ecosystem":"npm"},"vulnerable_version_range":"<= 6.4.3","patched_versions":"6.4.4"},
            {"package":{"name":"shared","ecosystem":"npm"},"vulnerable_version_range":"> 8.2.0, <= 8.2.3","patched_versions":"8.2.4"},
            {"package":{"name":"shared","ecosystem":"npm"},"vulnerable_version_range":">=7.0.0, <7.3.7","patched_versions":"7.3.8"}
        ]
    }))).await;
}

#[tokio::test]
async fn a_second_sources_fix_is_not_removed_by_the_first_sources_events() {
    let mut osv = osv_record(json!([]));
    osv["affected"] = json_fixture(
        r#"[
            {
                "package": {"name": "shared", "ecosystem": "npm"},
                "ranges": [
                    {
                        "type": "SEMVER",
                        "events": [{"introduced": "0"}, {"fixed": "2.0.0"}]
                    }
                ]
            }
        ]"#,
    );
    let data = whole_records(
        osv,
        github_record(json_fixture(
            r#"{
                "vulnerabilities": [
                    {
                        "package": {"name": "shared", "ecosystem": "npm"},
                        "vulnerable_version_range": "< 2.0.0",
                        "patched_versions": "2.0.0"
                    }
                ]
            }"#,
        )),
    )
    .await;
    assert!(
        data["advisories"][0].get("fixedIn").is_none(),
        "own events already state it"
    );
    assert_eq!(
        data["advisories"][1]["fixedIn"],
        json_fixture(r#"["2.0.0"]"#)
    );
}

#[tokio::test]
async fn distinct_source_ids_keep_their_own_complete_records() {
    let mut osv = osv_record(json_fixture(
        r#"[{"type": "ADVISORY", "url": "https://refs.example/first"}]"#,
    ));
    osv["id"] = json!("PYSEC-2026-1");
    osv["aliases"] = json!([ID]);
    let data = whole_records(
        osv,
        github_record(json_fixture(
            r#"{"references": ["https://refs.example/second"]}"#,
        )),
    )
    .await;
    assert_eq!(data["advisories"][0]["id"], "PYSEC-2026-1");
    assert_eq!(data["advisories"][1]["id"], ID);
}
