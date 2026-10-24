//! Rejected rows retain their input indices and appear once while admitted evidence pages.
use crate::support::{Run as _, Workspace};
use serde_json::json;

#[tokio::test]
async fn rejected_rows_and_admitted_evidence_keep_original_indices_across_pages() {
    let workspace = Workspace::new();
    let body = (0..100)
        .map(|line| format!("needle batch {line}\n"))
        .collect::<String>();
    let a = workspace.write("a.txt", &body);
    let c = workspace.write("c.txt", &body);
    let runtime = workspace.runtime(&[]);
    for budget in [1000, 50_000] {
        let mut next = Some(json!({"queries":[
            {"path":a,"matchString":["needle"]},
            {"path":a,"matchString":["needle"],"bogusField":1},
            {"path":c,"matchString":["needle"]}
        ],"responseChars":budget}));
        let mut evidence = Vec::new();
        let mut rejected = Vec::new();
        let mut pages = 0;
        while let Some(input) = next.take() {
            let output = runtime
                .run(
                    format!("batch-{budget}-{pages}"),
                    "localSearch".into(),
                    input,
                )
                .await
                .expect("valid rows execute")
                .structured_content;
            octocode_native::contracts::validate_output("localSearch", &output)
                .expect("page output contract");
            for row in output["results"].as_array().expect("indexed rows") {
                let index = row["index"].as_u64().expect("original index");
                if row["status"] == "error" {
                    assert_eq!(pages, 0, "rejection only on first page");
                    assert_eq!(row["data"]["errorCode"], "invalidInput");
                    assert!(
                        crate::support::checks::leads(row).is_empty(),
                        "no guessed repair"
                    );
                    rejected.push(index);
                } else {
                    for file in row["data"]["files"].as_array().into_iter().flatten() {
                        for hit in file["matches"].as_array().into_iter().flatten() {
                            evidence.push((index, hit["line"].as_u64().expect("line")));
                        }
                    }
                }
            }
            pages += 1;
            assert!(pages < 300, "finite response pages");
            next = output.pointer("/responsePagination/next/input").cloned();
            if let Some(input) = &next {
                crate::support::assert_cursor_only(input);
            }
        }
        assert_eq!(rejected, [1]);
        evidence.sort_unstable();
        assert_eq!(
            evidence,
            [0, 2]
                .into_iter()
                .flat_map(|index| (1..=100).map(move |line| (index, line)))
                .collect::<Vec<_>>()
        );
        assert_eq!(pages > 1, budget == 1000);
    }
    runtime.close().await;
}
