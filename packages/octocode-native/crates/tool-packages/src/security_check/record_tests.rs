//! Each database's record as a row.
use super::*;
use crate::artifact::test_http::recorded;
use octocode_contract_types::ArtifactType;
use octocode_test_support::json_fixture;
use serde_json::json;

fn row(record: Value, kind: ArtifactType, name: &str) -> SecurityAdvisory {
    let names = [name.to_owned()];
    advisory(
        &record,
        &Selection::Named(
            crate::artifact::query::artifact_type::osv_ecosystem(kind),
            &names,
            None,
        ),
    )
    .expect("record")
}

#[test]
fn affected_package_identity_is_preserved_for_all_and_named() {
    let names = ["shared".to_owned()];
    let package = json_fixture(
        r#"{
            "name": "shared",
            "ecosystem": "Debian:12",
            "purl": "pkg:deb/debian/shared@1.2?arch=arm64&distro=debian-12"
        }"#,
    );
    let record = json!({"id": "X", "affected": [{"package": package}]});
    for selection in [
        Selection::All,
        Selection::Named(Ecosystem::Debian, &names, None),
        Selection::Named(Ecosystem::Debian, &names, Some("12")),
    ] {
        let item = advisory(&record, &selection).expect("record");
        assert_eq!(item.affected.len(), 1);
        for (source, shown) in [
            ("name", "packageName"),
            ("ecosystem", "ecosystem"),
            ("purl", "purl"),
        ] {
            assert_eq!(
                item.affected[0][shown], package[source],
                "{shown}: {item:?}"
            );
        }
    }
}

#[test]
fn affected_package_identity_is_not_invented_or_normalized() {
    for package in [
        json_fixture(r#"{"name": "shared"}"#),
        json_fixture(
            r#"{
                "name": "shared",
                "ecosystem": "FutureRegistry:edge",
                "purl": "pkg:generic/shared?variant=next"
            }"#,
        ),
    ] {
        let item = advisory(
            &json!({"id": "X", "affected": [{"package": package}]}),
            &Selection::All,
        )
        .expect("record");
        for field in ["ecosystem", "purl"] {
            assert_eq!(item.affected[0].get(field), package.get(field), "{item:?}");
        }
    }
}

