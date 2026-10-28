use super::*;
use tokio_util::sync::CancellationToken;

#[test]
fn every_code_fragment_keeps_optional_provenance_and_evidence_qualifiers() {
    let lines = (0..30)
        .map(|line| json!(format!("{line}\t{}", "evidence ".repeat(20))))
        .collect::<Vec<_>>();
    let identity = json!({"owner":"owner","repo":"repo","path":"source.rs",
        "commitSha":"f".repeat(40),"atRef":false,"lineResolved":false});
    let mut file = identity.clone();
    file["lines"] = json!(lines);
    let plan = PreparedRows::new(
        vec![json!({"index":0,"data":{"files":[file]}})],
        &[None],
        Some(crate::id::ToolId::GhSearchCode),
    )
    .plan(1000, &CancellationToken::new())
    .expect("valid plan");
    assert!(plan.total_pages > 1);
    let mut seen = Vec::new();
    for page in 0..plan.total_pages {
        for row in plan
            .build_page(page, &[None], &[], &CancellationToken::new())
            .expect("valid build_page")
        {
            for file in row["data"]["files"].as_array().expect("files") {
                for (key, expected) in identity.as_object().expect("identity") {
                    assert_eq!(&file[key], expected, "page {page}: missing {key}");
                }
                seen.extend(file["lines"].as_array().expect("lines").iter().cloned());
            }
        }
    }
    assert_eq!(seen, lines);
}

#[test]
fn rows_scope_splits_nested_arrays_into_complete_json_pages() {
    let matches = (0..200)
        .map(|line| json!({"line":line,"value":"x".repeat(40)}))
        .collect::<Vec<_>>();
    let structured = json!({"results":[{"index":0,"data":{
        "stats":{"total":400},
        "files":[{"path":"a","matches":matches.clone()},{"path":"b","matches":matches}]
    }}]});
    let options = ResponsePageOptions {
        response_chars: Some(2000),
        paging: true,
        ..Default::default()
    };
    let (first, pagination) = paginate_rows(
        structured.as_object().unwrap().clone(),
        &options,
        Vec::new(),
        &[],
        None,
    );
    assert!(pagination.total_pages > 2 && pagination.has_more);
    let row = &first["results"][0];
    assert_eq!(
        row["data"]["stats"]["total"], 400,
        "non-split fields are kept"
    );
    assert_eq!(row["data"]["files"].as_array().unwrap().len(), 1);
    assert!(
        octocode_engine::json_size::utf16_units(&Value::Object(first.clone())).unwrap_or(0)
            <= 2000 + 200
    );
    let mut seen = 0;
    for page in 0..pagination.total_pages {
        let options = ResponsePageOptions {
            response_offset: Some(page),
            response_snapshot: Some(pagination.snapshot.clone()),
            ..options.clone()
        };
        let (envelope, _) = paginate_rows(
            structured.as_object().unwrap().clone(),
            &options,
            Vec::new(),
            &[],
            None,
        );
        for row in envelope["results"].as_array().unwrap() {
            for file in row["data"]["files"].as_array().unwrap() {
                seen += file["matches"].as_array().unwrap().len();
            }
        }
    }
    assert_eq!(seen, 400, "every match appears exactly once");
}

#[test]
fn later_slices_of_an_element_do_not_repeat_its_description() {
    let comments = (0..80)
        .map(|n| json!({"author":"a","body":format!("comment {n} {}", "x".repeat(60))}))
        .collect::<Vec<_>>();
    let body = "B".repeat(600);
    let structured = json!({"results":[{"index":0,"data":{
        "pullRequests":[{"number":7,"title":"t","state":"open","body":body,
            "headSha":"h","comments":comments,"reviews":[{"author":"r","state":"approved"}]}]
    }}]});
    let options = ResponsePageOptions {
        response_chars: Some(3_000),
        paging: true,
        ..Default::default()
    };
    let mut offset = 0;
    let mut bodies = 0;
    let mut seen = 0;
    loop {
        let page = ResponsePageOptions {
            response_offset: Some(offset),
            response_snapshot: (offset > 0).then(|| {
                paginate_rows(
                    structured.as_object().unwrap().clone(),
                    &options,
                    Vec::new(),
                    &[],
                    Some(crate::id::ToolId::GhGetHistoryItem),
                )
                .1
                .snapshot
            }),
            ..options.clone()
        };
        let (envelope, pagination) = paginate_rows(
            structured.as_object().unwrap().clone(),
            &page,
            Vec::new(),
            &[],
            Some(crate::id::ToolId::GhGetHistoryItem),
        );
        let pr = &envelope["results"][0]["data"]["pullRequests"][0];
        assert_eq!(pr["number"], 7, "{envelope:?}");
        bodies += usize::from(pr.get("body").is_some());
        seen += pr["comments"].as_array().map_or(0, Vec::len);
        assert_ne!(pr.get("reviews"), Some(&json!([])), "no emptied list: {pr}");
        if !pagination.has_more {
            break;
        }
        offset += 1;
    }
    assert_eq!(seen, 80);
    assert_eq!(bodies, 1, "the body rides one part");
}

