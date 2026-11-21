//! A row over its page is cut wherever a cut fits it: free text no longer
//! than a page moves out of a part that holds it only over the page, and
//! the fields a row states once go alone when the row's first item does not
//! fit beside them. The oracle is the page size and the unsplit row.
use super::evidence_closure::chars;
use super::required_text::{assert_joins_to, walk_and_join};
use super::*;
use crate::id::ToolId;

/// One advisory as `securityCheck` `view:"details"` shapes it, its
/// `details` text `text` chars long.
fn advisory(id: usize, text: usize) -> Value {
    let references = (0..6)
        .map(|n| json!({"type": "WEB", "url": format!("https://example.test/advisories/{id}/reference/{n}")}))
        .collect::<Vec<_>>();
    json!({
        "id": format!("GHSA-0000-0000-{id:04}"),
        "aliases": [format!("CVE-2026-{id:04}")],
        "database": "OSV",
        "source": format!("https://example.test/advisory-database/github-reviewed/GHSA-0000-0000-{id:04}.json"),
        "severity": "moderate",
        "cvss": [{"type": "CVSS_V3", "vector": "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:L"}],
        "cwes": ["CWE-1333", "CWE-400"],
        "summary": "Regular expression denial of service in the trim functions",
        "details": "All versions before the fix are affected. ".repeat(text / 42 + 1)[..text],
        "publishedAt": "2022-01-06T20:30:46Z",
        "modifiedAt": "2025-09-29T21:12:31Z",
        "alsoAffects": ["demo-es", "demo.trim", "demo.trimend"],
        "affected": [{"packageName": "demo", "ecosystem": "npm", "purl": "pkg:npm/demo",
            "ranges": [{"type": "SEMVER", "events": [{"introduced": "4.0.0"}, {"fixed": "4.17.21"}]}]}],
        "references": references,
    })
}

/// A details row: about 900 chars of fields stated once, then advisories of
/// about 800 chars without their text, each text shorter than any page
/// below.
pub(in crate::response) fn details_row() -> Value {
    json!({
        "answer": "demo@4.17.20 is affected: 3 known advisories. CVEs: CVE-2026-0001, CVE-2026-0002, CVE-2026-0003. Upgrade to 4.18.1 to clear every one listed.",
        "affected": true,
        "upgradeTo": "4.18.1",
        "upgradeNote": "Skipped 4.18.0 (OSV lists GHSA-0000-0000-0009 at it).",
        "sources": [{"source": "osv", "status": "found"},
            {"source": "github", "status": "found", "readOn": "2026-11-21"}],
        "advisories": [advisory(1, 800), advisory(2, 1_200), advisory(3, 570)],
        "latestVersion": "4.18.1",
        "latestPublishedAt": "2026-04-01",
        "currentPublishedAt": "2020-08-13",
        "outdated": true,
        "upgradeKind": "minor",
        "repository": {"owner": "demo", "repo": "demo", "stars": 61_327, "forks": 7_342,
            "archived": false, "fork": false, "defaultBranch": "main", "readOn": "2026-11-21",
            "pushedAt": "2026-10-01", "license": "NOASSERTION", "provenance": "claimed"},
        "warnings": ["GitHub withdrew GHSA-0000-0000-0008 and has not reviewed it, so it is not counted."],
    })
}

/// Every piece of this row fits each page alone: the fields stated once,
/// an advisory without its text, each text. So no part outgrows its page by
/// more than the fixed parts one more part would repeat (the size of a later
/// part without its advisories), and the parts join back to the row.
#[test]
fn a_part_whose_pieces_each_fit_the_page_outgrows_it_by_no_more_than_a_parts_fixed_cost() {
    let data = details_row();
    let mut stated_once = data.clone();
    stated_once["advisories"] = json!([]);
    let mut advisory = data["advisories"][1].clone();
    let text = chars(&advisory["details"]);
    advisory["details"] = json!("");
    advisory["references"] = json!([]);
    let largest = chars(&json!({"index": 0, "data": stated_once}))
        .max(chars(&advisory))
        .max(text);
    for budget in [largest + 400, largest + 700, largest * 2, largest * 3] {
        let (parts, joined) = walk_and_join(ToolId::SecurityCheck, &data, budget);
        assert_joins_to(&joined, &data, &format!("budget {budget}"));
        let fixed = parts[1..]
            .iter()
            .map(|part| chars(part) - chars(&part["data"]["advisories"]))
            .max()
            .expect("the row is split");
        for (at, part) in parts.iter().enumerate() {
            assert!(
                chars(part) <= budget + fixed,
                "budget {budget}: part {} of {} holds {} chars, {fixed} of them fixed: {part}",
                at + 1,
                parts.len(),
                chars(part)
            );
        }
    }
}

/// R4: a text shorter than a page is never cut. A part shows it whole or
/// empty; the part that delivers it after the one that showed it empty
/// names it (`rowPart.text`), so the join is defined.
#[test]
fn a_text_shorter_than_a_page_is_never_cut() {
    let data = details_row();
    let texts = data["advisories"]
        .as_array()
        .expect("advisories")
        .iter()
        .map(|advisory| advisory["details"].clone())
        .collect::<Vec<_>>();
    let longest = texts.iter().map(chars).max().expect("texts");
    let mut moved = false;
    for budget in (longest + 300..longest * 3).step_by(50) {
        let (parts, joined) = walk_and_join(ToolId::SecurityCheck, &data, budget);
        assert_joins_to(&joined, &data, &format!("budget {budget}"));
        for part in &parts {
            assert!(
                part["data"]["answer"] == "" || part["data"]["answer"] == data["answer"],
                "budget {budget}: {part}"
            );
            for advisory in part["data"]["advisories"].as_array().into_iter().flatten() {
                let Some(shown) = advisory.get("details") else {
                    continue;
                };
                assert!(
                    shown == "" || texts.contains(shown),
                    "budget {budget}: a text is cut: {part}"
                );
                moved |= part["rowPart"]["text"]
                    .as_str()
                    .is_some_and(|pointer| pointer.ends_with("/details"))
                    && shown != "";
            }
        }
    }
    assert!(moved, "some budget moves a text to a later part");
}