#[test]
fn affected_package_identity_is_preserved_across_source_notations() {
    let package = json_fixture(
        r#"{
            "name": "shared",
            "ecosystem": "npm",
            "purl": "pkg:npm/shared"
        }"#,
    );
    let mut other_ecosystem = package.clone();
    other_ecosystem["ecosystem"] = json!("PyPI");
    other_ecosystem["purl"] = json!("pkg:pypi/shared");
    let mut other_purl = package.clone();
    other_purl["purl"] = json!("pkg:npm/shared?repository_url=https%3A%2F%2Fregistry.example");
    let mut no_ecosystem = package.clone();
    no_ecosystem
        .as_object_mut()
        .expect("package")
        .remove("ecosystem");
    let mut no_purl = package.clone();
    no_purl.as_object_mut().expect("package").remove("purl");
    for other in [
        package.clone(),
        other_ecosystem,
        other_purl,
        no_ecosystem,
        no_purl,
        json_fixture(r#"{"name": "shared"}"#),
    ] {
        let osv = json!({"id": "GHSA-same", "affected": [{"package": package,
            "ranges": [{"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "2.0.0"}]}]}]});
        let github = json!({"id": "GHSA-same", "affected": [{"package": other,
            "ecosystem_specific": {"vulnerable_version_range": "< 2.0.0", "first_patched_version": "2.0.0"}}]});
        for source in [&osv, &github] {
            let selected = advisory(source, &Selection::All).expect("record");
            assert_eq!(selected.affected.len(), 1);
            let entry = &selected.affected[0];
            let raw = &source["affected"][0];
            for (from, to) in PACKAGE_IDENTITY {
                // The ecosystem is shown as the `ecosystem` input spells it;
                // the name and purl stay as the source states them.
                let stated = raw["package"]
                    .get(from)
                    .cloned()
                    .map(|value| match value.as_str() {
                        Some(name) if from == "ecosystem" => {
                            json!(crate::security_check::ecosystem::as_input(name))
                        }
                        _ => value,
                    });
                assert_eq!(entry.get(to), stated.as_ref(), "{from}: {source}");
            }
            assert_eq!(entry.get("ranges"), raw.get("ranges"));
            assert_eq!(
                entry.get("ecosystemSpecific"),
                raw.get("ecosystem_specific")
            );
        }
    }
}

#[test]
fn a_github_record_keeps_its_source_severity_and_ranges() {
    let source = "https://github.com/github/advisory-database/blob/main/advisories/X.json";
    let item = row(
        json!({
            "id": "GHSA-29mw-wpgm-hmr9", "schema_version": "1.4.0",
            "aliases": ["CVE-2020-28500"], "summary": "ReDoS", "details": "Whole text.",
            "published": "2022-01-06T20:30:46Z", "modified": "2025-01-14T18:09:53Z",
            "severity": [{"type": "CVSS_V3", "score": "CVSS:3.1/AV:N"}],
            "database_specific": {"severity": "MODERATE", "cwe_ids": ["CWE-400"],
                "github_reviewed": true, "nvd_published_at": "2021-02-15T11:15:00Z"},
            "references": [{"type": "ADVISORY", "url": "https://nvd.nist.gov/x"}],
            "credits": [{"name": "dropped"}],
            "affected": [{
                "package": {"name": "lodash", "ecosystem": "npm", "purl": "pkg:npm/lodash"},
                "ranges": [{"type": "SEMVER", "events": [{"introduced": "4.0.0"}, {"fixed": "4.17.21"}]}],
                "database_specific": {"source": source, "last_known_affected_version_range": "<= 4.17.20"}
            }]
        }),
        ArtifactType::Npm,
        "lodash",
    );
    assert_eq!(
        item.link,
        "https://osv.dev/vulnerability/GHSA-29mw-wpgm-hmr9"
    );
    assert_eq!(item.source.as_deref(), Some(source));
    assert_eq!(item.severity.as_deref(), Some("moderate"));
    assert_eq!(item.cwes, ["CWE-400"]);
    assert_eq!(item.fixed_in, ["4.17.21"]);
    assert_eq!(item.aliases, ["CVE-2020-28500"]);
    // OSV names the vector `score`; the row names it `vector` and states no
    // score, as OSV gives none.
    assert_eq!(
        item.cvss,
        [json_fixture(
            r#"{"type": "CVSS_V3", "vector": "CVSS:3.1/AV:N"}"#
        )]
    );
    // Per-entry source metadata stays intact even when its range text
    // describes the same versions as the events.
    assert_eq!(
        item.affected,
        [json!({
            "packageName": "lodash",
            "ecosystem": "npm",
            "purl": "pkg:npm/lodash",
            "ranges": [{"type": "SEMVER", "events": [{"introduced": "4.0.0"}, {"fixed": "4.17.21"}]}],
            "databaseSpecific": {"source": source, "last_known_affected_version_range": "<= 4.17.20"},
        })]
    );
    let printed = serde_json::to_string(&item).expect("row");
    for dropped in [
        "github_reviewed",
        "nvd_published_at",
        "credits",
        "schema_version",
    ] {
        assert!(!printed.contains(dropped), "{dropped}: {printed}");
    }
}

/// Each source spells severity and its source link its own way.
#[test]
fn severity_and_source_read_each_databases_spelling() {
    let go = row(
        json_fixture(
            r#"{
                "id": "GO-1",
                "database_specific": {"url": "https://pkg.go.dev/vuln/GO-1"},
                "affected": [
                    {
                        "package": {"name": "golang.org/x/text", "ecosystem": "Go"},
                        "ecosystem_specific": {"imports": [{"path": "golang.org/x/text/language"}]},
                        "ranges": [{"type": "SEMVER", "events": [{"fixed": "0.3.8"}]}]
                    }
                ]
            }"#,
        ),
        ArtifactType::Go,
        "golang.org/x/text",
    );
    assert_eq!(go.source.as_deref(), Some("https://pkg.go.dev/vuln/GO-1"));
    assert_eq!(go.severity, None, "a label is never defaulted");
    assert_eq!(
        go.affected[0]["ecosystemSpecific"]["imports"][0]["path"],
        "golang.org/x/text/language"
    );
    let labelled = row(
        json_fixture(
            r#"{
                "id": "X",
                "affected": [
                    {
                        "package": {"name": "a", "ecosystem": "npm"},
                        "ecosystem_specific": {"severity": "HIGH"}
                    }
                ]
            }"#,
        ),
        ArtifactType::Npm,
        "a",
    );
    assert_eq!(labelled.severity.as_deref(), Some("high"));
}

#[test]
fn a_commit_is_no_fixed_version() {
    let item = row(
        json_fixture(
            r#"{
                "id": "X",
                "affected": [
                    {
                        "package": {"name": "a", "ecosystem": "npm"},
                        "ranges": [
                            {
                                "type": "GIT",
                                "repo": "https://example.test/r",
                                "events": [{"fixed": "abc123"}]
                            },
                            {"type": "ECOSYSTEM", "events": [{"fixed": "2.0.0"}]},
                            {
                                "type": "SEMVER",
                                "events": [{"fixed": "2.0.0"}, {"fixed": "3.0.0"}]
                            }
                        ]
                    }
                ]
            }"#,
        ),
        ArtifactType::Npm,
        "a",
    );
    assert_eq!(item.fixed_in, ["2.0.0", "3.0.0"]);
    assert_eq!(
        item.affected[0]["ranges"][0]["type"], "GIT",
        "kept in the ranges"
    );
    let git_only = row(
        json_fixture(
            r#"{
                "id": "X",
                "affected": [
                    {
                        "package": {"name": "a", "ecosystem": "npm"},
                        "ranges": [{"type": "GIT", "events": [{"fixed": "abc123"}]}]
                    }
                ]
            }"#,
        ),
        ArtifactType::Npm,
        "a",
    );
    assert!(git_only.fixed_in.is_empty());
    assert_eq!(git_only.affected.len(), 1);
}

#[test]
fn entries_select_by_the_ecosystems_own_name_rule() {
    let entry =
        |name: &str, ecosystem: &str| json!({"package": {"name": name, "ecosystem": ecosystem}});
    for (kind, stated, osv, asked, own) in [
        (ArtifactType::Pypi, "PyPI", "PyYAML", "pyyaml", true),
        (
            ArtifactType::Pypi,
            "PyPI",
            "zope.interface",
            "Zope-Interface",
            true,
        ),
        (ArtifactType::Pypi, "PyPI", "a__b", "a-b", true),
        (ArtifactType::Pypi, "PyPI", "ab", "a-b", false),
        (
            ArtifactType::Nuget,
            "NuGet",
            "Newtonsoft.Json",
            "newtonsoft.json",
            true,
        ),
        (
            ArtifactType::Crates,
            "crates.io",
            "Foo_Bar",
            "foo-bar",
            true,
        ),
        (ArtifactType::Npm, "npm", "LODASH", "lodash", false),
        (ArtifactType::Maven, "Maven", "g:A", "g:a", false),
        (
            ArtifactType::Go,
            "Go",
            "golang.org/x/Text",
            "golang.org/x/text",
            false,
        ),
        (ArtifactType::Packagist, "Packagist", "O/R", "o/r", false),
        (ArtifactType::Rubygems, "RubyGems", "Rack", "rack", false),
        (ArtifactType::Npm, "PyPI", "lodash", "lodash", false),
    ] {
        let names = [asked.to_owned()];
        assert_eq!(
            matches!(
                scope(
                    &entry(osv, stated),
                    &Selection::Named(
                        crate::artifact::query::artifact_type::osv_ecosystem(kind),
                        &names,
                        None
                    )
                ),
                Scope::Own
            ),
            own,
            "{kind:?} {osv} {asked}"
        );
    }
}

/// A record names other packages too: they are disclosed, and a record with no
/// entry for this package is kept, never dropped.
#[test]
fn other_packages_are_disclosed_and_no_record_is_dropped() {
    let item = row(
        json_fixture(
            r#"{
                "id": "X",
                "affected": [
                    {"package": {"name": "lodash", "ecosystem": "npm"}},
                    {"package": {"name": "lodash-es", "ecosystem": "npm"}},
                    {"package": {"name": "lodash.merge", "ecosystem": "npm"}},
                    {"package": {"name": "lodash-es", "ecosystem": "npm"}}
                ]
            }"#,
        ),
        ArtifactType::Npm,
        "lodash",
    );
    assert_eq!(item.also_affects, ["lodash-es", "lodash.merge"]);
    let none_own = row(
        json_fixture(
            r#"{
                "id": "X",
                "affected": [{"package": {"name": "other", "ecosystem": "npm"}}]
            }"#,
        ),
        ArtifactType::Npm,
        "lodash",
    );
    assert_eq!(none_own.id, "X");
    assert!(none_own.affected.is_empty());
    assert_eq!(none_own.also_affects, ["other"]);
}