/// Every secondary collection is delivered once, even when it is too small
/// to trigger a separate page of its own.
#[test]
fn rows_scope_never_repeats_secondary_evidence_per_fragment() {
    let commits = (0..100)
        .map(|n| json!({"sha":format!("{n:040}"),"messageHeadline":"x".repeat(40)}))
        .collect::<Vec<_>>();
    for count in [1, 8, 100] {
        for files in [
            (0..count)
                .map(|n| json!(format!("M +1 -1 src/file_{n:03}.rs")))
                .collect::<Vec<_>>(),
            (0..count).map(|n| json!(n)).collect::<Vec<_>>(),
        ] {
            let structured = json!({"results":[{"index":0,"data":{
                "commits":commits,"files":files
            }}]});
            let options = ResponsePageOptions {
                response_chars: Some(6_000),
                paging: true,
                ..Default::default()
            };
            let (_, first) = paginate_rows(
                structured.as_object().unwrap().clone(),
                &options,
                Vec::new(),
                &[],
                None,
            );
            assert!(first.total_pages > 1);
            let (mut actual_commits, mut actual_files) = (Vec::new(), Vec::new());
            for page in 0..first.total_pages {
                let options = ResponsePageOptions {
                    response_offset: Some(page),
                    response_snapshot: Some(first.snapshot.clone()),
                    ..options.clone()
                };
                let (envelope, _) = paginate_rows(
                    structured.as_object().unwrap().clone(),
                    &options,
                    Vec::new(),
                    &[],
                    None,
                );
                for row in envelope["results"].as_array().unwrap() {
                    actual_commits.extend(
                        row["data"]["commits"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .cloned(),
                    );
                    actual_files.extend(
                        row["data"]["files"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .cloned(),
                    );
                }
            }
            assert_eq!(actual_commits, commits);
            assert_eq!(actual_files, files);
        }
    }
}

/// A fitting skeleton does not justify oversized batching to reduce page count.
#[test]
fn rows_scope_preserves_dominant_context_with_complete_bounded_pages() {
    let matches = (0..30)
        .map(|line| json!({"line":line,"value":"x".repeat(20)}))
        .collect::<Vec<_>>();
    let structured = json!({"results":[{"index":0,"data":{
        "stats":{"note":"s".repeat(300)},
        "files":[{"path":"a","matches":matches.clone()}]
    }}]});
    let options = ResponsePageOptions {
        response_chars: Some(600),
        paging: true,
        ..Default::default()
    };
    let (_, pagination) = paginate_rows(
        structured.as_object().unwrap().clone(),
        &options,
        Vec::new(),
        &[],
        None,
    );
    let mut seen = Vec::new();
    assert!(pagination.total_pages > 1);
    for page in 0..pagination.total_pages {
        let (envelope, current) = paginate_rows(
            structured.as_object().unwrap().clone(),
            &ResponsePageOptions {
                response_offset: Some(page),
                response_snapshot: Some(pagination.snapshot.clone()),
                ..options.clone()
            },
            Vec::new(),
            &[],
            None,
        );
        let envelope = Value::Object(envelope);
        assert!(octocode_engine::json_size::utf16_units(&envelope).unwrap() <= 600);
        assert_eq!(current.total_pages, pagination.total_pages);
        assert_eq!(current.has_more, page + 1 < pagination.total_pages);
        for row in envelope["results"].as_array().unwrap() {
            assert_eq!(
                row["data"]["stats"],
                structured["results"][0]["data"]["stats"]
            );
            for file in row["data"]["files"].as_array().unwrap() {
                seen.extend(file["matches"].as_array().unwrap().iter().cloned());
            }
        }
    }
    assert_eq!(seen, matches);
}
