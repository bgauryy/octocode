//! Each database's record as a row.
use super::*;
use crate::providers::artifact::test_http::recorded;
use octocode_contract_types::ArtifactType;
use serde_json::json;

fn row(record: Value, kind: ArtifactType, name: &str) -> SecurityAdvisory {
    let names = [name.to_owned()];
    advisory(
        &record,
        &Selection::Named(
            crate::providers::artifact::query::artifact_type::osv_ecosystem(kind),
            &names,
            None,
        ),
    )
    .expect("record")
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
        [json!({"type": "CVSS_V3", "vector": "CVSS:3.1/AV:N"})]
    );
    // GitHub's `<= 4.17.20` restates the events: stated once, by them.
    assert_eq!(
        item.affected,
        [json!({
            "packageName": "lodash",
            "ranges": [{"type": "SEMVER", "events": [{"introduced": "4.0.0"}, {"fixed": "4.17.21"}]}],
        })]
    );
    let printed = serde_json::to_string(&item).expect("row");
    for dropped in [
        "github_reviewed",
        "nvd_published_at",
        "credits",
        "schema_version",
        "purl",
    ] {
        assert!(!printed.contains(dropped), "{dropped}: {printed}");
    }
}

/// Each source spells severity and its source link its own way.
#[test]
fn severity_and_source_read_each_databases_spelling() {
    let go = row(
        json!({"id": "GO-1", "database_specific": {"url": "https://pkg.go.dev/vuln/GO-1"},
            "affected": [{"package": {"name": "golang.org/x/text", "ecosystem": "Go"},
                "ecosystem_specific": {"imports": [{"path": "golang.org/x/text/language"}]},
                "ranges": [{"type": "SEMVER", "events": [{"fixed": "0.3.8"}]}]}]}),
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
        json!({"id": "X", "affected": [{"package": {"name": "a", "ecosystem": "npm"},
            "ecosystem_specific": {"severity": "HIGH"}}]}),
        ArtifactType::Npm,
        "a",
    );
    assert_eq!(labelled.severity.as_deref(), Some("high"));
}

