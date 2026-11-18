//! A pull request row split across its sections: the walk of a section
//! (`next.<section>` on the pull request) rides the row's last part only,
//! and no part shows an empty list for a section whose items another part
//! delivers; a section that is truly empty stays `[]`.
use super::part_metadata::parts;
use crate::id::ToolId;
use serde_json::{Value, json};

fn pull_request(files: usize, reviews: usize) -> Value {
    let files = (0..files)
        .map(|at| format!("M +{at} -0 packages/vite/src/node/module-{at:03}.ts"))
        .collect::<Vec<_>>();
    let reviews = (0..reviews)
        .map(|at| json!({"id": format!("{}", 2_013_197_887 + at), "author": "reviewer", "state": "COMMENTED", "submittedAt": "2024-04-21T00:40:27Z"}))
        .collect::<Vec<_>>();
    json!({"pullRequests": [{
        "number": 16471, "state": "merged", "headCommitSha": "9d421b9f57affb6fe526c5609397054273faf50e",
        "files": files, "reviews": reviews, "comments": [],
        "sectionPagination": {"reviews": {"unit": "items", "offset": 0, "length": reviews.len(), "hasMore": true}},
        "next": {"reviews": {"kind": "continue", "tool": "ghGetHistoryItem", "input": {"cursor": "cursor2.reviews"}}}
    }]})
}

#[test]
fn a_section_walk_rides_the_last_part_and_no_part_shows_a_false_empty_section() {
    let data = pull_request(60, 40);
    let mut split = false;
    for budget in (600..=3_000).step_by(200) {
        let parts = parts(ToolId::GhGetHistoryItem, &data, budget);
        split |= parts.len() > 1;
        let walks = parts
            .iter()
            .enumerate()
            .filter(|(_, part)| part.pointer("/data/pullRequests/0/next/reviews").is_some())
            .map(|(at, _)| at)
            .collect::<Vec<_>>();
        assert_eq!(
            walks,
            [parts.len() - 1],
            "budget {budget}: the walk rides the last part"
        );
        for part in &parts {
            let pr = &part["data"]["pullRequests"][0];
            for section in ["files", "reviews"] {
                assert_ne!(
                    pr.get(section),
                    Some(&json!([])),
                    "budget {budget}: {section} in {part}"
                );
            }
        }
        let shown = |section: &str| {
            parts
                .iter()
                .filter_map(|part| part["data"]["pullRequests"][0][section].as_array())
                .map(Vec::len)
                .sum::<usize>()
        };
        assert_eq!(
            (shown("files"), shown("reviews")),
            (60, 40),
            "budget {budget}: every item once"
        );
        assert!(
            parts
                .iter()
                .any(|part| part["data"]["pullRequests"][0]["comments"] == json!([])),
            "budget {budget}: a truly empty section stays []"
        );
    }
    assert!(split, "some budget splits the row");
}
