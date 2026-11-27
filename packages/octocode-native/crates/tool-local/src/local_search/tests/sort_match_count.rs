//! `sort.by: "matchCount"` has one meaning in every view: occurrences.
use super::*;

/// The file names of a row, in the order it lists them.
fn order(body: &serde_json::Value) -> Vec<String> {
    body["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|file| {
            let path = file["path"].as_str().expect("path");
            path.rsplit('/').next().unwrap_or(path).to_owned()
        })
        .collect()
}

/// One line with ten hits outranks three lines with one hit each in every
/// view, and an ascending order is the same ranking reversed.
#[test]
fn match_count_sort_means_occurrences_in_every_view() {
    let files = [
        ("y.txt", "zeta\nzeta\nzeta\n"),
        (
            "z.txt",
            "zeta zeta zeta zeta zeta zeta zeta zeta zeta zeta\n",
        ),
    ];
    let views = [
        serde_json::json!({"view": "content"}),
        serde_json::json!({"view": "files"}),
        serde_json::json!({"view": "countLines"}),
        serde_json::json!({"view": "countMatches"}),
        serde_json::json!({"view": "matchOnly"}),
        serde_json::json!({"view": "matchOnly", "unique": "list"}),
    ];
    for (order_asked, expected) in [("desc", ["z.txt", "y.txt"]), ("asc", ["y.txt", "z.txt"])] {
        for view in &views {
            let mut fields = view.clone();
            fields["matchString"] = serde_json::json!(["zeta"]);
            fields["sort"] = serde_json::json!({"by": "matchCount", "order": order_asked});
            let body = search_fixture(&files, ls_query(fields, None));
            assert_eq!(order(&body), expected, "{view} {order_asked}: {body}");
        }
    }
}