#[test]
fn a_commit_is_no_fixed_version() {
    let item = row(
        json!({"id": "X", "affected": [{"package": {"name": "a", "ecosystem": "npm"}, "ranges": [
            {"type": "GIT", "repo": "https://example.test/r", "events": [{"fixed": "abc123"}]},
            {"type": "ECOSYSTEM", "events": [{"fixed": "2.0.0"}]},
            {"type": "SEMVER", "events": [{"fixed": "2.0.0"}, {"fixed": "3.0.0"}]}
        ]}]}),
        ArtifactType::Npm,
        "a",
    );
    assert_eq!(item.fixed_in, ["2.0.0", "3.0.0"]);
    assert_eq!(
        item.affected[0]["ranges"][0]["type"], "GIT",
        "kept in the ranges"
    );
    let git_only = row(
        json!({"id": "X", "affected": [{"package": {"name": "a", "ecosystem": "npm"},
            "ranges": [{"type": "GIT", "events": [{"fixed": "abc123"}]}]}]}),
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
                        crate::providers::artifact::query::artifact_type::osv_ecosystem(kind),
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
        json!({"id": "X", "affected": [
            {"package": {"name": "lodash", "ecosystem": "npm"}},
            {"package": {"name": "lodash-es", "ecosystem": "npm"}},
            {"package": {"name": "lodash.merge", "ecosystem": "npm"}},
            {"package": {"name": "lodash-es", "ecosystem": "npm"}}
        ]}),
        ArtifactType::Npm,
        "lodash",
    );
    assert_eq!(item.also_affects, ["lodash-es", "lodash.merge"]);
    let none_own = row(
        json!({"id": "X", "affected": [{"package": {"name": "other", "ecosystem": "npm"}}]}),
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
        json!({"id": "X", "withdrawn": "2024-01-01T00:00:00Z"}),
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
        json!({"id": "GHSA-aaaa-bbbb-cccc", "aliases": ["CVE-2024-1"]}),
        ArtifactType::Npm,
        "a",
    );
    let github = row(
        json!({"id": "GHSA-aaaa-bbbb-cccc", "aliases": ["CVE-2024-1"],
            "withdrawn": "2024-01-01T00:00:00Z"}),
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
        json!({"id": "GHSA-live-yyyy-zzzz", "aliases": ["CVE-2024-1"]}),
        ArtifactType::Npm,
        "a",
    );
    let duplicate = row(
        json!({"id": "PYSEC-2024-1", "aliases": ["CVE-2024-1"],
            "withdrawn": "2024-01-01T00:00:00Z"}),
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
        assert_eq!(brief["aliases"], json!(["PYSEC-2024-1"]), "{brief}");
    }
    // Every record withdrawn: the advisory is.
    let also = row(
        json!({"id": "GHSA-live-yyyy-zzzz", "withdrawn": "2024-02-01T00:00:00Z"}),
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
        json!({
            "id": "GHSA-29mw-wpgm-hmr9", "schema_version": "1.4.0",
            "aliases": ["CVE-2020-28500", "OTHER-1"], "summary": "ReDoS", "details": "Whole text.",
            "severity": [{"type": "CVSS_V3", "score": "CVSS:3.1/AV:N"}],
            "database_specific": {"severity": "MODERATE"},
            "affected": [{"package": {"name": "lodash", "ecosystem": "npm"},
                "database_specific": {"source": "https://example.test/source.json"},
                "ranges": [{"type": "SEMVER", "events": [{"introduced": "4.0.0"}, {"fixed": "4.17.21"}]}]}]
        }),
        ArtifactType::Npm,
        "lodash",
    );
    assert_eq!(
        item.brief(),
        json!({
            "id": "GHSA-29mw-wpgm-hmr9",
            "cves": ["CVE-2020-28500"],
            "severity": "moderate",
            "fixedIn": ["4.17.21"],
            "link": "https://osv.dev/vulnerability/GHSA-29mw-wpgm-hmr9",
        })
    );
    // No severity word: the vector as published. No text is on a list row.
    let bare = row(
        json!({"id": "PYSEC-1", "details": "\n First line.\nSecond line.",
            "severity": [{"type": "CVSS_V3", "score": "CVSS:3.1/AV:L"}]}),
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
    let worded = json!({"id": "UBUNTU-CVE-2022-1293",
        "severity": [{"type": "Ubuntu", "score": "low"}],
        "database_specific": {"severity": "HIGH"},
        "affected": [{"package": {"name": "lodash", "ecosystem": "npm"}}]});
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
        json!({"id": "a", "severity": "low"}),
        json!({"id": "b"}),
        json!({"id": "c", "severity": "moderate"}),
        json!({"id": "d", "severity": "critical"}),
        json!({"id": "e", "severity": "medium"}),
        json!({"id": "f", "severity": "high"}),
    ];
    most_severe_first(&mut rows);
    let ids = rows
        .iter()
        .map(|row| row["id"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["d", "f", "c", "e", "a", "b"].map(Some));
}

/// The details rows of one id: the later record keeps its id and only the
/// fields its source states otherwise; a record of another id is untouched.
#[test]
fn a_twin_record_states_only_its_differences() {
    let mut rows = vec![
        json!({"id": "GHSA-a", "summary": "s", "details": "long text", "link": "https://osv.dev/a",
            "cwes": ["CWE-77"], "modified": "2026-10-20T20:00:04Z"}),
        json!({"id": "GHSA-a", "summary": "s", "details": "long text", "link": "https://github.com/advisories/a",
            "cwes": ["CWE-77"], "modified": "2026-10-20T19:48:26Z", "epss": {"percentage": 0.01}}),
        json!({"id": "GHSA-b", "summary": "s", "details": "long text"}),
    ];
    let twins = state_twin_differences(&mut rows);
    assert_eq!(twins, ["GHSA-a"]);
    assert_eq!(
        rows[1],
        json!({"id": "GHSA-a", "link": "https://github.com/advisories/a",
            "modified": "2026-10-20T19:48:26Z", "epss": {"percentage": 0.01}})
    );
    assert_eq!(rows[0]["details"], "long text");
    assert_eq!(rows[2]["details"], "long text");
}

/// A difference of notation is none: GitHub's text range with the same lower
/// bound and fix as OSV's events, and a reference to a URL already stated,
/// are left out; a range that differs, and a new URL, stay.
#[test]
fn a_twin_restates_no_range_or_reference_in_another_notation() {
    let osv_range = |name: &str, introduced: &str, fixed: &str| {
        json!({"packageName": name, "ranges": [{"type": "SEMVER", "events": [
            {"introduced": introduced}, {"fixed": fixed}]}],
            "databaseSpecific": {"last_known_affected_version_range": "<= x"}})
    };
    let github_range = |name: &str, range: &str, first: &str| {
        json!({"packageName": name, "ecosystemSpecific": {"vulnerable_version_range": range,
            "patched_versions": null, "first_patched_version": first}})
    };
    let mut rows = vec![
        json!({"id": "GHSA-a", "link": "https://osv.dev/a",
            "affected": [osv_range("vite", "8.3.0", "8.3.3"), osv_range("vite", "0", "6.4.4")],
            "references": [{"type": "WEB", "url": "https://example.test/fix"},
                {"type": "ADVISORY", "url": "https://nvd.test/CVE-1"}]}),
        json!({"id": "GHSA-a", "link": "https://github.com/advisories/a",
            "affected": [github_range("vite", ">= 8.3.0, <= 8.3.2", "8.3.3"),
                github_range("vite", "<= 6.4.3", "6.4.4"),
                // An exclusive lower bound and another fix are differences.
                github_range("vite", "> 8.2.0, <= 8.2.3", "8.2.4"),
                github_range("vite", ">=7.0.0, <7.3.7", "7.3.8")],
            "references": [{"type": "WEB", "url": "https://nvd.test/CVE-1"},
                {"type": "WEB", "url": "https://github.com/advisories/a"},
                {"type": "WEB", "url": "https://example.test/new"}]}),
    ];
    state_twin_differences(&mut rows);
    assert_eq!(
        rows[1],
        json!({"id": "GHSA-a", "link": "https://github.com/advisories/a",
            "affected": [github_range("vite", "> 8.2.0, <= 8.2.3", "8.2.4"),
                github_range("vite", ">=7.0.0, <7.3.7", "7.3.8")],
            "references": [{"type": "WEB", "url": "https://example.test/new"}]})
    );
    // Every range and reference the same: neither key is stated again.
    let mut rows = vec![
        json!({"id": "GHSA-a", "affected": [osv_range("vite", "0", "6.4.4")],
            "references": [{"type": "WEB", "url": "https://example.test/fix"}]}),
        json!({"id": "GHSA-a", "affected": [github_range("vite", "< 6.4.4", "6.4.4")],
            "references": [{"type": "ADVISORY", "url": "https://example.test/fix"}]}),
    ];
    state_twin_differences(&mut rows);
    assert_eq!(rows[1], json!({"id": "GHSA-a"}));
}