#[test]
fn a_withdrawn_record_keeps_the_field() {
    let item = row(
        json_fixture(r#"{"id": "X", "withdrawn": "2024-01-01T00:00:00Z"}"#),
        ArtifactType::Npm,
        "a",
    );
    assert_eq!(item.withdrawn.as_deref(), Some("2024-01-01T00:00:00Z"));
}

/// A withdrawal stated by either record of one id (OSV's and GitHub's twin of
/// it) is that record's, whichever leads the group; the advisory is withdrawn
/// when every record in it is.
#[test]
fn a_withdrawal_stated_by_either_record_of_an_id_is_the_advisorys() {
    let osv = row(
        json_fixture(r#"{"id": "GHSA-aaaa-bbbb-cccc", "aliases": ["CVE-2024-1"]}"#),
        ArtifactType::Npm,
        "a",
    );
    let github = row(
        json_fixture(
            r#"{
                "id": "GHSA-aaaa-bbbb-cccc",
                "aliases": ["CVE-2024-1"],
                "withdrawn": "2024-01-01T00:00:00Z"
            }"#,
        ),
        ArtifactType::Npm,
        "a",
    );
    for group in [[&osv, &github], [&github, &osv]] {
        assert_eq!(
            group_brief(&group)["withdrawn"],
            "2024-01-01T00:00:00Z",
            "{:?}",
            group.map(|record| &record.id)
        );
        assert!(Standing::of(&group).is_withdrawn());
    }
}

/// A live record keeps its advisory live whichever record leads the group: a
/// withdrawn record of another id (a duplicate sharing its CVE) is stated
/// apart, and the row is the live record's.
#[test]
fn a_live_record_keeps_the_advisory_live_beside_a_withdrawn_duplicate() {
    let live = row(
        json_fixture(r#"{"id": "GHSA-live-yyyy-zzzz", "aliases": ["CVE-2024-1"]}"#),
        ArtifactType::Npm,
        "a",
    );
    let duplicate = row(
        json_fixture(
            r#"{
                "id": "PYSEC-2024-1",
                "aliases": ["CVE-2024-1"],
                "withdrawn": "2024-01-01T00:00:00Z"
            }"#,
        ),
        ArtifactType::Npm,
        "a",
    );
    for group in [[&live, &duplicate], [&duplicate, &live]] {
        let standing = Standing::of(&group);
        assert!(!standing.is_withdrawn());
        assert_eq!(standing.live.len(), 1);
        assert_eq!(
            standing.withdrawn,
            [("PYSEC-2024-1", "2024-01-01T00:00:00Z")]
        );
        let brief = group_brief(&group);
        assert_eq!(brief["id"], "GHSA-live-yyyy-zzzz", "{brief}");
        assert!(brief.get("withdrawn").is_none(), "{brief}");
        assert_eq!(
            brief["aliases"],
            json_fixture(r#"["PYSEC-2024-1"]"#),
            "{brief}"
        );
    }
    // Every record withdrawn: the advisory is.
    let also = row(
        json_fixture(
            r#"{
                "id": "GHSA-live-yyyy-zzzz",
                "withdrawn": "2024-02-01T00:00:00Z"
            }"#,
        ),
        ArtifactType::Npm,
        "a",
    );
    assert!(Standing::of(&[&also, &duplicate]).is_withdrawn());
}

/// A brief row leads with what decides an upgrade and names none of the record's
/// internals; the whole record stays one call away.
#[test]
fn a_brief_row_states_only_what_decides_an_upgrade() {
    let item = row(
        json_fixture(
            r#"{
                "id": "GHSA-29mw-wpgm-hmr9",
                "schema_version": "1.4.0",
                "aliases": ["CVE-2020-28500", "OTHER-1"],
                "summary": "ReDoS",
                "details": "Whole text.",
                "severity": [{"type": "CVSS_V3", "score": "CVSS:3.1/AV:N"}],
                "database_specific": {"severity": "MODERATE"},
                "affected": [
                    {
                        "package": {"name": "lodash", "ecosystem": "npm"},
                        "database_specific": {"source": "https://example.test/source.json"},
                        "ranges": [
                            {
                                "type": "SEMVER",
                                "events": [{"introduced": "4.0.0"}, {"fixed": "4.17.21"}]
                            }
                        ]
                    }
                ]
            }"#,
        ),
        ArtifactType::Npm,
        "lodash",
    );
    assert_eq!(
        item.brief(),
        json_fixture(
            r#"{
                "id": "GHSA-29mw-wpgm-hmr9",
                "cves": ["CVE-2020-28500"],
                "severity": "moderate",
                "fixedIn": ["4.17.21"],
                "link": "https://osv.dev/vulnerability/GHSA-29mw-wpgm-hmr9"
            }"#
        )
    );
    // No severity word: the vector as published. No text is on a list row.
    let bare = row(
        json_fixture(
            r#"{
                "id": "PYSEC-1",
                "details": "\n First line.\nSecond line.",
                "severity": [{"type": "CVSS_V3", "score": "CVSS:3.1/AV:L"}]
            }"#,
        ),
        ArtifactType::Pypi,
        "x",
    );
    assert!(bare.brief().get("summary").is_none());
    assert_eq!(bare.brief()["cvss"][0]["vector"], "CVSS:3.1/AV:L");
    assert!(
        bare.brief()["cvss"][0].get("score").is_none(),
        "none computed"
    );
    assert!(bare.brief().get("severity").is_none());
}

