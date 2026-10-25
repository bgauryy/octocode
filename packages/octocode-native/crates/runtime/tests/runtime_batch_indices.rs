//! Rejected rows retain their input indices and appear once while admitted evidence pages.
use crate::support::{Run as _, Workspace};
use serde_json::json;

#[tokio::test]
async fn tool_continuations_preserve_nonzero_indices_and_every_source_line() {
    let workspace = Workspace::new();
    let body = (1..=36)
        .map(|line| format!("line-{line:02} {}\n", "evidence λ ".repeat(12)))
        .collect::<String>();
    let file = workspace.write("evidence.txt", &body);
    let other = workspace.write("other.txt", "other source\n");
    let runtime = workspace.runtime(&[]);
    let mut pending = std::collections::VecDeque::from([json!({"queries":[
        {"path":other},
        {"path":file,"bogus":true},
        {"path":file}
    ],"responseChars":1200})]);
    let mut seen = std::collections::HashSet::new();
    let mut lines = Vec::new();
    let mut rejections = 0;
    let mut calls = 0;
    let mut tool_pages = 0;
    while let Some(input) = pending.pop_front() {
        calls += 1;
        assert!(calls < 100, "continuations terminate");
        let output = runtime
            .run(format!("continued-{calls}"), "localFetch".into(), input)
            .await
            .expect("admitted reads execute")
            .structured_content;
        octocode_native::contracts::validate_output("localFetch", &output)
            .expect("indexed output contract");
        for row in output["results"].as_array().expect("results") {
            if row["status"] == "error" {
                assert_eq!(row["index"], 1);
                assert_eq!(row["data"]["errorCode"], "invalidInput");
                rejections += 1;
            } else if row["data"]["path"] == "evidence.txt" {
                assert_eq!(row["index"], 2, "cursor retains originating query");
                lines.extend(
                    row["data"]["content"]
                        .as_str()
                        .expect("source content")
                        .lines()
                        .map(|line| line.split_once('\t').expect("numbered source").1.to_owned()),
                );
            }
            for call in row["data"]["next"]
                .as_object()
                .into_iter()
                .flat_map(|next| next.values())
            {
                let input = &call["input"];
                crate::support::assert_cursor_only(input);
                if seen.insert(input.to_string()) {
                    tool_pages += 1;
                    pending.push_back(input.clone());
                }
            }
        }
        if let Some(input) = output.pointer("/responsePagination/next/input") {
            crate::support::assert_cursor_only(input);
            assert!(seen.insert(input.to_string()), "response pages never cycle");
            pending.push_front(input.clone());
        }
    }
    assert!(tool_pages > 1, "walk multiple source windows");
    assert_eq!(rejections, 1, "rejected row is returned once");
    assert_eq!(
        lines,
        body.lines().collect::<Vec<_>>(),
        "no gaps or duplicates"
    );
    runtime.close().await;
}

#[tokio::test]
async fn clasify_isolates_invalid_rows_before_capturing_admitted_sources() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_BETA", "true".into()),
        ("OCTOCODE_CLASSIFICATION_API", "test-key".into()),
        (
            "OCTOCODE_CLASSIFICATION_API_HOST",
            "http://127.0.0.1:1".into(),
        ),
    ]);
    let output = runtime
        .run("mixed-clasify".into(), "clasify".into(), json!({"queries":[
            {"operation":"models","bogus":true},
            {"operation":"evaluate","source":{"tool":"localFetch","query":{"path":"missing.txt"}},
             "questions":{"q":{"type":"noul","instructions":"Does it contain evidence?"}}}
        ]}))
        .await
        .expect("invalid row does not block an admitted sibling")
        .structured_content;
    octocode_native::contracts::validate_output("clasify", &output).expect("output contract");
    let rows = output["results"].as_array().expect("indexed results");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["index"], 0);
    assert_eq!(rows[0]["data"]["errorCode"], "invalidInput");
    assert_eq!(rows[1]["index"], 1);
    assert_eq!(rows[1]["status"], "error");
    assert_ne!(rows[1]["data"]["errorCode"], "invalidInput");
    assert!(
        rows[1]["data"]["error"]
            .as_str()
            .is_some_and(|message| message.contains("missing.txt"))
    );
    runtime.close().await;
}

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