/// What OSV really sends. A record's whole row holds its vectors and its word.
#[test]
fn a_recorded_record_keeps_its_vectors_and_its_word() {
    let record = recorded::answer("osv/advisory-ghsa", "/v1/vulns/");
    let item = row(record.clone(), ArtifactType::Npm, "lodash");
    let vectors = record["severity"].as_array().expect("OSV's severity list");
    assert!(!vectors.is_empty());
    assert_eq!(item.cvss, as_vectors(vectors));
    let word = record["database_specific"]["severity"]
        .as_str()
        .expect("a word");
    assert_eq!(item.severity.as_deref(), Some(word.to_lowercase().as_str()));
}

/// A real record with a score and no word: the brief row shows the vector as published.
#[test]
fn a_recorded_record_with_no_word_shows_its_vector() {
    // Native asks `/v1/querybatch` for the ids, then `/v1/vulns/{id}` for each record.
    let recording = recorded::recording("osv/package-score-only");
    let bare = recording["exchanges"]
        .as_array()
        .expect("exchanges")
        .iter()
        .filter(|exchange| {
            exchange["request"]["url"]
                .as_str()
                .is_some_and(|url| url.contains("/v1/vulns/"))
        })
        .map(|exchange| &exchange["body"])
        .find(|record| {
            record["severity"].is_array() && record["database_specific"]["severity"].is_null()
        })
        .expect("a record with no word");
    let item = row(bare.clone(), ArtifactType::Pypi, "requests");
    let vectors = bare["severity"].as_array().expect("OSV's severity list");
    assert_eq!(item.brief()["cvss"], json!(as_vectors(vectors)));
}

/// OSV's severity list as the row states it: each vector under `vector`.
fn as_vectors(severity: &[Value]) -> Vec<Value> {
    severity
        .iter()
        .map(|entry| json!({"type": entry["type"], "vector": entry["score"]}))
        .collect()
}

/// OSV's `severity` holds CVSS vectors and, for Ubuntu's records, a priority
/// word (`{type:"Ubuntu", score:"medium"}`). Only a CVSS vector is a `cvss`
/// entry; the word is the record's severity when it states no other.
#[test]
fn a_priority_word_in_osvs_severity_is_no_cvss_vector() {
    let vector = "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N";
    let record = json!({"id": "UBUNTU-CVE-2022-1292",
        "severity": [{"type": "CVSS_V3", "score": vector}, {"type": "Ubuntu", "score": "medium"}],
        "affected": [{"package": {"name": "lodash", "ecosystem": "npm"}}]});
    let item = row(record, ArtifactType::Npm, "lodash");
    assert_eq!(
        item.cvss,
        [json!({"type": "CVSS_V3", "vector": vector})],
        "only the vector"
    );
    assert_eq!(item.severity.as_deref(), Some("medium"), "the word kept");
    // A database's own word still wins over the priority.
    let worded = json_fixture(
        r#"{
            "id": "UBUNTU-CVE-2022-1293",
            "severity": [{"type": "Ubuntu", "score": "low"}],
            "database_specific": {"severity": "HIGH"},
            "affected": [{"package": {"name": "lodash", "ecosystem": "npm"}}]
        }"#,
    );
    let item = row(worded, ArtifactType::Npm, "lodash");
    assert!(item.cvss.is_empty(), "{:?}", item.cvss);
    assert_eq!(item.severity.as_deref(), Some("high"));
}

fn twin(provider: &str, id: &str) -> SecurityAdvisory {
    SecurityAdvisory {
        id: id.to_owned(),
        link: format!("https://{provider}.example/{id}"),
        ..SecurityAdvisory::default()
    }
}

/// A list row gives one reference: the record's link, and its database's
/// file (`source`) only when it has no link. The record's own row keeps both.
#[test]
fn a_brief_row_gives_one_reference() {
    let mut record = twin("osv", "GHSA-1");
    record.source = Some("https://github.example/advisories/GHSA-1.json".into());
    let brief = record.brief();
    assert_eq!(
        brief,
        json!({"id": "GHSA-1", "link": record.link}),
        "{brief}"
    );
    let whole = serde_json::to_value(&record).expect("row");
    assert_eq!(whole["source"], json!(record.source));
    record.link.clear();
    assert_eq!(record.brief()["source"], json!(record.source));
}

/// The most severe advisory leads a list; rows of one severity keep their order.
#[test]
fn brief_rows_list_the_most_severe_first() {
    let mut rows = vec![
        json_fixture(r#"{"id": "a", "severity": "low"}"#),
        json_fixture(r#"{"id": "b"}"#),
        json_fixture(r#"{"id": "c", "severity": "moderate"}"#),
        json_fixture(r#"{"id": "d", "severity": "critical"}"#),
        json_fixture(r#"{"id": "e", "severity": "medium"}"#),
        json_fixture(r#"{"id": "f", "severity": "high"}"#),
    ];
    most_severe_first(&mut rows);
    let ids = rows
        .iter()
        .map(|row| row["id"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["d", "f", "c", "e", "a", "b"].map(Some));
}

#[test]
fn selected_affected_entries_keep_their_opaque_metadata_and_attribution() {
    let entries = json_fixture(
        r#"[
            {
                "package": {
                    "name": "shared",
                    "ecosystem": "npm",
                    "purl": "pkg:npm/shared"
                },
                "ranges": [
                    {
                        "type": "SEMVER",
                        "events": [{"introduced": "0"}, {"fixed": "2.0.0"}]
                    }
                ],
                "database_specific": {
                    "source": "https://records.example/a",
                    "last_known_affected_version_range": "< 2.0.0",
                    "extra": {"source": "nested-a"}
                },
                "ecosystem_specific": {"source": "ecosystem-a", "other": [1, 2]}
            },
            {
                "package": {
                    "name": "shared",
                    "ecosystem": "npm",
                    "purl": "pkg:npm/shared?variant=b"
                },
                "database_specific": {
                    "source": "https://records.example/b",
                    "last_known_affected_version_range": "opaque range"
                },
                "ecosystem_specific": {"source": "ecosystem-b", "other": {"flag": true}}
            }
        ]"#,
    );
    let source = json!({"id":"GHSA-metadata","affected":entries});
    let names = ["shared".to_owned()];
    for selection in [
        Selection::All,
        Selection::Named(Ecosystem::Npm, &names, None),
    ] {
        let selected = advisory(&source, &selection).expect("selected record");
        assert_eq!(
            selected.source.as_deref(),
            Some("https://records.example/a")
        );
        assert_eq!(selected.affected.len(), 2);
        for (actual, original) in selected
            .affected
            .iter()
            .zip(entries.as_array().expect("entries"))
        {
            assert_eq!(actual["databaseSpecific"], original["database_specific"]);
            assert_eq!(actual["ecosystemSpecific"], original["ecosystem_specific"]);
            for (raw, shown) in [
                ("name", "packageName"),
                ("ecosystem", "ecosystem"),
                ("purl", "purl"),
            ] {
                assert_eq!(actual[shown], original["package"][raw]);
            }
        }
    }
}

#[test]
fn fix_redundancy_uses_only_the_same_records_events() {
    let mut rows = vec![
        json_fixture(
            r#"{
                "id": "same",
                "fixedIn": ["2.0.0"],
                "affected": [{"ranges": [{"events": [{"fixed": "2.0.0"}]}]}]
            }"#,
        ),
        json_fixture(
            r#"{
                "id": "same",
                "fixedIn": ["2.0.0"],
                "affected": [{"ecosystemSpecific": {"first_patched_version": "2.0.0"}}]
            }"#,
        ),
    ];
    drop_restated_fixes(&mut rows);
    assert!(
        rows[0].get("fixedIn").is_none(),
        "same-record duplication is omitted"
    );
    assert_eq!(
        rows[1]["fixedIn"],
        json_fixture(r#"["2.0.0"]"#),
        "another record cannot erase this statement"
    );
}

/// Records of one id from several sources (OSV, GitHub's database, the
/// repository's advisory) state their text once: a later record's summary or
/// details equal to an earlier one's, line endings aside, is left out and
/// `textSameAs` names the record that holds it; text that differs, every
/// other field, and a record of another id keep their own.
#[test]
fn twin_records_state_their_text_once() {
    let text = "## Summary\nA flaw.\n\n## Impact\nData read.";
    let mut rows = vec![
        json!({"id": "GHSA-a", "link": "https://osv.dev/vulnerability/GHSA-a",
            "summary": "S", "details": text, "severity": "high"}),
        json!({"id": "GHSA-a", "link": "https://github.com/o/r/security/advisories/GHSA-a",
            "summary": "S", "details": text.replace('\n', "\r\n"), "severity": "high"}),
        json!({"id": "GHSA-a", "link": "https://github.com/advisories/GHSA-a",
            "summary": "S", "details": format!("{text} More."), "cwes": ["CWE-1"]}),
        json!({"id": "GHSA-a", "link": "https://example/edited",
            "summary": "S edited", "details": format!("{text} Other.")}),
        json!({"id": "GHSA-b", "link": "https://osv.dev/vulnerability/GHSA-b",
            "summary": "S", "details": text}),
    ];
    let before = rows.clone();
    drop_restated_text(&mut rows);
    let unified = |value: &Value| value.as_str().map(|text| text.replace("\r\n", "\n"));
    for (at, (kept, was)) in rows.iter().zip(&before).enumerate() {
        // Every field but the text stays as the record stated it.
        for (field, value) in was.as_object().expect("row") {
            if field != "summary" && field != "details" {
                assert_eq!(&kept[field], value, "row {at} {field}");
            }
        }
        for field in ["summary", "details"] {
            match (kept.get(field), kept.get("textSameAs")) {
                (Some(own), _) => assert_eq!(own, &was[field], "row {at} {field}"),
                // An omitted text is held, equal, by the record its pointer names.
                (None, Some(pointer)) => {
                    let holder = rows
                        .iter()
                        .find(|row| &row["link"] == pointer && row["id"] == kept["id"])
                        .unwrap_or_else(|| panic!("row {at}: {pointer} resolves"));
                    assert!(holder.get("textSameAs").is_none(), "row {at}");
                    assert_eq!(unified(&holder[field]), unified(&was[field]), "row {at}");
                }
                (None, None) => panic!("row {at} lost {field} with no pointer"),
            }
        }
    }
    // Each distinct text of an id is stated exactly once.
    for id in ["GHSA-a", "GHSA-b"] {
        for field in ["summary", "details"] {
            let stated = rows
                .iter()
                .filter(|row| row["id"] == id)
                .filter_map(|row| unified(&row[field]))
                .collect::<Vec<_>>();
            let mut distinct = stated.clone();
            distinct.sort();
            distinct.dedup();
            assert_eq!(stated.len(), distinct.len(), "{id} {field}: {stated:?}");
        }
    }
    assert!(rows[1].get("details").is_none() && rows[1].get("summary").is_none());
    assert_eq!(
        rows[2]["textSameAs"], before[0]["link"],
        "summary only: {}",
        rows[2]
    );
    assert!(rows[3].get("textSameAs").is_none(), "{}", rows[3]);
    assert!(
        rows[4].get("textSameAs").is_none(),
        "another id: {}",
        rows[4]
    );
}

/// A compact row never pairs one record's severity word with another's CVSS
/// vectors: a lead record with vectors only, beside a twin with a word, states
/// the word alone.
#[test]
fn a_row_never_pairs_a_twins_severity_with_the_leads_vectors() {
    let vector = "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:L/I:N/A:N";
    let cve = row(
        json_fixture(&format!(
            r#"{{"id": "CVE-2024-1", "severity": [{{"type": "CVSS_V3", "score": "{vector}"}}]}}"#
        )),
        ArtifactType::Npm,
        "a",
    );
    let ghsa = row(
        json_fixture(
            r#"{"id": "GHSA-aaaa-bbbb-cccc", "aliases": ["CVE-2024-1"],
                "database_specific": {"severity": "HIGH"}}"#,
        ),
        ArtifactType::Npm,
        "a",
    );
    for group in [[&cve, &ghsa], [&ghsa, &cve]] {
        let brief = group_brief(&group);
        assert_eq!(brief["severity"], "high", "{brief}");
        assert!(brief.get("cvss").is_none(), "{brief}");
    }
    // Alone, the vectors are still stated.
    assert_eq!(group_brief(&[&cve])["cvss"][0]["vector"], vector);
}

/// A CVE the row's own id names is not restated in `cves`; another CVE is.
#[test]
fn a_brief_row_lists_no_cve_that_is_only_its_id() {
    let own = row(
        json!({"id": "CVE-2021-23337", "aliases": []}),
        ArtifactType::Npm,
        "lodash",
    )
    .brief();
    assert!(own.get("cves").is_none(), "{own}");
    let alias = row(
        json!({"id": "GHSA-35jh-r3h4-6jhm", "aliases": ["CVE-2021-23337"]}),
        ArtifactType::Npm,
        "lodash",
    )
    .brief();
    assert_eq!(alias["cves"], json!(["CVE-2021-23337"]), "{alias}");
}
